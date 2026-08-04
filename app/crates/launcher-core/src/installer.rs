use crate::data::{
    load_or_create_download_settings, load_or_create_settings, AppPaths, DataError,
    DownloadSettings,
};
use crate::downloader::{self, DownloadEngine, DownloadOptions, DownloadRequest, TransferUpdate};
use crate::instance::{
    self, CreateInstanceRequest, GameLoader, InstallationState, InstanceConfig, InstanceError,
    LoaderConfig,
};
use crate::java::{self, JavaError};
use crate::loaders::{self, LoaderError};
use crate::versions::{self, VersionError};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;

pub use crate::downloader::DownloadControl as InstallControl;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallInstanceRequest {
    pub name: String,
    pub version_id: String,
    #[serde(default)]
    pub loader: Option<GameLoader>,
    #[serde(default)]
    pub loader_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub stage: InstallStage,
    pub completed_files: usize,
    pub total_files: usize,
    pub current_file: String,
    pub downloaded_bytes: u64,
    pub download_speed_bytes_per_second: u64,
    pub download_engine: DownloadEngine,
    pub active_connections: u8,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallStage {
    Metadata,
    Loader,
    Libraries,
    Assets,
    Finalizing,
    Complete,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionMetadata {
    downloads: VersionDownloads,
    libraries: Vec<Library>,
    asset_index: DownloadArtifact,
    java_version: Option<JavaVersionMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JavaVersionMetadata {
    major_version: u16,
}

#[derive(Debug, Deserialize)]
struct VersionDownloads {
    client: DownloadArtifact,
}

#[derive(Debug, Clone, Deserialize)]
struct DownloadArtifact {
    #[serde(default)]
    path: String,
    url: String,
    #[serde(default)]
    sha1: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    id: String,
}

#[derive(Debug, Deserialize)]
struct Library {
    #[allow(dead_code)]
    name: String,
    downloads: LibraryDownloads,
    #[serde(default)]
    natives: HashMap<String, String>,
    #[serde(default)]
    rules: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
struct LibraryDownloads {
    artifact: Option<DownloadArtifact>,
    #[serde(default)]
    classifiers: HashMap<String, DownloadArtifact>,
}

#[derive(Debug, Deserialize)]
struct Rule {
    action: String,
    os: Option<RuleOs>,
    features: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RuleOs {
    name: Option<String>,
    arch: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

#[derive(Debug, Clone)]
struct DownloadSpec {
    label: String,
    url: String,
    sha1: String,
    size: u64,
    destination: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct DownloadUpdate {
    downloaded_bytes: u64,
    speed_bytes_per_second: u64,
    engine: DownloadEngine,
    active_connections: u8,
}

impl Default for DownloadUpdate {
    fn default() -> Self {
        Self {
            downloaded_bytes: 0,
            speed_bytes_per_second: 0,
            engine: DownloadEngine::Cache,
            active_connections: 0,
        }
    }
}

#[derive(Debug)]
pub enum InstallError {
    Data(DataError),
    Version(VersionError),
    Instance(InstanceError),
    Java(JavaError),
    Loader(LoaderError),
    VersionNotFound(String),
    Network(reqwest::Error),
    HttpStatus {
        url: String,
        status: reqwest::StatusCode,
    },
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    InvalidJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    InvalidDownloadPath(String),
    ChecksumMismatch(String),
    Download(String),
    JavaRequired(u16),
    ThreadPool(String),
    Cancelled,
}

impl fmt::Display for InstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => error.fmt(formatter),
            Self::Version(error) => error.fmt(formatter),
            Self::Instance(error) => error.fmt(formatter),
            Self::Java(error) => error.fmt(formatter),
            Self::Loader(error) => error.fmt(formatter),
            Self::VersionNotFound(version) => write!(formatter, "官方清单中找不到版本 {version}"),
            Self::Network(error) => write!(formatter, "下载失败：{error}"),
            Self::HttpStatus { url, status } => {
                write!(formatter, "下载 {url} 时返回 HTTP {status}")
            }
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "无法{action} {}：{source}", path.display()),
            Self::InvalidJson { path, source } => {
                write!(formatter, "安装元数据 {} 无效：{source}", path.display())
            }
            Self::InvalidDownloadPath(path) => write!(formatter, "下载路径不安全：{path}"),
            Self::ChecksumMismatch(file) => write!(formatter, "文件校验失败：{file}"),
            Self::Download(message) => write!(formatter, "下载失败：{message}"),
            Self::JavaRequired(major) => write!(
                formatter,
                "此版本需要 Java {major}。请在设置的 Java 运行环境中下载，或启用运行环境自动管理"
            ),
            Self::ThreadPool(error) => write!(formatter, "无法创建下载任务池：{error}"),
            Self::Cancelled => write!(formatter, "安装已取消"),
        }
    }
}

impl std::error::Error for InstallError {}

impl From<DataError> for InstallError {
    fn from(error: DataError) -> Self {
        Self::Data(error)
    }
}

impl From<VersionError> for InstallError {
    fn from(error: VersionError) -> Self {
        Self::Version(error)
    }
}

impl From<InstanceError> for InstallError {
    fn from(error: InstanceError) -> Self {
        Self::Instance(error)
    }
}

impl From<JavaError> for InstallError {
    fn from(error: JavaError) -> Self {
        Self::Java(error)
    }
}

impl From<LoaderError> for InstallError {
    fn from(error: LoaderError) -> Self {
        Self::Loader(error)
    }
}

pub fn install_instance<F>(
    request: InstallInstanceRequest,
    progress: F,
) -> Result<InstanceConfig, InstallError>
where
    F: Fn(InstallProgress) + Sync + Send,
{
    install_instance_controlled(request, progress, || InstallControl::Running)
}

pub fn install_instance_controlled<F, C>(
    request: InstallInstanceRequest,
    progress: F,
    control: C,
) -> Result<InstanceConfig, InstallError>
where
    F: Fn(InstallProgress) + Sync + Send,
    C: Fn() -> InstallControl + Sync + Send,
{
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    let download_settings = load_or_create_download_settings(&paths)?;
    let ignore_download_update = |_: DownloadUpdate| {};
    let launcher_settings = load_or_create_settings(&paths)?;
    let catalog = versions::load_version_catalog(false)?;
    let version = catalog
        .versions
        .into_iter()
        .find(|candidate| candidate.id == request.version_id)
        .ok_or_else(|| InstallError::VersionNotFound(request.version_id.clone()))?;

    progress(InstallProgress {
        stage: InstallStage::Metadata,
        completed_files: 0,
        total_files: 0,
        current_file: format!("{}.json", version.id),
        downloaded_bytes: 0,
        download_speed_bytes_per_second: 0,
        download_engine: DownloadEngine::Streaming,
        active_connections: 0,
    });

    let version_json_path = paths.versions_dir.join(format!("{}.json", version.id));
    download_one(
        &build_client(&download_settings)?,
        &DownloadSpec {
            label: format!("{}.json", version.id),
            url: version.url,
            sha1: version.sha1,
            size: 0,
            destination: version_json_path.clone(),
        },
        &download_settings,
        &control,
        &ignore_download_update,
    )?;
    let metadata_contents = fs::read_to_string(&version_json_path)
        .map_err(|source| io_error("读取版本元数据", &version_json_path, source))?;
    let metadata_value: serde_json::Value =
        serde_json::from_str(&metadata_contents).map_err(|source| InstallError::InvalidJson {
            path: version_json_path.clone(),
            source,
        })?;
    let metadata: VersionMetadata =
        serde_json::from_str(&metadata_contents).map_err(|source| InstallError::InvalidJson {
            path: version_json_path.clone(),
            source,
        })?;
    let required_java_major = metadata
        .java_version
        .as_ref()
        .map_or(8, |java| java.major_version);

    if let Some(required_java) = metadata.java_version.as_ref() {
        let has_compatible_runtime = java::detect_java_runtimes()
            .iter()
            .any(|runtime| runtime.major_version == Some(required_java.major_version));
        if !has_compatible_runtime {
            if launcher_settings.manage_runtimes {
                progress(InstallProgress {
                    stage: InstallStage::Metadata,
                    completed_files: 0,
                    total_files: 0,
                    current_file: format!("Java {} 运行环境", required_java.major_version),
                    downloaded_bytes: 0,
                    download_speed_bytes_per_second: 0,
                    download_engine: DownloadEngine::Streaming,
                    active_connections: 0,
                });
                java::install_managed_runtime_controlled(
                    required_java.major_version,
                    &control,
                    &|update| {
                        progress(InstallProgress {
                            stage: InstallStage::Metadata,
                            completed_files: 0,
                            total_files: 0,
                            current_file: format!("Java {} 运行环境", required_java.major_version),
                            downloaded_bytes: update.downloaded_bytes,
                            download_speed_bytes_per_second: update.speed_bytes_per_second,
                            download_engine: update.engine,
                            active_connections: update.active_connections,
                        });
                    },
                )
                .map_err(|error| match error {
                    JavaError::Cancelled => InstallError::Cancelled,
                    error => InstallError::Java(error),
                })?;
            } else {
                return Err(InstallError::JavaRequired(required_java.major_version));
            }
        }
    }

    let asset_index_path = paths
        .assets_dir
        .join("indexes")
        .join(format!("{}.json", metadata.asset_index.id));
    download_one(
        &build_client(&download_settings)?,
        &DownloadSpec {
            label: format!("assets/indexes/{}.json", metadata.asset_index.id),
            url: metadata.asset_index.url.clone(),
            sha1: metadata.asset_index.sha1.clone(),
            size: metadata.asset_index.size,
            destination: asset_index_path.clone(),
        },
        &download_settings,
        &control,
        &ignore_download_update,
    )?;

    let mut library_downloads = Vec::new();
    for library in metadata
        .libraries
        .into_iter()
        .filter(library_allowed_on_windows)
    {
        if let Some(artifact) = library.downloads.artifact {
            library_downloads.push(spec_from_library(&paths, artifact)?);
        }
        if let Some(classifier) = library.natives.get("windows") {
            let classifier = classifier.replace("${arch}", windows_arch());
            if let Some(artifact) = library.downloads.classifiers.get(&classifier) {
                library_downloads.push(spec_from_library(&paths, artifact.clone())?);
            }
        }
    }
    library_downloads.push(DownloadSpec {
        label: format!("client/{}.jar", version.id),
        url: metadata.downloads.client.url,
        sha1: metadata.downloads.client.sha1,
        size: metadata.downloads.client.size,
        destination: paths.versions_dir.join(format!("{}.jar", version.id)),
    });
    download_group(
        &library_downloads,
        InstallStage::Libraries,
        &download_settings,
        &progress,
        &control,
    )?;

    let loader_kind = request.loader.unwrap_or(GameLoader::Vanilla);
    let mut loader_config = LoaderConfig::vanilla();
    let mut loader_artifact_count = 0;
    if loader_kind != GameLoader::Vanilla {
        let loader_version = request
            .loader_version
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| LoaderError::VersionNotFound("未选择版本".to_string()))?;
        let plan = match loader_kind {
            GameLoader::Fabric | GameLoader::Quilt => loaders::plan_profile_loader_install(
                loader_kind,
                &request.version_id,
                loader_version,
                &metadata_value,
            )?,
            GameLoader::Forge | GameLoader::NeoForge => install_processor_loader(
                &paths,
                loader_kind,
                &request.version_id,
                loader_version,
                required_java_major,
                &metadata_value,
                &download_settings,
                &progress,
                &control,
            )?,
            GameLoader::Vanilla => unreachable!(),
        };
        let loader_downloads = plan
            .artifacts
            .iter()
            .map(|artifact| {
                Ok(DownloadSpec {
                    label: artifact.label.clone(),
                    url: artifact.url.clone(),
                    sha1: artifact.sha1.clone(),
                    size: artifact.size,
                    destination: paths
                        .libraries_dir
                        .join(safe_relative_path(&artifact.relative_path)?),
                })
            })
            .collect::<Result<Vec<_>, InstallError>>()?;
        loader_artifact_count = loader_downloads.len();
        download_group(
            &loader_downloads,
            InstallStage::Loader,
            &download_settings,
            &progress,
            &control,
        )?;
        let profile_id = plan
            .config
            .profile_id
            .as_deref()
            .ok_or_else(|| LoaderError::InvalidProfile("缺少 profile id".to_string()))?;
        if !valid_profile_id(profile_id) {
            return Err(
                LoaderError::InvalidProfile("profile id 不是安全文件名".to_string()).into(),
            );
        }
        let effective_path = paths.versions_dir.join(format!("{profile_id}.json"));
        let mut effective_contents =
            serde_json::to_vec_pretty(&plan.effective_profile).map_err(|source| {
                InstallError::InvalidJson {
                    path: effective_path.clone(),
                    source,
                }
            })?;
        effective_contents.push(b'\n');
        let mut temporary = NamedTempFile::new_in(&paths.versions_dir).map_err(|source| {
            io_error("创建加载器启动配置临时文件", &paths.versions_dir, source)
        })?;
        temporary
            .write_all(&effective_contents)
            .map_err(|source| io_error("写入加载器启动配置", temporary.path(), source))?;
        temporary
            .as_file_mut()
            .sync_all()
            .map_err(|source| io_error("同步加载器启动配置", temporary.path(), source))?;
        temporary
            .persist(&effective_path)
            .map_err(|error| io_error("替换加载器启动配置", &effective_path, error.error))?;
        ensure_loader_client_alias(&paths, &request.version_id, profile_id)?;
        loader_config = plan.config;
    }

    let asset_contents = fs::read_to_string(&asset_index_path)
        .map_err(|source| io_error("读取资源索引", &asset_index_path, source))?;
    let asset_index: AssetIndex =
        serde_json::from_str(&asset_contents).map_err(|source| InstallError::InvalidJson {
            path: asset_index_path,
            source,
        })?;
    let asset_downloads = asset_index
        .objects
        .into_iter()
        .map(|(name, object)| {
            let prefix = object.hash.get(..2).unwrap_or_default();
            DownloadSpec {
                label: name,
                url: format!(
                    "https://resources.download.minecraft.net/{prefix}/{}",
                    object.hash
                ),
                sha1: object.hash.clone(),
                size: object.size,
                destination: paths
                    .assets_dir
                    .join("objects")
                    .join(prefix)
                    .join(object.hash),
            }
        })
        .collect::<Vec<_>>();
    download_group(
        &asset_downloads,
        InstallStage::Assets,
        &download_settings,
        &progress,
        &control,
    )?;

    progress(InstallProgress {
        stage: InstallStage::Finalizing,
        completed_files: library_downloads.len() + asset_downloads.len() + loader_artifact_count,
        total_files: library_downloads.len() + asset_downloads.len() + loader_artifact_count,
        current_file: "instance.json".to_string(),
        downloaded_bytes: 0,
        download_speed_bytes_per_second: 0,
        download_engine: DownloadEngine::Streaming,
        active_connections: 0,
    });
    let mut instance = instance::create_instance_in(
        &paths,
        CreateInstanceRequest {
            name: request.name,
            game_version: request.version_id,
        },
    )?;
    instance.loader = loader_config;
    instance.installation.state = InstallationState::Ready;
    instance.installation.installed_epoch_ms = Some(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis()),
    );
    let created_instance_id = instance.id.clone();
    let instance = match instance::update_instance_in(&paths, instance) {
        Ok(instance) => instance,
        Err(error) => {
            let _ = instance::delete_instance_in(&paths, &created_instance_id);
            return Err(error.into());
        }
    };
    progress(InstallProgress {
        stage: InstallStage::Complete,
        completed_files: library_downloads.len() + asset_downloads.len() + loader_artifact_count,
        total_files: library_downloads.len() + asset_downloads.len() + loader_artifact_count,
        current_file: instance.name.clone(),
        downloaded_bytes: 0,
        download_speed_bytes_per_second: 0,
        download_engine: DownloadEngine::Streaming,
        active_connections: 0,
    });
    Ok(instance)
}

fn build_client(settings: &DownloadSettings) -> Result<reqwest::blocking::Client, InstallError> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(u64::from(
            settings.connection_timeout_seconds,
        )))
        .user_agent("Nalongur/NaCL/0.4.0")
        .build()
        .map_err(InstallError::Network)
}

