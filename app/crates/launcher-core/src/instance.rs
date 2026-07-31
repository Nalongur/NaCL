use crate::data::{load_or_create_settings, save_settings, AppPaths, DataError, LauncherSettings};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use uuid::Uuid;

pub const INSTANCE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub loader: GameLoader,
    pub game_directory: PathBuf,
    pub java: JavaSelection,
    pub memory: MemorySettings,
    #[serde(default)]
    pub display: DisplaySettings,
    #[serde(default)]
    pub advanced: AdvancedSettings,
    #[serde(default)]
    pub installation: InstallationInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GameLoader {
    Vanilla,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum JavaSelection {
    Auto,
    Custom { path: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySettings {
    pub minimum_mb: u32,
    pub maximum_mb: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowMode {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplaySettings {
    pub mode: WindowMode,
    pub width: u32,
    pub height: u32,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            mode: WindowMode::Windowed,
            width: 1280,
            height: 720,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedSettings {
    pub jvm_arguments: String,
    pub game_arguments: String,
    pub debug_logging: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallationState {
    #[default]
    Incomplete,
    Ready,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationInfo {
    pub state: InstallationState,
    pub installed_epoch_ms: Option<u128>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInstanceRequest {
    pub name: String,
    pub game_version: String,
}

#[derive(Debug)]
pub enum InstanceError {
    Data(DataError),
    InvalidName,
    InvalidGameVersion,
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    InvalidConfig {
        path: PathBuf,
        source: serde_json::Error,
    },
    InvalidConfigStructure(PathBuf),
    InstanceNotFound(String),
    InvalidInstanceValue(&'static str),
    SerializeConfig(serde_json::Error),
}

impl fmt::Display for InstanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => error.fmt(formatter),
            Self::InvalidName => write!(formatter, "实例名称不能为空、过长或包含控制字符"),
            Self::InvalidGameVersion => {
                write!(formatter, "游戏版本标识为空或包含不支持的字符")
            }
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "无法{action} {}：{source}", path.display()),
            Self::InvalidConfig { path, source } => {
                write!(formatter, "实例配置 {} 无效：{source}", path.display())
            }
            Self::InvalidConfigStructure(path) => {
                write!(formatter, "实例配置 {} 的目录结构无效", path.display())
            }
            Self::InstanceNotFound(id) => write!(formatter, "找不到实例 {id}"),
            Self::InvalidInstanceValue(message) => write!(formatter, "实例设置无效：{message}"),
            Self::SerializeConfig(source) => write!(formatter, "无法序列化实例配置：{source}"),
        }
    }
}

impl std::error::Error for InstanceError {}

impl From<DataError> for InstanceError {
    fn from(error: DataError) -> Self {
        Self::Data(error)
    }
}

pub type InstanceResult<T> = Result<T, InstanceError>;

pub fn list_instances() -> InstanceResult<Vec<InstanceConfig>> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    list_instances_in(&paths)
}

pub fn create_instance(request: CreateInstanceRequest) -> InstanceResult<InstanceConfig> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    create_instance_in(&paths, request)
}

pub fn select_instance(instance_id: String) -> InstanceResult<LauncherSettings> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    select_instance_in(&paths, &instance_id)
}

pub fn update_instance(instance: InstanceConfig) -> InstanceResult<InstanceConfig> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    update_instance_in(&paths, instance)
}

pub fn duplicate_instance(instance_id: String) -> InstanceResult<InstanceConfig> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    duplicate_instance_in(&paths, &instance_id)
}

pub fn delete_instance(instance_id: String) -> InstanceResult<LauncherSettings> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    delete_instance_in(&paths, &instance_id)
}

