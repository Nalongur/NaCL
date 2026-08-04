use crate::auth::{self, AuthError};
use crate::data::{load_or_create_settings, AppPaths, DataError};
use crate::instance::{
    self, InstallationState, InstanceConfig, InstanceError, JavaSelection, WindowMode,
};
use crate::java::{self, JavaRuntime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;

const PROFILE_SCHEMA_VERSION: u32 = 1;
const LAUNCHER_NAME: &str = "NaCL";
const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineProfile {
    pub schema_version: u32,
    pub username: String,
    pub uuid: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchGameRequest {
    pub instance_id: String,
    #[serde(default)]
    pub account: LaunchAccountKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchAccountKind {
    Microsoft,
    #[default]
    Offline,
}

#[derive(Debug, Clone)]
struct LaunchIdentity {
    username: String,
    uuid: String,
    access_token: String,
    client_id: String,
    xuid: String,
    user_type: &'static str,
    mode_label: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub instance_id: String,
    pub process_id: u32,
    pub started_epoch_ms: u128,
    pub log_file: PathBuf,
}

pub struct LaunchedGame {
    pub result: LaunchResult,
    pub child: Child,
}

#[derive(Debug)]
pub enum LaunchError {
    Auth(AuthError),
    Data(DataError),
    Instance(InstanceError),
    MissingProfile,
    InvalidUsername,
    InstanceNotReady,
    MissingVersionMetadata(PathBuf),
    MissingClient(PathBuf),
    MissingLibrary(PathBuf),
    InvalidMetadata {
        path: PathBuf,
        source: serde_json::Error,
    },
    MissingMainClass,
    MissingArguments,
    JavaUnavailable(u16),
    JavaVersionMismatch {
        expected: u16,
        actual: Option<u16>,
    },
    InvalidCustomArguments,
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Archive {
        path: PathBuf,
        source: zip::result::ZipError,
    },
}

impl fmt::Display for LaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth(error) => error.fmt(formatter),
            Self::Data(error) => error.fmt(formatter),
            Self::Instance(error) => error.fmt(formatter),
            Self::MissingProfile => write!(formatter, "请先创建离线档案"),
            Self::InvalidUsername => write!(
                formatter,
                "离线用户名须为 3–16 位，只能包含英文字母、数字和下划线"
            ),
            Self::InstanceNotReady => write!(formatter, "实例尚未安装完整"),
            Self::MissingVersionMetadata(path) => {
                write!(formatter, "找不到版本元数据 {}", path.display())
            }
            Self::MissingClient(path) => write!(formatter, "找不到游戏客户端 {}", path.display()),
            Self::MissingLibrary(path) => write!(formatter, "缺少游戏依赖 {}", path.display()),
            Self::InvalidMetadata { path, source } => {
                write!(formatter, "版本元数据 {} 无效：{source}", path.display())
            }
            Self::MissingMainClass => write!(formatter, "版本元数据缺少主类"),
            Self::MissingArguments => write!(formatter, "版本元数据缺少启动参数"),
            Self::JavaUnavailable(major) => {
                write!(formatter, "未找到兼容的 Java {major} 运行环境")
            }
            Self::JavaVersionMismatch { expected, actual } => write!(
                formatter,
                "所选 Java 版本不兼容：需要 Java {expected}，当前为 {}",
                actual.map_or_else(|| "未知版本".to_string(), |value| format!("Java {value}"))
            ),
            Self::InvalidCustomArguments => write!(formatter, "自定义启动参数包含未闭合的引号"),
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "无法{action} {}：{source}", path.display()),
            Self::Archive { path, source } => {
                write!(formatter, "无法解压原生库 {}：{source}", path.display())
            }
        }
    }
}

impl std::error::Error for LaunchError {}

impl From<DataError> for LaunchError {
    fn from(value: DataError) -> Self {
        Self::Data(value)
    }
}

impl From<AuthError> for LaunchError {
    fn from(value: AuthError) -> Self {
        Self::Auth(value)
    }
}

