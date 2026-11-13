use crate::data::{load_or_create_settings, save_settings, AppPaths, DataError, LauncherSettings};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use uuid::Uuid;

pub const INSTANCE_SCHEMA_VERSION: u32 = 2;
const SHARED_INSTANCE_SCHEMA_VERSION: u32 = 3;

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub loader: LoaderConfig,
    pub game_directory: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_directory: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub can_rollback: bool,
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
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderConfig {
    pub kind: GameLoader,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}

impl LoaderConfig {
    pub fn vanilla() -> Self {
        Self {
            kind: GameLoader::Vanilla,
            version: None,
            profile_id: None,
        }
    }

    pub fn modded(kind: GameLoader, version: String, profile_id: String) -> Self {
        Self {
            kind,
            version: Some(version),
            profile_id: Some(profile_id),
        }
    }

    pub fn is_vanilla(&self) -> bool {
        self.kind == GameLoader::Vanilla
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LoaderConfigWire {
    Legacy(GameLoader),
    Current {
        kind: GameLoader,
        version: Option<String>,
        #[serde(rename = "profileId")]
        profile_id: Option<String>,
    },
}

impl<'de> Deserialize<'de> for LoaderConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match LoaderConfigWire::deserialize(deserializer)? {
            LoaderConfigWire::Legacy(kind) => Ok(Self {
                kind,
                version: None,
                profile_id: None,
            }),
            LoaderConfigWire::Current {
                kind,
                version,
                profile_id,
            } => Ok(Self {
                kind,
                version,
                profile_id,
            }),
        }
    }
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

pub fn list_deleted_instances() -> InstanceResult<Vec<InstanceConfig>> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    list_deleted_instances_in(&paths)
}

pub fn restore_instance(instance_id: String) -> InstanceResult<InstanceConfig> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    restore_instance_in(&paths, &instance_id)
}

pub fn permanently_delete_instance(instance_id: String) -> InstanceResult<()> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    permanently_delete_instance_in(&paths, &instance_id)
}

fn deleted_instances_dir(paths: &AppPaths) -> PathBuf {
    paths.instances_dir.join(".deleted")
}

fn verify_deleted_instances_dir(paths: &AppPaths, deleted_dir: &Path) -> InstanceResult<()> {
    let metadata = fs::symlink_metadata(deleted_dir)
        .map_err(|source| io_error("读取已删除实例目录", deleted_dir, source))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(InstanceError::InvalidInstanceValue("回收区必须是真实目录"));
    }
    let root = fs::canonicalize(&paths.instances_dir)
        .map_err(|source| io_error("读取实例目录", &paths.instances_dir, source))?;
    let deleted = fs::canonicalize(deleted_dir)
        .map_err(|source| io_error("读取已删除实例目录", deleted_dir, source))?;
    if deleted.parent() != Some(root.as_path()) {
        return Err(InstanceError::InvalidInstanceValue(
            "回收区必须位于当前实例目录内",
        ));
    }
    Ok(())
}

