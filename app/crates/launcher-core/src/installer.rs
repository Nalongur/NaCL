use crate::data::{
    load_or_create_download_settings, load_or_create_settings, AppPaths, DataError,
    DownloadSettings,
};
use crate::instance::{
    self, CreateInstanceRequest, InstallationState, InstanceConfig, InstanceError,
};
use crate::java::{self, JavaError};
use crate::versions::{self, VersionError};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallInstanceRequest {
    pub name: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub stage: InstallStage,
    pub completed_files: usize,
    pub total_files: usize,
    pub current_file: String,
    pub downloaded_bytes: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallStage {
    Metadata,
    Libraries,
    Assets,
    Finalizing,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallControl {
    Running,
    Paused,
    Cancelled,
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

#[derive(Debug)]
pub enum InstallError {
    Data(DataError),
    Version(VersionError),
    Instance(InstanceError),
    Java(JavaError),
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
    )?;
    let metadata_contents = fs::read_to_string(&version_json_path)
        .map_err(|source| io_error("读取版本元数据", &version_json_path, source))?;
    let metadata: VersionMetadata =
        serde_json::from_str(&metadata_contents).map_err(|source| InstallError::InvalidJson {
            path: version_json_path.clone(),
            source,
        })?;

    if let Some(required_java) = metadata.java_version.as_ref() {
        let has_compatible_runtime = java::detect_java_runtimes()
            .iter()
            .any(|runtime| runtime.major_version == Some(required_java.major_version));
        if !has_compatible_runtime && launcher_settings.manage_runtimes {
            progress(InstallProgress {
                stage: InstallStage::Metadata,
                completed_files: 0,
                total_files: 0,
                current_file: format!("Java {} 运行环境", required_java.major_version),
                downloaded_bytes: 0,
            });
            java::install_managed_runtime(required_java.major_version)?;
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
        completed_files: library_downloads.len() + asset_downloads.len(),
        total_files: library_downloads.len() + asset_downloads.len(),
        current_file: "instance.json".to_string(),
        downloaded_bytes: 0,
    });
    let mut instance = instance::create_instance_in(
        &paths,
        CreateInstanceRequest {
            name: request.name,
            game_version: request.version_id,
        },
    )?;
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
        completed_files: library_downloads.len() + asset_downloads.len(),
        total_files: library_downloads.len() + asset_downloads.len(),
        current_file: instance.name.clone(),
        downloaded_bytes: 0,
    });
    Ok(instance)
}

fn build_client(settings: &DownloadSettings) -> Result<reqwest::blocking::Client, InstallError> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(u64::from(
            settings.connection_timeout_seconds,
        )))
        .user_agent("NaCL/0.1")
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
    let client = build_client(settings)?;
    let mut task_settings = settings.clone();
    if task_settings.speed_limit_kib_per_second > 0 {
        task_settings.speed_limit_kib_per_second = task_settings
            .speed_limit_kib_per_second
            .div_ceil(u32::from(task_settings.concurrent_downloads));
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(usize::from(settings.concurrent_downloads))
        .build()
        .map_err(|error| InstallError::ThreadPool(error.to_string()))?;

    pool.install(|| {
        downloads.par_iter().try_for_each(|download| {
            let bytes = download_one(&client, download, &task_settings, control)?;
            downloaded_bytes.fetch_add(bytes, Ordering::Relaxed);
            let current = completed.fetch_add(1, Ordering::Relaxed) + 1;
            progress(InstallProgress {
                stage,
                completed_files: current,
                total_files: total,
                current_file: download.label.clone(),
                downloaded_bytes: downloaded_bytes.load(Ordering::Relaxed),
            });
            Ok::<(), InstallError>(())
        })
    })
}

fn download_one(
    client: &reqwest::blocking::Client,
    download: &DownloadSpec,
    settings: &DownloadSettings,
    control: &(impl Fn() -> InstallControl + Sync),
) -> Result<u64, InstallError> {
    wait_for_control(control)?;
    if download.destination.is_file()
        && (download.sha1.is_empty() || file_sha1(&download.destination)? == download.sha1)
    {
        return Ok(0);
    }
    if let Some(parent) = download.destination.parent() {
        fs::create_dir_all(parent).map_err(|source| io_error("创建下载目录", parent, source))?;
    }
    let temporary = download.destination.with_extension("part");
    let mut last_error = None;
    for _ in 0..=settings.retry_count {
        match download_attempt(client, download, &temporary, settings, control) {
            Ok(bytes) => {
                if download.destination.is_file() {
                    fs::remove_file(&download.destination).map_err(|source| {
                        io_error("替换旧下载文件", &download.destination, source)
                    })?;
                }
                fs::rename(&temporary, &download.destination)
                    .map_err(|source| io_error("完成下载", &download.destination, source))?;
                return Ok(bytes);
            }
            Err(error) => {
                last_error = Some(error);
                let _ = fs::remove_file(&temporary);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| InstallError::ChecksumMismatch(download.label.clone())))
}

fn download_attempt(
    client: &reqwest::blocking::Client,
    download: &DownloadSpec,
    temporary: &Path,
    settings: &DownloadSettings,
    control: &(impl Fn() -> InstallControl + Sync),
) -> Result<u64, InstallError> {
    wait_for_control(control)?;
    let mut response = client
        .get(&download.url)
        .send()
        .map_err(InstallError::Network)?;
    if !response.status().is_success() {
        return Err(InstallError::HttpStatus {
            url: download.url.clone(),
            status: response.status(),
        });
    }
    let mut file = fs::File::create(temporary)
        .map_err(|source| io_error("创建临时下载文件", temporary, source))?;
    let mut hasher = Sha1::new();
    let mut total = 0_u64;
    let started = Instant::now();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        wait_for_control(control)?;
        let count = response
            .read(&mut buffer)
            .map_err(|source| io_error("读取下载响应", temporary, source))?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])
            .map_err(|source| io_error("写入临时下载文件", temporary, source))?;
        hasher.update(&buffer[..count]);
        total = total.saturating_add(count as u64);
        throttle_download(total, started, settings.speed_limit_kib_per_second);
    }
    file.sync_all()
        .map_err(|source| io_error("同步临时下载文件", temporary, source))?;
    let actual = format!("{:x}", hasher.finalize());
    if !download.sha1.is_empty() && actual != download.sha1 {
        return Err(InstallError::ChecksumMismatch(download.label.clone()));
    }
    if download.size > 0 && total != download.size {
        return Err(InstallError::ChecksumMismatch(download.label.clone()));
    }
    Ok(total)
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

fn throttle_download(total_bytes: u64, started: Instant, limit_kib_per_second: u32) {
    if limit_kib_per_second == 0 {
        return;
    }
    let expected =
        Duration::from_secs_f64(total_bytes as f64 / (f64::from(limit_kib_per_second) * 1024.0));
    let elapsed = started.elapsed();
    if expected > elapsed {
        std::thread::sleep((expected - elapsed).min(Duration::from_millis(250)));
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
        library_allowed_on_windows, safe_relative_path, Library, LibraryDownloads, Rule, RuleOs,
    };
    use std::collections::HashMap;

    #[test]
    fn rejects_parent_directory_download_paths() {
        assert!(safe_relative_path("../client.jar").is_err());
        assert!(safe_relative_path("com/example/client.jar").is_ok());
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
}