pub fn list_instances_in(paths: &AppPaths) -> InstanceResult<Vec<InstanceConfig>> {
    let mut instances = Vec::new();
    let entries = fs::read_dir(&paths.instances_dir)
        .map_err(|source| io_error("读取实例目录", &paths.instances_dir, source))?;

    for entry in entries {
        let entry =
            entry.map_err(|source| io_error("读取实例目录项", &paths.instances_dir, source))?;
        if !entry
            .file_type()
            .map_err(|source| io_error("读取实例目录项类型", &entry.path(), source))?
            .is_dir()
        {
            continue;
        }

        let folder_name = entry.file_name();
        if folder_name.to_string_lossy().starts_with('.') {
            continue;
        }

        let config_path = entry.path().join("instance.json");
        if !config_path.is_file() {
            continue;
        }

        let contents = fs::read_to_string(&config_path)
            .map_err(|source| io_error("读取实例配置", &config_path, source))?;
        let instance: InstanceConfig =
            serde_json::from_str(&contents).map_err(|source| InstanceError::InvalidConfig {
                path: config_path.clone(),
                source,
            })?;
        if instance.schema_version != INSTANCE_SCHEMA_VERSION
            || instance.id != folder_name.to_string_lossy()
            || instance.loader != GameLoader::Vanilla
            || instance.game_directory != Path::new("game")
        {
            return Err(InstanceError::InvalidConfigStructure(config_path));
        }
        instances.push(instance);
    }

    instances.sort_by(|left: &InstanceConfig, right: &InstanceConfig| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(instances)
}

pub fn create_instance_in(
    paths: &AppPaths,
    request: CreateInstanceRequest,
) -> InstanceResult<InstanceConfig> {
    paths.initialize()?;
    let mut launcher_settings = load_or_create_settings(paths)?;

    let name = request.name.trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(InstanceError::InvalidName);
    }

    let game_version = request.game_version.trim();
    if game_version.is_empty()
        || game_version.len() > 40
        || !game_version
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
    {
        return Err(InstanceError::InvalidGameVersion);
    }

    let id = Uuid::new_v4().simple().to_string();
    let staging_dir = paths.instances_dir.join(format!(".create-{id}"));
    let final_dir = paths.instances_dir.join(&id);
    let game_dir = staging_dir.join("game");
    let config_path = staging_dir.join("instance.json");

    fs::create_dir(&staging_dir)
        .map_err(|source| io_error("创建实例暂存目录", &staging_dir, source))?;

    let result = (|| {
        fs::create_dir(&game_dir).map_err(|source| io_error("创建游戏目录", &game_dir, source))?;

        let instance = InstanceConfig {
            schema_version: INSTANCE_SCHEMA_VERSION,
            id: id.clone(),
            name: name.to_owned(),
            game_version: game_version.to_owned(),
            loader: GameLoader::Vanilla,
            game_directory: PathBuf::from("game"),
            java: JavaSelection::Auto,
            memory: MemorySettings {
                minimum_mb: 1024,
                maximum_mb: launcher_settings.default_memory_mb,
            },
            display: DisplaySettings {
                width: launcher_settings.default_window_width,
                height: launcher_settings.default_window_height,
                ..DisplaySettings::default()
            },
            advanced: AdvancedSettings::default(),
            installation: InstallationInfo::default(),
        };
        let mut contents =
            serde_json::to_vec_pretty(&instance).map_err(InstanceError::SerializeConfig)?;
        contents.push(b'\n');
        fs::write(&config_path, contents)
            .map_err(|source| io_error("写入实例配置", &config_path, source))?;
        fs::rename(&staging_dir, &final_dir)
            .map_err(|source| io_error("完成实例创建", &final_dir, source))?;

        launcher_settings.selected_instance_id = Some(id);
        save_settings(paths, &launcher_settings)?;
        Ok(instance)
    })();

    if result.is_err() {
        let rollback_path = if final_dir.is_dir() {
            &final_dir
        } else {
            &staging_dir
        };
        let _ = fs::remove_dir_all(rollback_path);
    }

    result
}

pub fn update_instance_in(
    paths: &AppPaths,
    instance: InstanceConfig,
) -> InstanceResult<InstanceConfig> {
    validate_instance(&instance)?;
    let instance_dir = paths.instances_dir.join(&instance.id);
    let config_path = instance_dir.join("instance.json");
    if !instance_dir.is_dir() || !config_path.is_file() {
        return Err(InstanceError::InstanceNotFound(instance.id));
    }

    let mut contents =
        serde_json::to_vec_pretty(&instance).map_err(InstanceError::SerializeConfig)?;
    contents.push(b'\n');
    let mut temporary = NamedTempFile::new_in(&instance_dir)
        .map_err(|source| io_error("创建实例配置临时文件", &instance_dir, source))?;
    temporary
        .write_all(&contents)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|source| io_error("写入实例配置临时文件", temporary.path(), source))?;
    temporary
        .persist(&config_path)
        .map_err(|error| io_error("替换实例配置", &config_path, error.error))?;
    Ok(instance)
}