impl From<InstanceError> for LaunchError {
    fn from(value: InstanceError) -> Self {
        Self::Instance(value)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionMetadata {
    #[serde(default)]
    main_class: String,
    #[serde(rename = "type", default)]
    version_type: String,
    asset_index: AssetIndexReference,
    #[serde(default)]
    java_version: Option<JavaVersionMetadata>,
    #[serde(default)]
    libraries: Vec<Library>,
    #[serde(default)]
    arguments: Option<VersionArguments>,
    #[serde(default)]
    minecraft_arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JavaVersionMetadata {
    major_version: u16,
}

#[derive(Debug, Deserialize)]
struct AssetIndexReference {
    #[serde(default)]
    id: String,
}

#[derive(Debug, Deserialize)]
struct VersionArguments {
    #[serde(default)]
    game: Vec<ArgumentEntry>,
    #[serde(default)]
    jvm: Vec<ArgumentEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ArgumentEntry {
    Plain(String),
    Conditional {
        #[serde(default)]
        rules: Vec<Rule>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ArgumentValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct Library {
    name: String,
    downloads: LibraryDownloads,
    #[serde(default)]
    natives: HashMap<String, String>,
    #[serde(default)]
    rules: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
struct LibraryDownloads {
    #[serde(default)]
    artifact: Option<Artifact>,
    #[serde(default)]
    classifiers: HashMap<String, Artifact>,
}

#[derive(Debug, Clone, Deserialize)]
struct Artifact {
    #[serde(default)]
    path: String,
}

#[derive(Debug, Deserialize)]
struct Rule {
    action: String,
    #[serde(default)]
    os: Option<RuleOs>,
    #[serde(default)]
    features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Deserialize)]
struct RuleOs {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arch: Option<String>,
}

pub fn load_offline_profile() -> Result<Option<OfflineProfile>, LaunchError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    load_offline_profile_in(&paths)
}

pub fn save_offline_profile(username: String) -> Result<OfflineProfile, LaunchError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    save_offline_profile_in(&paths, username)
}

pub fn launch_game(request: LaunchGameRequest) -> Result<LaunchedGame, LaunchError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    let identity = match request.account {
        LaunchAccountKind::Microsoft => {
            let online = auth::online_launch_identity()?;
            LaunchIdentity {
                username: online.username,
                uuid: online.uuid,
                access_token: online.access_token,
                client_id: online.client_id,
                xuid: online.xuid,
                user_type: "msa",
                mode_label: "Microsoft",
            }
        }
        LaunchAccountKind::Offline => {
            let profile = load_offline_profile_in(&paths)?.ok_or(LaunchError::MissingProfile)?;
            LaunchIdentity {
                username: profile.username,
                uuid: profile.uuid,
                access_token: "0".to_string(),
                client_id: String::new(),
                xuid: String::new(),
                user_type: "legacy",
                mode_label: "offline",
            }
        }
    };
    let instance = instance::list_instances_in(&paths)?
        .into_iter()
        .find(|candidate| candidate.id == request.instance_id)
        .ok_or_else(|| InstanceError::InstanceNotFound(request.instance_id.clone()))?;
    launch_game_in(&paths, instance, identity)
}

fn load_offline_profile_in(paths: &AppPaths) -> Result<Option<OfflineProfile>, LaunchError> {
    let path = profile_path(paths);
    if !path.is_file() {
        return Ok(None);
    }
    let contents =
        fs::read_to_string(&path).map_err(|source| io_error("读取离线档案", &path, source))?;
    let profile = serde_json::from_str::<OfflineProfile>(&contents).map_err(|source| {
        LaunchError::InvalidMetadata {
            path: path.clone(),
            source,
        }
    })?;
    if profile.schema_version != PROFILE_SCHEMA_VERSION || !valid_username(&profile.username) {
        return Err(LaunchError::InvalidUsername);
    }
    Ok(Some(profile))
}

fn save_offline_profile_in(
    paths: &AppPaths,
    username: String,
) -> Result<OfflineProfile, LaunchError> {
    let username = username.trim().to_string();
    if !valid_username(&username) {
        return Err(LaunchError::InvalidUsername);
    }
    let profile = OfflineProfile {
        schema_version: PROFILE_SCHEMA_VERSION,
        uuid: offline_uuid(&username),
        username,
    };
    let path = profile_path(paths);
    let bytes =
        serde_json::to_vec_pretty(&profile).map_err(|source| LaunchError::InvalidMetadata {
            path: path.clone(),
            source,
        })?;
    let mut temporary = NamedTempFile::new_in(&paths.config_dir)
        .map_err(|source| io_error("创建离线档案临时文件", &paths.config_dir, source))?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.write_all(b"\n"))
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|source| io_error("写入离线档案", temporary.path(), source))?;
    temporary
        .persist(&path)
        .map_err(|error| io_error("保存离线档案", &path, error.error))?;
    Ok(profile)
}

fn profile_path(paths: &AppPaths) -> PathBuf {
    paths.config_dir.join("offline-profile.json")
}

fn valid_username(value: &str) -> bool {
    (3..=16).contains(&value.len())
        && value
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || character == b'_')
}