pub fn list_deleted_instances_in(paths: &AppPaths) -> InstanceResult<Vec<InstanceConfig>> {
    let deleted_dir = deleted_instances_dir(paths);
    if !deleted_dir.exists() {
        return Ok(Vec::new());
    }
    verify_deleted_instances_dir(paths, &deleted_dir)?;
    let mut deleted_paths = paths.clone();
    deleted_paths.instances_dir = deleted_dir;
    list_instances_in(&deleted_paths)
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
        let mut instance: InstanceConfig =
            serde_json::from_str(&contents).map_err(|source| InstanceError::InvalidConfig {
                path: config_path.clone(),
                source,
            })?;
        let needs_migration = instance.schema_version == 1 && instance.loader.is_vanilla();
        if needs_migration {
            instance.schema_version = INSTANCE_SCHEMA_VERSION;
        }
        if !valid_layout_schema(&instance)
            || instance.id != folder_name.to_string_lossy()
            || !valid_version_directory(&instance)
            || !valid_loader_config(&instance.loader)
            || instance.game_directory != Path::new("game")
        {
            return Err(InstanceError::InvalidConfigStructure(config_path));
        }
        if needs_migration {
            persist_instance_config(&entry.path(), &config_path, &instance)?;
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

pub(crate) fn find_instance_in(paths: &AppPaths, id: &str) -> InstanceResult<InstanceConfig> {
    list_instances_in(paths)?
        .into_iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| InstanceError::InstanceNotFound(id.to_owned()))
}

fn safe_version_component(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.ends_with(['.', ' '])
        && !name.chars().any(|c| c.is_control() || "<>:\\/\"|?*!;".contains(c))
        && !matches!(
            name.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase().as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5"
            | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4"
            | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9" | "COM¹" | "COM²" | "COM³"
            | "LPT¹" | "LPT²" | "LPT³" | "CONIN$" | "CONOUT$"
        )
}

fn valid_version_directory(instance: &InstanceConfig) -> bool {
    instance.version_directory.as_deref().is_none_or(safe_version_component)
        && instance.id.len() == 32
        && instance.id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_layout_schema(instance: &InstanceConfig) -> bool {
    if instance.version_directory.is_some() {
        instance.schema_version == SHARED_INSTANCE_SCHEMA_VERSION
    } else {
        instance.schema_version == INSTANCE_SCHEMA_VERSION
    }
}

fn version_directory_name(paths: &AppPaths, name: &str) -> InstanceResult<String> {
    let cleaned: String = name.chars().take(64).map(|c| {
        if c.is_control() || "<>:\\/\"|?*!;".contains(c) { '_' } else { c }
    }).collect();
    let cleaned = cleaned.trim_matches(['.', ' ']);
    let base = if safe_version_component(cleaned) { cleaned.to_owned() } else { format!("实例 {cleaned}").trim_end().to_owned() };
    for suffix in 1..=999 {
        let candidate = if suffix == 1 { base.clone() } else { format!("{base} ({suffix})") };
        let path = paths.minecraft_root.join("versions").join(&candidate);
        if matches!(fs::symlink_metadata(&path), Err(error) if error.kind() == io::ErrorKind::NotFound) {
            return Ok(candidate);
        }
    }
    Err(InstanceError::InvalidInstanceValue("无法分配版本目录名"))
}

impl InstanceConfig {
    pub(crate) fn version_name(&self) -> InstanceResult<&str> {
        self.version_directory.as_deref().filter(|name| safe_version_component(name))
            .ok_or(InstanceError::InvalidInstanceValue("请先迁移旧实例"))
    }

    pub(crate) fn game_path(&self, paths: &AppPaths) -> InstanceResult<PathBuf> {
        let versions = paths.minecraft_root.join("versions");
        let path = versions.join(self.version_name()?);
        if !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_dir()) {
        return Err(InstanceError::InvalidInstanceValue("共享游戏目录缺失或为链接"));
    }
    let canonical = fs::canonicalize(&path)
            .map_err(|source| io_error("读取共享游戏目录", &path, source))?;
        let root = fs::canonicalize(&versions)
            .map_err(|source| io_error("读取共享版本根目录", &versions, source))?;
        if canonical.parent() != Some(root.as_path()) || canonical.file_name() != path.file_name() || !canonical.is_dir() {
            return Err(InstanceError::InvalidInstanceValue("共享游戏目录路径不安全"));
        }
        Ok(path)
    }

    pub(crate) fn cached_profile(&self) -> &str {
        self.loader.profile_id.as_deref().unwrap_or(&self.game_version)
    }
}

pub(crate) fn write_version_files(
    paths: &AppPaths,
    instance: &InstanceConfig,
    target: &Path,
) -> InstanceResult<()> {
    let name = instance.version_name()?;
    let profile = instance.cached_profile();
    let metadata = paths.versions_dir.join(format!("{profile}.json"));
    let raw = fs::read(&metadata).map_err(|source| io_error("读取启动配置", &metadata, source))?;
    let mut json: serde_json::Value = serde_json::from_slice(&raw)
        .map_err(|source| InstanceError::InvalidConfig { path: metadata.clone(), source })?;
    let object = json.as_object_mut().ok_or_else(|| InstanceError::InvalidConfigStructure(metadata))?;
    // NaCL's cached loader profile already contains the base game's arguments and libraries.
    object.remove("inheritsFrom");
    object.remove("jar");
    object.insert("id".into(), serde_json::Value::String(name.to_owned()));
    let client = paths.versions_dir.join(format!("{profile}.jar"));
    let client = if client.is_file() { client } else { paths.versions_dir.join(format!("{}.jar", instance.game_version)) };
    let jar = target.join(format!("{name}.jar"));
    copy_file_atomically(&client, &jar)?;
    let ini = target.join("PCL").join("Setup.ini");
    if !ini.exists() {
        let pcl = target.join("PCL");
        fs::create_dir_all(&pcl).map_err(|source| io_error("创建 PCL 配置目录", &pcl, source))?;
        fs::write(&ini, "VersionArgumentIndie:1\r\nVersionArgumentIndieV2:True\r\n")
            .map_err(|source| io_error("设置 PCL 版本隔离", &ini, source))?;
    }
    let metadata = target.join(format!("{name}.json"));
    let contents = serde_json::to_vec_pretty(&json).map_err(InstanceError::SerializeConfig)?;
    write_atomically(&metadata, &contents)
}

fn copy_file_atomically(source: &Path, destination: &Path) -> InstanceResult<()> {
    let mut input = fs::File::open(source).map_err(|error| io_error("读取源文件", source, error))?;
    let mut output = NamedTempFile::new_in(destination.parent().expect("file has parent"))
        .map_err(|error| io_error("创建暂存文件", destination, error))?;
    io::copy(&mut input, &mut output).map_err(|error| io_error("复制文件", destination, error))?;
    output.persist(destination).map_err(|error| io_error("提交文件", destination, error.error))?;
    Ok(())
}

fn write_atomically(destination: &Path, contents: &[u8]) -> InstanceResult<()> {
    let mut output = NamedTempFile::new_in(destination.parent().expect("file has parent"))
        .map_err(|error| io_error("创建暂存文件", destination, error))?;
    output.write_all(contents).map_err(|error| io_error("写入文件", destination, error))?;
    output.as_file().sync_all().map_err(|error| io_error("同步文件", destination, error))?;
    output.persist(destination).map_err(|error| io_error("提交文件", destination, error.error))?;
    Ok(())
}

fn copy_tree(source: &Path, target: &Path, excluded: &[&Path], reuse: bool) -> InstanceResult<()> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| io_error("读取复制来源", source, error))?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(InstanceError::InvalidInstanceValue("复制来源必须是普通目录"));
    }
    for entry in walkdir::WalkDir::new(source).min_depth(1).follow_links(false).follow_root_links(false).into_iter()
        .filter_entry(|entry| !excluded.iter().any(|path| entry.path() == *path))
    {
        let entry = entry.map_err(|error| io_error("读取复制来源", source, io::Error::other(error)))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| io_error("读取复制来源", entry.path(), error))?;
        if is_link(&metadata) {
            return Err(InstanceError::InvalidInstanceValue("迁移和复制暂不支持链接或特殊文件"));
        }
        let relative = entry.path().strip_prefix(source).expect("entry belongs to source");
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|error| io_error("创建目录", &destination, error))?;
        } else if entry.file_type().is_file() {
            if !reuse || !destination.is_file() {
                copy_file_atomically(entry.path(), &destination)?;
            }
        } else {
            return Err(InstanceError::InvalidInstanceValue("迁移和复制暂不支持链接或特殊文件"));
        }
    }
    Ok(())
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