pub fn duplicate_instance_in(
    paths: &AppPaths,
    instance_id: &str,
) -> InstanceResult<InstanceConfig> {
    let source = list_instances_in(paths)?
        .into_iter()
        .find(|candidate| candidate.id == instance_id)
        .ok_or_else(|| InstanceError::InstanceNotFound(instance_id.to_owned()))?;

    let id = Uuid::new_v4().simple().to_string();
    let staging_dir = paths.instances_dir.join(format!(".create-{id}"));
    let final_dir = paths.instances_dir.join(&id);
    let source_dir = paths.instances_dir.join(instance_id);
    fs::create_dir(&staging_dir)
        .map_err(|source| io_error("创建实例副本暂存目录", &staging_dir, source))?;

    let result = (|| {
        for entry in walkdir::WalkDir::new(&source_dir).min_depth(1) {
            let entry = entry.map_err(|error| {
                io_error(
                    "读取实例副本来源",
                    error.path().unwrap_or(&source_dir),
                    io::Error::other(error.to_string()),
                )
            })?;
            let relative = entry.path().strip_prefix(&source_dir).map_err(|error| {
                io_error(
                    "解析实例副本路径",
                    entry.path(),
                    io::Error::other(error.to_string()),
                )
            })?;
            let destination = staging_dir.join(relative);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&destination)
                    .map_err(|source| io_error("创建实例副本目录", &destination, source))?;
            } else if entry.file_name() != "instance.json" {
                fs::copy(entry.path(), &destination)
                    .map_err(|source| io_error("复制实例文件", &destination, source))?;
            }
        }

        let mut duplicate = source;
        duplicate.id.clone_from(&id);
        duplicate.name = unique_copy_name(paths, &duplicate.name)?;
        let config_path = staging_dir.join("instance.json");
        let mut contents =
            serde_json::to_vec_pretty(&duplicate).map_err(InstanceError::SerializeConfig)?;
        contents.push(b'\n');
        fs::write(&config_path, contents)
            .map_err(|source| io_error("写入实例副本配置", &config_path, source))?;
        fs::rename(&staging_dir, &final_dir)
            .map_err(|source| io_error("完成实例复制", &final_dir, source))?;

        let mut settings = load_or_create_settings(paths)?;
        settings.selected_instance_id = Some(id);
        save_settings(paths, &settings)?;
        Ok(duplicate)
    })();

    if result.is_err() {
        let rollback_path = if final_dir.is_dir() {
            &final_dir
        } else {
            &staging_dir
        };
        let _ = fs::remove_dir_all(rollback_path);
    }
    result
}

pub fn delete_instance_in(paths: &AppPaths, instance_id: &str) -> InstanceResult<LauncherSettings> {
    let instances = list_instances_in(paths)?;
    if !instances
        .iter()
        .any(|candidate| candidate.id == instance_id)
    {
        return Err(InstanceError::InstanceNotFound(instance_id.to_owned()));
    }

    let instance_dir = paths.instances_dir.join(instance_id);
    fs::remove_dir_all(&instance_dir)
        .map_err(|source| io_error("删除实例目录", &instance_dir, source))?;

    let mut settings = load_or_create_settings(paths)?;
    if settings.selected_instance_id.as_deref() == Some(instance_id) {
        settings.selected_instance_id = instances
            .iter()
            .find(|candidate| candidate.id != instance_id)
            .map(|candidate| candidate.id.clone());
        save_settings(paths, &settings)?;
    }
    Ok(settings)
}

fn unique_copy_name(paths: &AppPaths, source_name: &str) -> InstanceResult<String> {
    let names = list_instances_in(paths)?
        .into_iter()
        .map(|instance| instance.name)
        .collect::<Vec<_>>();
    let base = format!("{source_name} 副本");
    if !names.iter().any(|name| name == &base) {
        return Ok(base);
    }
    for suffix in 2..=999 {
        let candidate = format!("{base} {suffix}");
        if !names.iter().any(|name| name == &candidate) {
            return Ok(candidate);
        }
    }
    Err(InstanceError::InvalidInstanceValue(
        "无法生成唯一的实例副本名称",
    ))
}