fn offline_uuid(username: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes.iter().map(|value| format!("{value:02x}")).collect()
}

fn launch_game_in(
    paths: &AppPaths,
    instance: InstanceConfig,
    identity: LaunchIdentity,
) -> Result<LaunchedGame, LaunchError> {
    if instance.installation.state != InstallationState::Ready {
        return Err(LaunchError::InstanceNotReady);
    }
    let metadata_id = instance
        .loader
        .profile_id
        .as_deref()
        .unwrap_or(&instance.game_version);
    let metadata_path = paths.versions_dir.join(format!("{metadata_id}.json"));
    if !metadata_path.is_file() {
        return Err(LaunchError::MissingVersionMetadata(metadata_path));
    }
    let metadata_contents = fs::read_to_string(&metadata_path)
        .map_err(|source| io_error("读取版本元数据", &metadata_path, source))?;
    let metadata: VersionMetadata = serde_json::from_str(&metadata_contents).map_err(|source| {
        LaunchError::InvalidMetadata {
            path: metadata_path.clone(),
            source,
        }
    })?;
    if metadata.main_class.is_empty() {
        return Err(LaunchError::MissingMainClass);
    }

    let profile_client_path = paths.versions_dir.join(format!("{metadata_id}.jar"));
    let client_path = if profile_client_path.is_file() {
        profile_client_path
    } else {
        paths
            .versions_dir
            .join(format!("{}.jar", instance.game_version))
    };
    if !client_path.is_file() {
        return Err(LaunchError::MissingClient(client_path));
    }
    let required_java = metadata
        .java_version
        .as_ref()
        .map_or(8, |java| java.major_version);
    let java = select_java(&instance, required_java)?;
    let instance_root = paths.instances_dir.join(&instance.id);
    let game_directory = instance_root.join(&instance.game_directory);
    fs::create_dir_all(&game_directory)
        .map_err(|source| io_error("创建游戏目录", &game_directory, source))?;
    let natives_directory = instance_root.join(".nacl").join("natives");
    prepare_natives(paths, &metadata.libraries, &natives_directory)?;

    let mut classpath = Vec::new();
    for library in &metadata.libraries {
        if !rules_allow(&library.rules, true)
            || (is_native_artifact(&library.name)
                && !native_artifact_for_current_arch(&library.name))
        {
            continue;
        }
        if let Some(artifact) = &library.downloads.artifact {
            let path = checked_library_path(paths, &artifact.path)?;
            classpath.push(path);
        }
    }
    classpath.push(client_path);
    let classpath_value = std::env::join_paths(&classpath)
        .map_err(|source| {
            io_error(
                "生成 classpath",
                &paths.libraries_dir,
                io::Error::other(source),
            )
        })?
        .to_string_lossy()
        .into_owned();

    let replacements = HashMap::from([
        ("${auth_player_name}", identity.username.clone()),
        ("${version_name}", metadata_id.to_owned()),
        (
            "${game_directory}",
            game_directory.to_string_lossy().into_owned(),
        ),
        (
            "${assets_root}",
            paths.assets_dir.to_string_lossy().into_owned(),
        ),
        ("${assets_index_name}", metadata.asset_index.id.clone()),
        ("${auth_uuid}", identity.uuid.clone()),
        ("${auth_access_token}", identity.access_token.clone()),
        ("${clientid}", identity.client_id.clone()),
        ("${auth_xuid}", identity.xuid.clone()),
        ("${user_properties}", "{}".to_string()),
        ("${user_type}", identity.user_type.to_string()),
        (
            "${version_type}",
            if metadata.version_type.is_empty() {
                "release".to_string()
            } else {
                metadata.version_type.clone()
            },
        ),
        (
            "${natives_directory}",
            natives_directory.to_string_lossy().into_owned(),
        ),
        ("${launcher_name}", LAUNCHER_NAME.to_string()),
        ("${launcher_version}", LAUNCHER_VERSION.to_string()),
        ("${classpath}", classpath_value.clone()),
        (
            "${classpath_separator}",
            if cfg!(windows) { ";" } else { ":" }.to_string(),
        ),
        (
            "${library_directory}",
            paths.libraries_dir.to_string_lossy().into_owned(),
        ),
        ("${resolution_width}", instance.display.width.to_string()),
        ("${resolution_height}", instance.display.height.to_string()),
    ]);

    let mut jvm_arguments = vec![
        format!("-Xms{}M", instance.memory.minimum_mb),
        format!("-Xmx{}M", instance.memory.maximum_mb),
        "-Dfile.encoding=UTF-8".to_string(),
    ];
    let mut game_arguments = Vec::new();
    if let Some(arguments) = &metadata.arguments {
        jvm_arguments.extend(expand_arguments(&arguments.jvm, &replacements, true));
        game_arguments.extend(expand_arguments(&arguments.game, &replacements, true));
        if let Some(arguments) = &metadata.minecraft_arguments {
            game_arguments.extend(
                split_custom_arguments(arguments)?
                    .into_iter()
                    .map(|value| replace_placeholders(&value, &replacements)),
            );
        }
        if !jvm_arguments
            .iter()
            .any(|argument| argument == "${classpath}" || argument == &classpath_value)
        {
            jvm_arguments.extend(
                [
                    "-Djava.library.path=${natives_directory}",
                    "-cp",
                    "${classpath}",
                ]
                .map(|value| replace_placeholders(value, &replacements)),
            );
        }
    } else if let Some(arguments) = &metadata.minecraft_arguments {
        game_arguments.extend(
            split_custom_arguments(arguments)?
                .into_iter()
                .map(|value| replace_placeholders(&value, &replacements)),
        );
        jvm_arguments.extend(
            [
                "-Djava.library.path=${natives_directory}",
                "-cp",
                "${classpath}",
            ]
            .map(|value| replace_placeholders(value, &replacements)),
        );
    } else {
        return Err(LaunchError::MissingArguments);
    }
    jvm_arguments.extend(split_custom_arguments(&instance.advanced.jvm_arguments)?);
    game_arguments.extend(split_custom_arguments(&instance.advanced.game_arguments)?);
    if instance.display.mode == WindowMode::Fullscreen {
        game_arguments.push("--fullscreen".to_string());
    }

    let started_epoch_ms = now_epoch_ms();
    let log_file = paths
        .logs_dir
        .join(format!("game-{}-{started_epoch_ms}.log", instance.id));
    let mut log = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&log_file)
        .map_err(|source| io_error("创建游戏日志", &log_file, source))?;
    let java_executable = launch_java_executable(&java.path);
    writeln!(
        log,
        "NaCL {} launch: instance={} version={} player={} java={}",
        identity.mode_label,
        instance.id,
        instance.game_version,
        identity.username,
        java_executable.display()
    )
    .map_err(|source| io_error("写入游戏日志", &log_file, source))?;
    let stderr = log
        .try_clone()
        .map_err(|source| io_error("打开游戏错误日志", &log_file, source))?;

    let mut command = Command::new(&java_executable);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .args(&jvm_arguments)
        .arg(&metadata.main_class)
        .args(&game_arguments)
        .current_dir(&game_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr));
    let child = command
        .spawn()
        .map_err(|source| io_error("启动 Java 进程", &java_executable, source))?;
    let result = LaunchResult {
        instance_id: instance.id,
        process_id: child.id(),
        started_epoch_ms,
        log_file,
    };
    Ok(LaunchedGame { result, child })
}