pub fn migrate_instance(id: &str) -> InstanceResult<InstanceConfig> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    migrate_instance_in(&paths, id)
}

pub fn rollback_instance(id: &str) -> InstanceResult<InstanceConfig> {
    let paths = AppPaths::resolve()?;
    rollback_instance_in(&paths, id)
}

fn migrate_instance_in(paths: &AppPaths, id: &str) -> InstanceResult<InstanceConfig> {
    let mut instance = find_instance_in(paths, id)?;
    if instance.version_directory.is_some() {
        return Ok(instance);
    }
    let record = paths.instances_dir.join(id);
    let legacy_game = record.join("game");
    let versions = paths.minecraft_root.join("versions");
    instance.version_directory = Some(version_directory_name(paths, &instance.name)?);
    instance.schema_version = SHARED_INSTANCE_SCHEMA_VERSION;
    instance.can_rollback = true;
    let target = versions.join(instance.version_name()?);
    let staging = tempfile::Builder::new().prefix(".nacl-").tempdir_in(&versions)
        .map_err(|error| io_error("创建迁移暂存目录", &versions, error))?;
    copy_tree(&legacy_game, staging.path(), &[], false)?;
    let legacy_state = record.join(".nacl");
    if legacy_state.is_dir() {
        let state = staging.path().join(".nacl");
        fs::create_dir_all(&state).map_err(|error| io_error("创建实例状态目录", &state, error))?;
        copy_tree(&legacy_state, &state, &[&legacy_state.join("natives")], false)?;
    }
    if instance.installation.state == InstallationState::Ready {
        for (source, target) in [
            (paths.cache_dir.join("assets"), &paths.assets_dir),
            (paths.cache_dir.join("libraries"), &paths.libraries_dir),
        ] {
            if source.is_dir() {
                copy_tree(&source, target, &[], true)?;
            }
        }
        write_version_files(paths, &instance, staging.path())?;
        crate::maintenance::verify_migrated_files(paths, &instance, staging.path())?;
    }
    move_directory(staging.path(), &target, "发布共享版本目录")?;
    if let Err(error) = persist_instance_config(&record, &record.join("instance.json"), &instance) {
        fs::rename(&target, staging.path())
            .map_err(|error| io_error("撤回共享版本目录", &target, error))?;
        return Err(error);
    }
    Ok(instance)
}