fn spec_from_library(
    paths: &AppPaths,
    artifact: DownloadArtifact,
) -> Result<DownloadSpec, InstallError> {
    let relative = safe_relative_path(&artifact.path)?;
    Ok(DownloadSpec {
        label: artifact.path,
        url: artifact.url,
        sha1: artifact.sha1,
        size: artifact.size,
        destination: paths.libraries_dir.join(relative),
    })
}

fn safe_relative_path(value: &str) -> Result<PathBuf, InstallError> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(InstallError::InvalidDownloadPath(value.to_owned()));
    }
    Ok(path.to_path_buf())
}

#[allow(clippy::too_many_arguments)]
fn install_processor_loader<F, C>(
    paths: &AppPaths,
    loader: GameLoader,
    game_version: &str,
    loader_version: &str,
    required_java_major: u16,
    base_profile: &serde_json::Value,
    download_settings: &DownloadSettings,
    progress: &F,
    control: &C,
) -> Result<loaders::LoaderInstallPlan, InstallError>
where
    F: Fn(InstallProgress) + Sync + Send,
    C: Fn() -> InstallControl + Sync + Send,
{
    let installer = loaders::plan_installer_loader_install(loader, loader_version)?;
    let installer_path = paths.downloads_dir.join(&installer.installer_file_name);
    progress(InstallProgress {
        stage: InstallStage::Loader,
        completed_files: 0,
        total_files: 1,
        current_file: installer.installer_file_name.clone(),
        downloaded_bytes: 0,
        download_speed_bytes_per_second: 0,
        download_engine: DownloadEngine::Streaming,
        active_connections: 0,
    });
    download_one(
        &build_client(download_settings)?,
        &DownloadSpec {
            label: installer.installer_file_name.clone(),
            url: installer.installer_url,
            sha1: installer.installer_sha1,
            size: 0,
            destination: installer_path.clone(),
        },
        download_settings,
        control,
        &|update| {
            progress(InstallProgress {
                stage: InstallStage::Loader,
                completed_files: 0,
                total_files: 1,
                current_file: installer.installer_file_name.clone(),
                downloaded_bytes: update.downloaded_bytes,
                download_speed_bytes_per_second: update.speed_bytes_per_second,
                download_engine: update.engine,
                active_connections: update.active_connections,
            });
        },
    )?;

    let runtime = java::detect_java_runtimes()
        .into_iter()
        .find(|runtime| runtime.major_version == Some(required_java_major))
        .ok_or(InstallError::JavaRequired(required_java_major))?;
    let launcher_profiles = paths.cache_dir.join("launcher_profiles.json");
    if !launcher_profiles.is_file() {
        fs::write(&launcher_profiles, b"{\"profiles\":{}}\n")
            .map_err(|source| io_error("创建隔离安装器配置", &launcher_profiles, source))?;
    }
    let process_log = paths
        .downloads_dir
        .join(format!("{}-install.log", installer.installer_file_name));
    let output = fs::File::create(&process_log)
        .map_err(|source| io_error("创建加载器安装日志", &process_log, source))?;
    let error_output = output
        .try_clone()
        .map_err(|source| io_error("复制加载器安装日志句柄", &process_log, source))?;
    let mut command = Command::new(&runtime.path);
    command
        .arg("-jar")
        .arg(&installer_path)
        .arg("--installClient")
        .arg(&paths.cache_dir)
        .current_dir(&paths.downloads_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(error_output));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|source| io_error("运行加载器安装器", &installer_path, source))?;
    let status = loop {
        if control() == InstallControl::Cancelled {
            let _ = child.kill();
            let _ = child.wait();
            return Err(InstallError::Cancelled);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|source| io_error("读取加载器安装器状态", &installer_path, source))?
        {
            break status;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if !status.success() {
        let detail = fs::read_to_string(&process_log)
            .unwrap_or_default()
            .chars()
            .rev()
            .take(2000)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        return Err(InstallError::Download(format!(
            "{} 安装器退出码 {:?}：{}",
            match loader {
                GameLoader::Forge => "Forge",
                GameLoader::NeoForge => "NeoForge",
                _ => "加载器",
            },
            status.code(),
            detail.trim()
        )));
    }

    let installed_profile =
        find_installed_loader_profile(paths, loader, game_version, loader_version)?;
    loaders::merge_installed_loader_profile(
        loader,
        game_version,
        loader_version,
        base_profile,
        &installed_profile,
    )
    .map_err(InstallError::from)
}

fn find_installed_loader_profile(
    paths: &AppPaths,
    loader: GameLoader,
    game_version: &str,
    loader_version: &str,
) -> Result<serde_json::Value, InstallError> {
    let needle = loader_version
        .strip_prefix(&format!("{game_version}-"))
        .unwrap_or(loader_version)
        .to_ascii_lowercase();
    let mut matches = Vec::new();
    let entries = walkdir::WalkDir::new(&paths.versions_dir)
        .min_depth(1)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok);
    for entry in entries {
        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(_) => continue,
        };
        let value: serde_json::Value = match serde_json::from_str(&contents) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let id = value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let main_class = value
            .get("mainClass")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let inherits = value
            .get("inheritsFrom")
            .and_then(serde_json::Value::as_str);
        let loader_marker = match loader {
            GameLoader::Forge => id.contains("forge") || main_class.contains("forge"),
            GameLoader::NeoForge => id.contains("neoforge") || main_class.contains("neoforge"),
            _ => false,
        };
        if inherits == Some(game_version) && loader_marker && id.contains(&needle) {
            let modified = fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(UNIX_EPOCH);
            matches.push((modified, value));
        }
    }
    matches
        .into_iter()
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, value)| value)
        .ok_or_else(|| {
            LoaderError::InvalidProfile("安装器没有生成可识别的客户端 profile".to_string()).into()
        })
}