fn launch_java_executable(java_path: &str) -> PathBuf {
    let configured = PathBuf::from(java_path);
    #[cfg(target_os = "windows")]
    {
        let javaw = configured.with_file_name("javaw.exe");
        if javaw.is_file() {
            return javaw;
        }
    }
    configured
}

fn select_java(instance: &InstanceConfig, required_major: u16) -> Result<JavaRuntime, LaunchError> {
    match &instance.java {
        JavaSelection::Custom { path } => {
            let runtime =
                java::inspect_java_runtime(path).map_err(|_| LaunchError::JavaVersionMismatch {
                    expected: required_major,
                    actual: None,
                })?;
            if runtime.major_version != Some(required_major) {
                return Err(LaunchError::JavaVersionMismatch {
                    expected: required_major,
                    actual: runtime.major_version,
                });
            }
            Ok(runtime)
        }
        JavaSelection::Auto => {
            let settings = load_or_create_settings(&AppPaths::resolve()?)?;
            let mut runtimes = java::detect_java_runtimes();
            if let Some(preferred) = settings.preferred_java_path {
                runtimes.sort_by_key(|runtime| Path::new(&runtime.path) != preferred);
            }
            runtimes
                .into_iter()
                .find(|runtime| runtime.major_version == Some(required_major))
                .ok_or(LaunchError::JavaUnavailable(required_major))
        }
    }
}