fn rollback_instance_in(paths: &AppPaths, id: &str) -> InstanceResult<InstanceConfig> {
    let mut instance = find_instance_in(paths, id)?;
    let record = paths.instances_dir.join(id);
    let game = record.join("game");
    if !instance.can_rollback || instance.version_directory.is_none() {
        return Err(InstanceError::InvalidInstanceValue("只有从旧布局迁移的实例可以回退"));
    }
    let shared = instance.game_path(paths)?;
    let name = instance.version_name()?;
    let saves = shared.join("saves");
    let mut excluded = vec![
        shared.join(format!("{name}.json")),
        shared.join(format!("{name}.jar")),
        shared.join("PCL"),
        shared.join(".nacl"),
    ];
    let mut world_locks = Vec::new();
    if saves.is_dir() {
        for entry in fs::read_dir(&saves).map_err(|error| io_error("读取存档目录", &saves, error))? {
            let world = entry.map_err(|error| io_error("读取存档目录", &saves, error))?;
            if !world.file_type().map_err(|error| io_error("读取存档类型", &world.path(), error))?.is_dir() {
                continue;
            }
            let path = world.path().join("session.lock");
            match fs::OpenOptions::new().read(true).write(true).open(&path) {
                Ok(file) => {
                    file.try_lock().map_err(|error| io_error("锁定存档，请退出游戏后重试", &path, error.into()))?;
                    world_locks.push(file);
                    excluded.push(path);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error("打开存档锁", &path, error)),
            }
        }
    }
    let staging = tempfile::tempdir_in(&record)
        .map_err(|error| io_error("创建回退暂存目录", &record, error))?;
    let excluded: Vec<_> = excluded.iter().map(PathBuf::as_path).collect();
    copy_tree(&shared, staging.path(), &excluded, false)?;
    let state = record.join(".nacl");
    let shared_state = shared.join(".nacl");
    let staged_state = tempfile::tempdir_in(&record)
        .map_err(|error| io_error("创建内容状态暂存目录", &record, error))?;
    if shared_state.is_dir() {
        copy_tree(&shared_state, staged_state.path(), &[&shared_state.join("natives")], false)?;
    }
    let state_backup = record.join(format!(".state-backup-{}", Uuid::new_v4().simple()));
    let backup = record.join(format!(".legacy-backup-{}", Uuid::new_v4().simple()));
    let backed_up = game.is_dir();
    if backed_up {
        move_directory(&game, &backup, "备份旧游戏目录")?;
    }
    let mut published = false;
    let mut state_backed_up = false;
    let mut state_published = false;
    let result = (|| {
        fs::rename(staging.path(), &game)
            .map_err(|error| io_error("恢复旧布局的游戏目录", &game, error))?;
        published = true;
        if state.is_dir() {
            move_directory(&state, &state_backup, "备份旧内容状态")?;
            state_backed_up = true;
        }
        move_directory(staged_state.path(), &state, "恢复内容状态")?;
        state_published = true;
        instance.version_directory = None;
        instance.schema_version = INSTANCE_SCHEMA_VERSION;
        instance.can_rollback = false;
        persist_instance_config(&record, &record.join("instance.json"), &instance)?;
        Ok(instance)
    })();
    if result.is_err() {
        if state_published {
            move_directory(&state, staged_state.path(), "撤回内容状态")?;
        }
        if state_backed_up {
            move_directory(&state_backup, &state, "恢复旧内容状态")?;
        }
        if published {
            fs::rename(&game, staging.path())
                .map_err(|error| io_error("撤回回退目录", &game, error))?;
        }
        if backed_up {
            move_directory(&backup, &game, "恢复迁移前备份")?;
        }
    }
    result
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
    let version_directory = version_directory_name(paths, name)?;
    let staging_dir = paths.instances_dir.join(format!(".create-{id}"));
    let final_dir = paths.instances_dir.join(&id);
    let game_dir = paths.minecraft_root.join("versions").join(&version_directory);
    let config_path = staging_dir.join("instance.json");

    fs::create_dir(&staging_dir)
        .map_err(|source| io_error("创建实例暂存目录", &staging_dir, source))?;

    let mut created_game_dir = false;
    let result = (|| {
        fs::create_dir(&game_dir).map_err(|source| io_error("创建游戏目录", &game_dir, source))?;
        created_game_dir = true;

        let instance = InstanceConfig {
            schema_version: SHARED_INSTANCE_SCHEMA_VERSION,
            id: id.clone(),
            name: name.to_owned(),
            game_version: game_version.to_owned(),
            loader: LoaderConfig::vanilla(),
            game_directory: PathBuf::from("game"),
            version_directory: Some(version_directory),
            can_rollback: false,
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
        if created_game_dir {
            let _ = fs::remove_dir(&game_dir);
        }
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

    let current = find_instance_in(paths, &instance.id)?;
    if instance.version_directory != current.version_directory || instance.can_rollback != current.can_rollback {
        return Err(InstanceError::InvalidInstanceValue("实例路径已变化，请重新加载实例"));
    }
    persist_instance_config(&instance_dir, &config_path, &instance)?;
    Ok(instance)
}

pub fn duplicate_instance_in(
    paths: &AppPaths,
    instance_id: &str,
) -> InstanceResult<InstanceConfig> {
    let source = find_instance_in(paths, instance_id)?;

    let id = Uuid::new_v4().simple().to_string();
    let staging_dir = paths.instances_dir.join(format!(".create-{id}"));
    let final_dir = paths.instances_dir.join(&id);
    fs::create_dir(&staging_dir)
        .map_err(|source| io_error("创建实例副本暂存目录", &staging_dir, source))?;

    let mut published_game: Option<PathBuf> = None;
    let result = (|| {
        let source_game = source.game_path(paths)?;
        let source_name = source.version_name()?;
        let source_json = source_game.join(format!("{source_name}.json"));
        let source_jar = source_game.join(format!("{source_name}.jar"));
        let mut duplicate = source;
        duplicate.id.clone_from(&id);
        duplicate.can_rollback = false;
        duplicate.name = unique_copy_name(paths, &duplicate.name)?;
        duplicate.version_directory = Some(version_directory_name(paths, &duplicate.name)?);
        let versions = paths.minecraft_root.join("versions");
        let game_staging = tempfile::Builder::new().prefix(".nacl-").tempdir_in(&versions)
            .map_err(|error| io_error("创建副本暂存目录", &versions, error))?;
        copy_tree(&source_game, game_staging.path(), &[&source_json, &source_jar], false)?;
        if duplicate.installation.state == InstallationState::Ready {
            write_version_files(paths, &duplicate, game_staging.path())?;
        }
        let game_destination = versions.join(duplicate.version_name()?);
        let config_path = staging_dir.join("instance.json");
        let mut contents =
            serde_json::to_vec_pretty(&duplicate).map_err(InstanceError::SerializeConfig)?;
        contents.push(b'\n');
        fs::write(&config_path, contents)
            .map_err(|source| io_error("写入实例副本配置", &config_path, source))?;
        move_directory(game_staging.path(), &game_destination, "完成游戏目录复制")?;
        published_game = Some(game_destination);
        fs::rename(&staging_dir, &final_dir)
            .map_err(|source| io_error("完成实例复制", &final_dir, source))?;

        let mut settings = load_or_create_settings(paths)?;
        settings.selected_instance_id = Some(id);
        save_settings(paths, &settings)?;
        Ok(duplicate)
    })();

    if result.is_err() {
        if let Some(game) = published_game {
            let _ = fs::remove_dir_all(game);
        }
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
    let instance = instances.iter().find(|candidate| candidate.id == instance_id)
        .ok_or_else(|| InstanceError::InstanceNotFound(instance_id.to_owned()))?;

    let mut settings = load_or_create_settings(paths)?;
    if settings.selected_instance_id.as_deref() == Some(instance_id) {
        settings.selected_instance_id = instances
            .iter()
            .find(|candidate| candidate.id != instance_id)
            .map(|candidate| candidate.id.clone());
    }
    let instance_dir = paths.instances_dir.join(instance_id);
    let deleted_dir = deleted_instances_dir(paths);
    fs::create_dir_all(&deleted_dir)
        .map_err(|source| io_error("创建已删除实例目录", &deleted_dir, source))?;
    verify_deleted_instances_dir(paths, &deleted_dir)?;
    let deleted_instance_dir = deleted_dir.join(instance_id);
    if deleted_instance_dir.exists() {
        return Err(InstanceError::InvalidInstanceValue(
            "回收区已有相同标识的实例",
        ));
    }
    let shared_game = instance.version_directory.as_ref()
        .map(|_| instance.game_path(paths)).transpose()?;
    let parked = instance_dir.join(".shared-game");
    if let Some(game) = &shared_game {
        move_directory(game, &parked, "移出共享游戏目录")?;
    }
    let result: InstanceResult<()> = (|| {
        move_directory(&instance_dir, &deleted_instance_dir, "移动实例到回收区")?;
        if let Err(error) = save_settings(paths, &settings) {
            move_directory(&deleted_instance_dir, &instance_dir, "撤回实例删除")?;
            return Err(error.into());
        }
        Ok(())
    })();
    if result.is_err() {
        if let Some(game) = &shared_game {
            if parked.is_dir() {
                move_directory(&parked, game, "恢复共享游戏目录")?;
            }
        }
    }
    result?;
    Ok(settings)
}

pub fn restore_instance_in(paths: &AppPaths, instance_id: &str) -> InstanceResult<InstanceConfig> {
    let instance = list_deleted_instances_in(paths)?
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| InstanceError::InstanceNotFound(instance_id.to_owned()))?;
    let source = deleted_instances_dir(paths).join(instance_id);
    let destination = paths.instances_dir.join(instance_id);
    if destination.exists() {
        return Err(InstanceError::InvalidInstanceValue(
            "实例目录已存在，无法恢复",
        ));
    }
    let shared_game = instance.version_directory.as_ref()
        .map(|name| paths.minecraft_root.join("versions").join(name));
    let parked = source.join(".shared-game");
    if let Some(game) = &shared_game {
        move_directory(&parked, game, "恢复共享游戏目录")?;
    }
    if let Err(error) = move_directory(&source, &destination, "恢复实例") {
        if let Some(game) = &shared_game {
            move_directory(game, &parked, "撤回共享目录恢复")?;
        }
        return Err(error);
    }
    Ok(instance)
}

pub fn permanently_delete_instance_in(paths: &AppPaths, instance_id: &str) -> InstanceResult<()> {
    if !list_deleted_instances_in(paths)?
        .iter()
        .any(|instance| instance.id == instance_id)
    {
        return Err(InstanceError::InstanceNotFound(instance_id.to_owned()));
    }
    let source = deleted_instances_dir(paths).join(instance_id);
    fs::remove_dir_all(&source)
        .map_err(|source_error| io_error("彻底删除实例", &source, source_error))
}

fn move_directory(source: &Path, destination: &Path, action: &'static str) -> InstanceResult<()> {
    if !matches!(fs::symlink_metadata(destination), Err(error) if error.kind() == io::ErrorKind::NotFound) {
        return Err(InstanceError::InvalidInstanceValue("目标目录已经存在或无法访问"));
    }
    fs::rename(source, destination).map_err(|error| io_error(action, destination, error))
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
    if !valid_layout_schema(instance)
        || !valid_loader_config(&instance.loader)
        || instance.game_directory != Path::new("game")
        || !valid_version_directory(instance)
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

fn persist_instance_config(
    instance_dir: &Path,
    config_path: &Path,
    instance: &InstanceConfig,
) -> InstanceResult<()> {
    let mut contents =
        serde_json::to_vec_pretty(instance).map_err(InstanceError::SerializeConfig)?;
    contents.push(b'\n');
    let mut temporary = NamedTempFile::new_in(instance_dir)
        .map_err(|source| io_error("创建实例配置临时文件", instance_dir, source))?;
    temporary
        .write_all(&contents)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|source| io_error("写入实例配置临时文件", temporary.path(), source))?;
    temporary
        .persist(config_path)
        .map_err(|error| io_error("替换实例配置", config_path, error.error))?;
    Ok(())
}

fn valid_loader_config(loader: &LoaderConfig) -> bool {
    match loader.kind {
        GameLoader::Vanilla => loader.version.is_none() && loader.profile_id.is_none(),
        GameLoader::Fabric | GameLoader::Quilt | GameLoader::Forge | GameLoader::NeoForge => {
            loader.version.as_deref().is_some_and(valid_loader_value)
                && loader.profile_id.as_deref().is_some_and(valid_loader_value)
        }
    }
}

fn valid_loader_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_+".contains(character))
}

#[cfg(test)]
mod tests {
    use super::{
        create_instance_in, delete_instance_in, duplicate_instance_in, list_deleted_instances_in,
        list_instances_in, permanently_delete_instance_in, restore_instance_in, select_instance_in,
        update_instance_in, AdvancedSettings, CreateInstanceRequest, DisplaySettings,
        InstallationInfo, InstanceConfig, JavaSelection, LoaderConfig, MemorySettings,
        INSTANCE_SCHEMA_VERSION,
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
            loader: LoaderConfig::vanilla(),
            game_directory: PathBuf::from("game"),
            version_directory: None,
            can_rollback: false,
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
        assert!(created.game_path(&paths).unwrap().is_dir());
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
    fn migrates_legacy_vanilla_loader_config_atomically() {
        let (root, paths) = temporary_paths();
        let created = create_instance_in(
            &paths,
            CreateInstanceRequest {
                name: "旧版实例".to_string(),
                game_version: "1.20.1".to_string(),
            },
        )
        .expect("fixture instance should be created");
        let config_path = paths.instances_dir.join(&created.id).join("instance.json");
        let mut legacy = serde_json::to_value(&created).expect("fixture should serialize");
        legacy["schemaVersion"] = serde_json::json!(1);
        legacy.as_object_mut().unwrap().remove("versionDirectory");
        legacy["loader"] = serde_json::json!("vanilla");
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&legacy).expect("legacy fixture should serialize"),
        )
        .expect("legacy fixture should be written");

        let migrated = list_instances_in(&paths).expect("legacy instance should migrate");
        assert_eq!(migrated[0].schema_version, INSTANCE_SCHEMA_VERSION);
        assert!(migrated[0].loader.is_vanilla());
        let persisted: serde_json::Value = serde_json::from_slice(
            &fs::read(&config_path).expect("migrated config should be readable"),
        )
        .expect("migrated config should be valid");
        assert_eq!(persisted["schemaVersion"], INSTANCE_SCHEMA_VERSION);
        assert_eq!(persisted["loader"]["kind"], "vanilla");
        assert!(!config_path.with_extension("tmp").exists());

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
            fs::read_dir(paths.minecraft_root.join("versions"))
                .expect("versions directory should be readable")
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
            created.game_path(&paths).unwrap()
                .join("options.txt"),
            "guiScale:3",
        )
        .expect("fixture should be written");

        let duplicate =
            duplicate_instance_in(&paths, &created.id).expect("instance should duplicate");
        assert_eq!(duplicate.name, "原版生存 副本");
        assert!(duplicate.game_path(&paths).unwrap()
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

        let deleted = list_deleted_instances_in(&paths).expect("deleted instances should list");
        assert_eq!(deleted, vec![duplicate.clone()]);
        assert_eq!(
            fs::read_to_string(
                paths
                    .instances_dir
                    .join(".deleted")
                    .join(&duplicate.id)
                    .join(".shared-game")
                    .join("options.txt")
            )
            .expect("world files should remain recoverable"),
            "guiScale:3"
        );
        let restored = restore_instance_in(&paths, &duplicate.id).expect("instance should restore");
        assert_eq!(restored, duplicate);
        assert!(list_deleted_instances_in(&paths)
            .expect("deleted instances should list")
            .is_empty());
        assert!(duplicate.game_path(&paths).unwrap()
            .join("options.txt")
            .is_file());

        delete_instance_in(&paths, &duplicate.id).expect("restored instance should be deleted");
        permanently_delete_instance_in(&paths, &duplicate.id)
            .expect("deleted instance should be permanently removed");
        assert!(!paths
            .instances_dir
            .join(".deleted")
            .join(&duplicate.id)
            .exists());

        fs::remove_dir_all(root).expect("temporary tree should be removable");
    }

    #[test]
    fn restore_refuses_to_overwrite_an_existing_instance_directory() {
        let (root, paths) = temporary_paths();
        let created = create_instance_in(
            &paths,
            CreateInstanceRequest {
                name: "冲突测试".to_string(),
                game_version: "1.21.1".to_string(),
            },
        )
        .expect("instance should be created");
        delete_instance_in(&paths, &created.id).expect("instance should move to recycle area");
        let collision = paths.instances_dir.join(&created.id);
        fs::create_dir(&collision).expect("collision directory should be created");

        assert!(restore_instance_in(&paths, &created.id).is_err());
        assert!(paths
            .instances_dir
            .join(".deleted")
            .join(&created.id)
            .exists());
        assert!(collision.exists());

        fs::remove_dir_all(root).expect("temporary tree should be removable");
    }
}