fn ensure_loader_client_alias(
    paths: &AppPaths,
    game_version: &str,
    profile_id: &str,
) -> Result<(), InstallError> {
    let source = paths.versions_dir.join(format!("{game_version}.jar"));
    let destination = paths.versions_dir.join(format!("{profile_id}.jar"));
    if destination.is_file() && file_sha1(&destination)? == file_sha1(&source)? {
        return Ok(());
    }

    let temporary = paths
        .versions_dir
        .join(format!(".{profile_id}.{}.jar.tmp", uuid::Uuid::new_v4()));
    if fs::hard_link(&source, &temporary).is_err() {
        fs::copy(&source, &temporary)
            .map_err(|source_error| io_error("复制加载器客户端别名", &temporary, source_error))?;
    }
    if destination.exists() {
        if let Err(source_error) = fs::remove_file(&destination) {
            let _ = fs::remove_file(&temporary);
            return Err(io_error("替换加载器客户端别名", &destination, source_error));
        }
    }
    if let Err(source_error) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_file(&temporary);
        return Err(io_error("完成加载器客户端别名", &destination, source_error));
    }
    if file_sha1(&destination)? != file_sha1(&source)? {
        return Err(InstallError::ChecksumMismatch(
            destination.to_string_lossy().into_owned(),
        ));
    }
    Ok(())
}