fn checked_library_path(paths: &AppPaths, relative: &str) -> Result<PathBuf, LaunchError> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(LaunchError::MissingLibrary(
            paths.libraries_dir.join(relative),
        ));
    }
    let path = paths.libraries_dir.join(relative);
    if !path.is_file() {
        return Err(LaunchError::MissingLibrary(path));
    }
    Ok(path)
}

fn prepare_natives(
    paths: &AppPaths,
    libraries: &[Library],
    destination: &Path,
) -> Result<(), LaunchError> {
    if destination.is_dir() {
        fs::remove_dir_all(destination)
            .map_err(|source| io_error("清理旧原生库", destination, source))?;
    }
    fs::create_dir_all(destination)
        .map_err(|source| io_error("创建原生库目录", destination, source))?;

    for library in libraries {
        if !rules_allow(&library.rules, true) {
            continue;
        }
        let mut native_artifacts: Vec<(&Artifact, NativeGroup)> = Vec::new();
        if native_artifact_for_current_arch(&library.name) {
            if let Some(artifact) = &library.downloads.artifact {
                native_artifacts.push((artifact, native_group(&library.name)));
            }
        }
        if let Some(classifier) = library.natives.get("windows") {
            let classifier = classifier.replace("${arch}", windows_arch());
            if let Some(artifact) = library.downloads.classifiers.get(&classifier) {
                native_artifacts.push((artifact, NativeGroup::Root));
            }
        }
        for (artifact, group) in native_artifacts {
            let archive_path = checked_library_path(paths, &artifact.path)?;
            extract_native_archive(&archive_path, destination, group)?;
        }
    }
    for group in ["java", "jna", "lwjgl", "netty"] {
        fs::create_dir_all(destination.join(group))
            .map_err(|source| io_error("创建原生库分组目录", destination, source))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum NativeGroup {
    Root,
    Java,
    Lwjgl,
}

fn native_group(library_name: &str) -> NativeGroup {
    if library_name.starts_with("org.lwjgl:") {
        NativeGroup::Lwjgl
    } else {
        NativeGroup::Java
    }
}

fn extract_native_archive(
    archive_path: &Path,
    destination: &Path,
    group: NativeGroup,
) -> Result<(), LaunchError> {
    let file =
        File::open(archive_path).map_err(|source| io_error("读取原生库", archive_path, source))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|source| LaunchError::Archive {
        path: archive_path.to_path_buf(),
        source,
    })?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|source| LaunchError::Archive {
                path: archive_path.to_path_buf(),
                source,
            })?;
        if entry.is_dir() {
            continue;
        }
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        let Some(file_name) = enclosed.file_name() else {
            continue;
        };
        let name = file_name.to_string_lossy();
        if !name.to_ascii_lowercase().ends_with(".dll") {
            continue;
        }
        let group_directory = match group {
            NativeGroup::Root => destination.to_path_buf(),
            NativeGroup::Java => destination.join("java"),
            NativeGroup::Lwjgl => destination.join("lwjgl"),
        };
        fs::create_dir_all(&group_directory)
            .map_err(|source| io_error("创建原生库分组目录", &group_directory, source))?;
        let output = group_directory.join(file_name);
        let mut output_file =
            File::create(&output).map_err(|source| io_error("创建原生库文件", &output, source))?;
        io::copy(&mut entry, &mut output_file)
            .map_err(|source| io_error("解压原生库文件", &output, source))?;
        if !matches!(group, NativeGroup::Root) {
            let root_output = destination.join(file_name);
            fs::copy(&output, &root_output)
                .map_err(|source| io_error("复制原生库兼容文件", &root_output, source))?;
        }
    }
    Ok(())
}