pub fn select_instance_in(paths: &AppPaths, instance_id: &str) -> InstanceResult<LauncherSettings> {
    let exists = list_instances_in(paths)?
        .iter()
        .any(|instance| instance.id == instance_id);
    if !exists {
        return Err(InstanceError::InstanceNotFound(instance_id.to_owned()));
    }

    let mut settings = load_or_create_settings(paths)?;
    settings.selected_instance_id = Some(instance_id.to_owned());
    save_settings(paths, &settings)?;
    Ok(settings)
}

fn io_error(action: &'static str, path: &std::path::Path, source: io::Error) -> InstanceError {
    InstanceError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

fn validate_instance(instance: &InstanceConfig) -> InstanceResult<()> {
    if instance.schema_version != INSTANCE_SCHEMA_VERSION
        || instance.loader != GameLoader::Vanilla
        || instance.game_directory != Path::new("game")
        || instance.id.len() != 32
        || !instance
            .id
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(InstanceError::InvalidInstanceValue(
            "实例标识或目录结构不正确",
        ));
    }
    let name = instance.name.trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(InstanceError::InvalidName);
    }
    let version = instance.game_version.trim();
    if version.is_empty()
        || version.len() > 40
        || !version
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
    {
        return Err(InstanceError::InvalidGameVersion);
    }
    if instance.memory.minimum_mb < 512
        || instance.memory.maximum_mb > 32768
        || instance.memory.minimum_mb > instance.memory.maximum_mb
    {
        return Err(InstanceError::InvalidInstanceValue(
            "内存范围必须在 512–32768 MB 之间",
        ));
    }
    if !(854..=7680).contains(&instance.display.width)
        || !(480..=4320).contains(&instance.display.height)
    {
        return Err(InstanceError::InvalidInstanceValue(
            "游戏窗口尺寸超出支持范围",
        ));
    }
    if instance.advanced.jvm_arguments.len() > 4096
        || instance.advanced.game_arguments.len() > 4096
        || instance
            .advanced
            .jvm_arguments
            .chars()
            .any(char::is_control)
        || instance
            .advanced
            .game_arguments
            .chars()
            .any(char::is_control)
    {
        return Err(InstanceError::InvalidInstanceValue(
            "高级参数过长或包含控制字符",
        ));
    }
    if let JavaSelection::Custom { path } = &instance.java {
        if path.as_os_str().is_empty() {
            return Err(InstanceError::InvalidInstanceValue(
                "自定义 Java 路径不能为空",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        create_instance_in, delete_instance_in, duplicate_instance_in, list_instances_in,
        select_instance_in, update_instance_in, AdvancedSettings, CreateInstanceRequest,
        DisplaySettings, GameLoader, InstallationInfo, InstanceConfig, JavaSelection,
        MemorySettings, INSTANCE_SCHEMA_VERSION,
    };
    use crate::data::{load_or_create_settings, AppPaths};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_paths() -> (PathBuf, AppPaths) {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nacl-instance-test-{}-{suffix}",
            std::process::id()
        ));
        let paths = AppPaths::from_roots(root.join("roaming"), root.join("local"));
        (root, paths)
    }

    #[test]
    fn vanilla_instance_model_round_trips() {
        let instance = InstanceConfig {
            schema_version: INSTANCE_SCHEMA_VERSION,
            id: "survival".to_string(),
            name: "原版生存".to_string(),
            game_version: "1.21".to_string(),
            loader: GameLoader::Vanilla,
            game_directory: PathBuf::from("game"),
            java: JavaSelection::Auto,
            memory: MemorySettings {
                minimum_mb: 1024,
                maximum_mb: 4096,
            },
            display: DisplaySettings::default(),
            advanced: AdvancedSettings::default(),
            installation: InstallationInfo::default(),
        };

        let json = serde_json::to_string(&instance).expect("instance should serialize");
        let restored: InstanceConfig =
            serde_json::from_str(&json).expect("instance should deserialize");
        assert_eq!(restored, instance);
    }

    #[test]
    fn creates_and_lists_a_real_instance_directory() {
        let (root, paths) = temporary_paths();
        let created = create_instance_in(
            &paths,
            CreateInstanceRequest {
                name: "原版生存".to_string(),
                game_version: "1.21.1".to_string(),
            },
        )
        .expect("instance should be created");

        let instance_dir = paths.instances_dir.join(&created.id);
        assert!(instance_dir.join("instance.json").is_file());
        assert!(instance_dir.join("game").is_dir());
        assert_eq!(created.game_directory, PathBuf::from("game"));

        let listed = list_instances_in(&paths).expect("instance should be listed");
        assert_eq!(listed, vec![created.clone()]);

        let settings = load_or_create_settings(&paths).expect("settings should load");
        assert_eq!(
            settings.selected_instance_id.as_deref(),
            Some(created.id.as_str())
        );
        assert!(fs::read_dir(&paths.instances_dir)
            .expect("instances directory should be readable")
            .all(|entry| !entry
                .expect("entry should be readable")
                .file_name()
                .to_string_lossy()
                .starts_with(".create-")));

        fs::remove_dir_all(root).expect("temporary tree should be removable");
    }

    #[test]
    fn rejects_invalid_instance_input_without_creating_files() {
        let (root, paths) = temporary_paths();
        paths
            .initialize()
            .expect("directory tree should initialize");

        let result = create_instance_in(
            &paths,
            CreateInstanceRequest {
                name: " ".to_string(),
                game_version: "../1.21".to_string(),
            },
        );
        assert!(result.is_err());
        assert_eq!(
            fs::read_dir(&paths.instances_dir)
                .expect("instances directory should be readable")
                .count(),
            0
        );

        fs::remove_dir_all(root).expect("temporary tree should be removable");
    }

    #[test]
    fn persists_an_existing_instance_selection() {
        let (root, paths) = temporary_paths();
        let first = create_instance_in(
            &paths,
            CreateInstanceRequest {
                name: "第一实例".to_string(),
                game_version: "1.20.6".to_string(),
            },
        )
        .expect("first instance should be created");
        create_instance_in(
            &paths,
            CreateInstanceRequest {
                name: "第二实例".to_string(),
                game_version: "1.21.1".to_string(),
            },
        )
        .expect("second instance should be created");

        let settings =
            select_instance_in(&paths, &first.id).expect("existing instance should be selected");
        assert_eq!(
            settings.selected_instance_id.as_deref(),
            Some(first.id.as_str())
        );

        fs::remove_dir_all(root).expect("temporary tree should be removable");
    }

    #[test]
    fn updates_instance_settings_atomically() {
        let (root, paths) = temporary_paths();
        let mut instance = create_instance_in(
            &paths,
            CreateInstanceRequest {
                name: "设置测试".to_string(),
                game_version: "1.21.1".to_string(),
            },
        )
        .expect("instance should be created");
        instance.memory.maximum_mb = 6144;
        instance.display.width = 1600;
        instance.advanced.debug_logging = true;

        let updated = update_instance_in(&paths, instance.clone()).expect("instance should update");
        assert_eq!(updated, instance);
        assert_eq!(
            list_instances_in(&paths)
                .expect("instance should reload")
                .first()
                .expect("instance should exist"),
            &instance
        );

        fs::remove_dir_all(root).expect("temporary tree should be removable");
    }

    #[test]
    fn duplicates_and_deletes_an_instance() {
        let (root, paths) = temporary_paths();
        let created = create_instance_in(
            &paths,
            CreateInstanceRequest {
                name: "原版生存".to_string(),
                game_version: "1.21.1".to_string(),
            },
        )
        .expect("instance should be created");
        fs::write(
            paths
                .instances_dir
                .join(&created.id)
                .join("game")
                .join("options.txt"),
            "guiScale:3",
        )
        .expect("fixture should be written");

        let duplicate =
            duplicate_instance_in(&paths, &created.id).expect("instance should duplicate");
        assert_eq!(duplicate.name, "原版生存 副本");
        assert!(paths
            .instances_dir
            .join(&duplicate.id)
            .join("game")
            .join("options.txt")
            .is_file());

        let settings =
            delete_instance_in(&paths, &duplicate.id).expect("duplicate should be deleted");
        assert_eq!(
            settings.selected_instance_id.as_deref(),
            Some(created.id.as_str())
        );
        assert_eq!(
            list_instances_in(&paths)
                .expect("instances should list")
                .len(),
            1
        );

        fs::remove_dir_all(root).expect("temporary tree should be removable");
    }
}