fn valid_profile_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_+".contains(character))
}

fn library_allowed_on_windows(library: &Library) -> bool {
    if library.rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in &library.rules {
        if rule.features.is_some() {
            continue;
        }
        let matches_os = rule.os.as_ref().is_none_or(|os| {
            os.name.as_deref().is_none_or(|name| name == "windows")
                && os.arch.as_deref().is_none_or(|arch| {
                    arch.eq_ignore_ascii_case(windows_arch())
                        || (arch == "x86" && windows_arch() == "64")
                })
        });
        if matches_os {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn windows_arch() -> &'static str {
    if cfg!(target_pointer_width = "64") {
        "64"
    } else {
        "32"
    }
}

fn download_group<F>(
    downloads: &[DownloadSpec],
    stage: InstallStage,
    settings: &DownloadSettings,
    progress: &F,
    control: &(impl Fn() -> InstallControl + Sync),
) -> Result<(), InstallError>
where
    F: Fn(InstallProgress) + Sync + Send,
{
    let total = downloads.len();
    let completed = AtomicUsize::new(0);
    let downloaded_bytes = AtomicU64::new(0);
    let download_speed = AtomicU64::new(0);
    let last_progress_emit = Mutex::new(
        Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now),
    );
    let client = build_client(settings)?;
    let mut task_settings = settings.clone();
    if task_settings.speed_limit_kib_per_second > 0 {
        task_settings.speed_limit_kib_per_second = task_settings
            .speed_limit_kib_per_second
            .div_ceil(u32::from(task_settings.concurrent_downloads));
    }
    let global_connection_budget = 16_u8;
    let per_file_budget = (global_connection_budget / settings.concurrent_downloads.max(1)).max(1);
    task_settings.connections_per_download =
        task_settings.connections_per_download.min(per_file_budget);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(usize::from(settings.concurrent_downloads))
        .build()
        .map_err(|error| InstallError::ThreadPool(error.to_string()))?;

    pool.install(|| {
        downloads.par_iter().try_for_each(|download| {
            let task_downloaded_bytes = AtomicU64::new(0);
            let task_speed = AtomicU64::new(0);
            let report_download = |update: DownloadUpdate| {
                replace_atomic_contribution(
                    &downloaded_bytes,
                    &task_downloaded_bytes,
                    update.downloaded_bytes,
                );
                replace_atomic_contribution(
                    &download_speed,
                    &task_speed,
                    update.speed_bytes_per_second,
                );

                let should_emit = {
                    let mut last_emit = last_progress_emit
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if last_emit.elapsed() >= Duration::from_millis(250) {
                        *last_emit = Instant::now();
                        true
                    } else {
                        false
                    }
                };
                if should_emit {
                    progress(InstallProgress {
                        stage,
                        completed_files: completed.load(Ordering::Relaxed),
                        total_files: total,
                        current_file: download.label.clone(),
                        downloaded_bytes: downloaded_bytes.load(Ordering::Relaxed),
                        download_speed_bytes_per_second: download_speed.load(Ordering::Relaxed),
                        download_engine: update.engine,
                        active_connections: update.active_connections,
                    });
                }
            };
            let outcome =
                download_one(&client, download, &task_settings, control, &report_download)?;
            let current = completed.fetch_add(1, Ordering::Relaxed) + 1;
            progress(InstallProgress {
                stage,
                completed_files: current,
                total_files: total,
                current_file: download.label.clone(),
                downloaded_bytes: downloaded_bytes.load(Ordering::Relaxed),
                download_speed_bytes_per_second: download_speed.load(Ordering::Relaxed),
                download_engine: outcome.engine,
                active_connections: 0,
            });
            Ok::<(), InstallError>(())
        })
    })
}

fn replace_atomic_contribution(total: &AtomicU64, previous: &AtomicU64, value: u64) {
    let old = previous.swap(value, Ordering::Relaxed);
    if value >= old {
        total.fetch_add(value - old, Ordering::Relaxed);
    } else {
        total.fetch_sub(old - value, Ordering::Relaxed);
    }
}

fn download_one<P>(
    client: &reqwest::blocking::Client,
    download: &DownloadSpec,
    settings: &DownloadSettings,
    control: &(impl Fn() -> InstallControl + Sync),
    progress: &P,
) -> Result<downloader::DownloadOutcome, InstallError>
where
    P: Fn(DownloadUpdate) + Sync,
{
    wait_for_control(control)?;
    if download.destination.is_file()
        && (download.sha1.is_empty() || file_sha1(&download.destination)? == download.sha1)
    {
        progress(DownloadUpdate::default());
        return Ok(downloader::DownloadOutcome {
            downloaded_bytes: 0,
            engine: DownloadEngine::Cache,
        });
    }
    if let Some(parent) = download.destination.parent() {
        fs::create_dir_all(parent).map_err(|source| io_error("创建下载目录", parent, source))?;
    }
    let mut temporary_name = download.destination.as_os_str().to_os_string();
    temporary_name.push(".part");
    let temporary = PathBuf::from(temporary_name);
    let mut last_error = None;
    for _ in 0..=settings.retry_count {
        let attempt = downloader::download(
            client,
            DownloadRequest {
                label: &download.label,
                url: &download.url,
                expected_size: download.size,
                identity: &download.sha1,
                temporary_path: &temporary,
            },
            DownloadOptions {
                segment_connections: settings.connections_per_download,
                segment_threshold_bytes: u64::from(settings.segmented_download_threshold_mib)
                    * 1024
                    * 1024,
                retry_count: settings.retry_count,
                speed_limit_kib_per_second: settings.speed_limit_kib_per_second,
            },
            control,
            &|update: TransferUpdate| {
                progress(DownloadUpdate {
                    downloaded_bytes: update.downloaded_bytes,
                    speed_bytes_per_second: update.speed_bytes_per_second,
                    engine: update.engine,
                    active_connections: update.active_connections,
                });
            },
        )
        .map_err(|error| match error {
            downloader::DownloadError::Cancelled => InstallError::Cancelled,
            error => InstallError::Download(format!("{}：{error}", download.label)),
        });
        match attempt {
            Ok(outcome) => {
                let bytes = outcome.downloaded_bytes;
                if !download.sha1.is_empty()
                    && file_sha1(&temporary)? != download.sha1.to_ascii_lowercase()
                {
                    last_error = Some(InstallError::ChecksumMismatch(download.label.clone()));
                    cleanup_partial_download(&temporary);
                    progress(DownloadUpdate::default());
                    continue;
                }
                if download.size > 0
                    && fs::metadata(&temporary)
                        .map_err(|source| io_error("读取临时下载文件", &temporary, source))?
                        .len()
                        != download.size
                {
                    last_error = Some(InstallError::ChecksumMismatch(download.label.clone()));
                    cleanup_partial_download(&temporary);
                    progress(DownloadUpdate::default());
                    continue;
                }
                if download.destination.is_file() {
                    fs::remove_file(&download.destination).map_err(|source| {
                        io_error("替换旧下载文件", &download.destination, source)
                    })?;
                }
                fs::rename(&temporary, &download.destination)
                    .map_err(|source| io_error("完成下载", &download.destination, source))?;
                progress(DownloadUpdate {
                    downloaded_bytes: bytes,
                    speed_bytes_per_second: 0,
                    engine: outcome.engine,
                    active_connections: 0,
                });
                return Ok(outcome);
            }
            Err(InstallError::Cancelled) => {
                downloader::cleanup_partial_state(&temporary);
                progress(DownloadUpdate::default());
                return Err(InstallError::Cancelled);
            }
            Err(error) => {
                last_error = Some(error);
                downloader::cleanup_partial_state(&temporary);
                progress(DownloadUpdate::default());
            }
        }
    }
    Err(last_error.unwrap_or_else(|| InstallError::ChecksumMismatch(download.label.clone())))
}

fn cleanup_partial_download(temporary: &Path) {
    downloader::cleanup_partial_state(temporary);
}

fn wait_for_control(control: &(impl Fn() -> InstallControl + Sync)) -> Result<(), InstallError> {
    loop {
        match control() {
            InstallControl::Running => return Ok(()),
            InstallControl::Cancelled => return Err(InstallError::Cancelled),
            InstallControl::Paused => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn file_sha1(path: &Path) -> Result<String, InstallError> {
    let mut file = fs::File::open(path).map_err(|source| io_error("读取缓存文件", path, source))?;
    let mut hasher = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|source| io_error("校验缓存文件", path, source))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> InstallError {
    InstallError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_loader_client_alias, find_installed_loader_profile, library_allowed_on_windows,
        safe_relative_path, valid_profile_id, Library, LibraryDownloads, Rule, RuleOs,
    };
    use crate::data::AppPaths;
    use crate::instance::GameLoader;
    use std::collections::HashMap;
    use std::fs;

    #[test]
    fn rejects_parent_directory_download_paths() {
        assert!(safe_relative_path("../client.jar").is_err());
        assert!(safe_relative_path("com/example/client.jar").is_ok());
    }

    #[test]
    fn rejects_unsafe_loader_profile_ids() {
        assert!(valid_profile_id("neoforge-21.1.248"));
        assert!(!valid_profile_id("../outside"));
        assert!(!valid_profile_id("nested/profile"));
    }

    #[test]
    fn applies_windows_library_rules() {
        let library = Library {
            name: "test".to_string(),
            downloads: LibraryDownloads {
                artifact: None,
                classifiers: HashMap::new(),
            },
            natives: HashMap::new(),
            rules: vec![Rule {
                action: "allow".to_string(),
                os: Some(RuleOs {
                    name: Some("windows".to_string()),
                    arch: None,
                }),
                features: None,
            }],
        };
        assert!(library_allowed_on_windows(&library));
    }

    #[test]
    fn finds_nested_installer_profile() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_roots(
            temporary.path().join("roaming"),
            temporary.path().join("local"),
        );
        let profile_id = "1.21.1-forge-52.1.16";
        let profile_dir = paths.versions_dir.join(profile_id);
        fs::create_dir_all(&profile_dir).expect("profile directory");
        fs::write(
            profile_dir.join(format!("{profile_id}.json")),
            serde_json::json!({
                "id": profile_id,
                "inheritsFrom": "1.21.1",
                "mainClass": "net.minecraftforge.bootstrap.ForgeBootstrap"
            })
            .to_string(),
        )
        .expect("profile");

        let profile =
            find_installed_loader_profile(&paths, GameLoader::Forge, "1.21.1", "1.21.1-52.1.16")
                .expect("nested profile");
        assert_eq!(profile["id"], profile_id);
    }

    #[test]
    fn creates_validated_loader_client_alias() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_roots(
            temporary.path().join("roaming"),
            temporary.path().join("local"),
        );
        fs::create_dir_all(&paths.versions_dir).expect("versions directory");
        fs::write(paths.versions_dir.join("1.21.1.jar"), b"client").expect("client");

        ensure_loader_client_alias(&paths, "1.21.1", "neoforge-21.1.248").expect("client alias");
        assert_eq!(
            fs::read(paths.versions_dir.join("neoforge-21.1.248.jar")).expect("alias"),
            b"client"
        );
    }
}