fn native_artifact_for_current_arch(name: &str) -> bool {
    let Some(classifier) = name.rsplit(':').next() else {
        return false;
    };
    match std::env::consts::ARCH {
        "x86" => classifier == "natives-windows-x86",
        "aarch64" => classifier == "natives-windows-arm64",
        _ => classifier == "natives-windows" || classifier == "natives-windows-x86_64",
    }
}

fn is_native_artifact(name: &str) -> bool {
    name.rsplit(':')
        .next()
        .is_some_and(|classifier| classifier.starts_with("natives-"))
}

fn rules_allow(rules: &[Rule], custom_resolution: bool) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let os_matches = rule.os.as_ref().is_none_or(|os| {
            os.name.as_deref().is_none_or(|name| name == "windows")
                && os.arch.as_deref().is_none_or(arch_matches)
        });
        let features_match = rule.features.as_ref().is_none_or(|features| {
            features.iter().all(|(name, expected)| {
                let actual = name == "has_custom_resolution" && custom_resolution;
                actual == *expected
            })
        });
        if os_matches && features_match {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn arch_matches(value: &str) -> bool {
    match std::env::consts::ARCH {
        "x86_64" => matches!(value, "x86_64" | "amd64" | "64"),
        "aarch64" => matches!(value, "aarch64" | "arm64"),
        "x86" => matches!(value, "x86" | "i386" | "32"),
        other => value == other,
    }
}

fn windows_arch() -> &'static str {
    if cfg!(target_pointer_width = "64") {
        "64"
    } else {
        "32"
    }
}

fn expand_arguments(
    entries: &[ArgumentEntry],
    replacements: &HashMap<&str, String>,
    custom_resolution: bool,
) -> Vec<String> {
    let mut result = Vec::new();
    for entry in entries {
        match entry {
            ArgumentEntry::Plain(value) => {
                result.push(replace_placeholders(value, replacements));
            }
            ArgumentEntry::Conditional { rules, value }
                if rules_allow(rules, custom_resolution) =>
            {
                match value {
                    ArgumentValue::One(value) => {
                        result.push(replace_placeholders(value, replacements))
                    }
                    ArgumentValue::Many(values) => result.extend(
                        values
                            .iter()
                            .map(|value| replace_placeholders(value, replacements)),
                    ),
                }
            }
            ArgumentEntry::Conditional { .. } => {}
        }
    }
    result
}

fn replace_placeholders(value: &str, replacements: &HashMap<&str, String>) -> String {
    replacements
        .iter()
        .fold(value.to_string(), |current, (from, to)| {
            current.replace(from, to)
        })
}

fn split_custom_arguments(value: &str) -> Result<Vec<String>, LaunchError> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quote == Some('"') {
            escaped = true;
            continue;
        }
        if character == '"' || character == '\'' {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            } else {
                current.push(character);
            }
            continue;
        }
        if character.is_whitespace() && quote.is_none() {
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if quote.is_some() || escaped {
        return Err(LaunchError::InvalidCustomArguments);
    }
    if !current.is_empty() {
        result.push(current);
    }
    Ok(result)
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> LaunchError {
    LaunchError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        native_artifact_for_current_arch, offline_uuid, split_custom_arguments, valid_username,
    };

    #[test]
    fn validates_offline_usernames() {
        assert!(valid_username("Steve_01"));
        assert!(!valid_username("ab"));
        assert!(!valid_username("名字"));
        assert!(!valid_username("has space"));
    }

    #[test]
    fn builds_java_compatible_offline_uuid() {
        assert_eq!(offline_uuid("Steve"), "5627dd98e6be3c21b8a8e92344183641");
    }

    #[test]
    fn splits_quoted_custom_arguments() {
        assert_eq!(
            split_custom_arguments(r#"-Dname="Na Craft" --demo"#).unwrap(),
            vec!["-Dname=Na Craft", "--demo"]
        );
        assert!(split_custom_arguments(r#""unfinished"#).is_err());
    }

    #[test]
    fn selects_only_host_native_classifier() {
        if cfg!(target_arch = "x86_64") {
            assert!(native_artifact_for_current_arch(
                "org.lwjgl:lwjgl:3.4.1:natives-windows"
            ));
            assert!(!native_artifact_for_current_arch(
                "org.lwjgl:lwjgl:3.4.1:natives-windows-x86"
            ));
        }
    }
}
