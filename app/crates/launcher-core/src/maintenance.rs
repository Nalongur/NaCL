use crate::data::{load_or_create_settings, AppPaths, DataError};
use crate::installer::{
    self, InstallControl, InstallError, InstallInstanceRequest, InstallProgress,
};
use crate::instance::{self, GameLoader, InstanceConfig, InstanceError};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceFileIssue {
    pub path: String,
    pub category: String,
    pub state: String,
    pub detail: String,
    pub repairable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceFileReport {
    pub instance_id: String,
    pub scanned_files: usize,
    pub valid_files: usize,
    pub missing_files: usize,
    pub corrupted_files: usize,
    pub issues: Vec<InstanceFileIssue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairInstanceReport {
    pub before: InstanceFileReport,
    pub after: InstanceFileReport,
    pub repaired_files: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashFinding {
    pub code: String,
    pub severity: String,
    pub title: String,
    pub description: String,
    pub actions: Vec<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashAnalysis {
    pub instance_id: String,
    pub status: String,
    pub summary: String,
    pub source_files: Vec<String>,
    pub findings: Vec<CrashFinding>,
    pub analyzed_epoch_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportInstanceReport {
    pub destination: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug)]
pub enum MaintenanceError {
    Data(DataError),
    Instance(InstanceError),
    Installer(InstallError),
    InvalidMetadata(String),
    UnsafeDestination(PathBuf),
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    Archive(String),
}

impl fmt::Display for MaintenanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => error.fmt(formatter),
            Self::Instance(error) => error.fmt(formatter),
            Self::Installer(error) => error.fmt(formatter),
            Self::InvalidMetadata(message) => write!(formatter, "实例元数据无效：{message}"),
            Self::UnsafeDestination(path) => {
                write!(formatter, "导出目标不安全：{}", path.display())
            }
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "无法{action} {}：{source}", path.display()),
            Self::Json { path, source } => {
                write!(formatter, "JSON 文件 {} 无效：{source}", path.display())
            }
            Self::Archive(message) => write!(formatter, "实例导出失败：{message}"),
        }
    }
}

impl std::error::Error for MaintenanceError {}

impl From<DataError> for MaintenanceError {
    fn from(error: DataError) -> Self {
        Self::Data(error)
    }
}

impl From<InstanceError> for MaintenanceError {
    fn from(error: InstanceError) -> Self {
        Self::Instance(error)
    }
}

impl From<InstallError> for MaintenanceError {
    fn from(error: InstallError) -> Self {
        Self::Installer(error)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionMetadata {
    downloads: VersionDownloads,
    libraries: Vec<Library>,
    asset_index: Artifact,
}

#[derive(Debug, Deserialize)]
struct VersionDownloads {
    client: Artifact,
}

#[derive(Debug, Clone, Deserialize)]
struct Artifact {
    #[serde(default)]
    path: String,
    #[serde(default)]
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
    #[serde(rename = "name")]
    _name: String,
    downloads: LibraryDownloads,
    #[serde(default)]
    natives: HashMap<String, String>,
    #[serde(default)]
    rules: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
struct LibraryDownloads {
    artifact: Option<Artifact>,
    #[serde(default)]
    classifiers: HashMap<String, Artifact>,
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
struct ExpectedFile {
    path: PathBuf,
    category: &'static str,
    sha1: String,
    size: u64,
    repairable: bool,
}

pub fn scan_instance_files(instance_id: &str) -> Result<InstanceFileReport, MaintenanceError> {
    let paths = paths()?;
    let instance = instance_by_id(&paths, instance_id)?;
    scan_instance_files_in(&paths, &instance)
}

pub fn repair_instance_controlled<F, C>(
    instance_id: &str,
    progress: F,
    control: C,
) -> Result<RepairInstanceReport, MaintenanceError>
where
    F: Fn(InstallProgress) + Sync + Send,
    C: Fn() -> InstallControl + Sync + Send,
{
    let paths = paths()?;
    let instance = instance_by_id(&paths, instance_id)?;
    let before = scan_instance_files_in(&paths, &instance)?;
    let settings = load_or_create_settings(&paths)?;
    let previous_selection = settings.selected_instance_id;
    let loader_version =
        match instance.loader.kind {
            GameLoader::Vanilla => None,
            _ => {
                Some(instance.loader.version.clone().ok_or_else(|| {
                    MaintenanceError::InvalidMetadata("缺少加载器版本".to_string())
                })?)
            }
        };
    let temporary = installer::install_instance_controlled(
        InstallInstanceRequest {
            name: format!("NaCL 修复校验 {}", instance.name),
            version_id: instance.game_version.clone(),
            loader: Some(instance.loader.kind),
            loader_version,
        },
        progress,
        control,
    )?;

    let after_result = scan_instance_files_in(&paths, &instance);
    let cleanup_result = instance::delete_instance_in(&paths, &temporary.id);
    if let Some(selection) = previous_selection {
        let _ = instance::select_instance_in(&paths, &selection);
    }
    cleanup_result?;
    let after = after_result?;
    Ok(RepairInstanceReport {
        repaired_files: before.issues.len().saturating_sub(after.issues.len()),
        before,
        after,
    })
}

pub fn analyze_instance_crash(instance_id: &str) -> Result<CrashAnalysis, MaintenanceError> {
    let paths = paths()?;
    let instance = instance_by_id(&paths, instance_id)?;
    let root = paths
        .instances_dir
        .join(&instance.id)
        .join(&instance.game_directory);
    let mut candidates = Vec::new();
    collect_files(&root.join("crash-reports"), "txt", &mut candidates);
    collect_named_file(&root.join("logs").join("latest.log"), &mut candidates);
    if paths.logs_dir.is_dir() {
        for entry in fs::read_dir(&paths.logs_dir)
            .map_err(|source| io_error("读取游戏日志目录", &paths.logs_dir, source))?
        {
            let entry =
                entry.map_err(|source| io_error("读取游戏日志目录项", &paths.logs_dir, source))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&format!("game-{}-", instance.id)) && name.ends_with(".log") {
                candidates.push(entry.path());
            }
        }
    }
    candidates.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH)
    });
    candidates.reverse();
    candidates.truncate(4);

    let mut source_files = Vec::new();
    let mut text = String::new();
    for path in &candidates {
        let contents = read_tail(path, 2 * 1024 * 1024)?;
        if !contents.is_empty() {
            source_files.push(path.to_string_lossy().into_owned());
            text.push_str(&contents);
            text.push('\n');
        }
    }

    let lower = text.to_ascii_lowercase();
    let mut findings = Vec::new();
    add_finding(
        &mut findings,
        &text,
        &lower,
        &[
            "outofmemoryerror",
            "java heap space",
            "gc overhead limit exceeded",
        ],
        "memory",
        "error",
        "内存不足",
        "Java 堆内存不足，游戏或大型整合包无法继续运行。",
        &[
            "提高实例最大内存",
            "减少高占用 Mod 或资源包",
            "确认使用 64 位 Java",
        ],
    );
    add_finding(
        &mut findings,
        &text,
        &lower,
        &["duplicate mods", "duplicate mod", "found duplicate mods"],
        "duplicate-mod",
        "error",
        "检测到重复 Mod",
        "多个文件声明了相同的 Mod ID。",
        &["在模组与资源页面查看重复 ID", "禁用或移除其中一个重复文件"],
    );
    add_finding(
        &mut findings,
        &text,
        &lower,
        &[
            "requires version",
            "requires any version",
            "missing mandatory dependencies",
            "dependency resolution failed",
        ],
        "missing-dependency",
        "error",
        "Mod 前置缺失或版本不匹配",
        "加载器无法满足一个或多个 Mod 的依赖要求。",
        &[
            "查看 Mod 依赖诊断",
            "安装缺少的前置",
            "选择与游戏和加载器匹配的版本",
        ],
    );
    add_finding(
        &mut findings,
        &text,
        &lower,
        &["unsupportedclassversionerror", "class file version"],
        "java-version",
        "error",
        "Java 版本不兼容",
        "当前 Java 无法加载由其他 Java 版本编译的类。",
        &[
            "切换到该 Minecraft 版本要求的 Java",
            "恢复实例 Java 为自动选择",
        ],
    );
    add_finding(
        &mut findings,
        &text,
        &lower,
        &["noclassdeffounderror", "classnotfoundexception"],
        "missing-class",
        "error",
        "Mod 类缺失",
        "通常由缺少前置、Mod 版本错误或加载器不兼容引起。",
        &[
            "检查缺失前置",
            "更新或回退最近变更的 Mod",
            "确认 Mod 对应当前加载器",
        ],
    );
    add_finding(
        &mut findings,
        &text,
        &lower,
        &[
            "mixin apply failed",
            "mixintransformererror",
            "invalidmixinexception",
        ],
        "mixin",
        "error",
        "Mixin 注入失败",
        "至少一个 Mod 无法修改目标游戏类，常见原因是 Mod 冲突或版本不兼容。",
        &[
            "优先检查最近安装的 Mod",
            "核对游戏、加载器和 Mod 版本",
            "查看证据中的 Mixin 名称",
        ],
    );
    add_finding(
        &mut findings,
        &text,
        &lower,
        &["glfw error", "failed to create window", "opengl context"],
        "graphics",
        "error",
        "图形环境初始化失败",
        "Minecraft 无法建立窗口或 OpenGL 上下文。",
        &[
            "更新显卡驱动",
            "让 Java 使用独立显卡",
            "移除图形注入和光影组件后复试",
        ],
    );
    add_finding(
        &mut findings,
        &text,
        &lower,
        &["resolutionexception: modules", "export package"],
        "module-conflict",
        "error",
        "Java 模块冲突",
        "启动 classpath 中存在重复模块或加载器客户端文件冲突。",
        &[
            "运行实例文件修复",
            "重新安装对应加载器",
            "检查手动加入的启动参数",
        ],
    );

    let healthy = lower.contains("sound engine started")
        || lower.contains("created: 1024x")
        || lower.contains("loaded entity animations");
    let status = if findings.iter().any(|finding| finding.severity == "error") {
        "error"
    } else if text.is_empty() {
        "no-data"
    } else if healthy {
        "healthy"
    } else {
        "unknown"
    };
    let summary = match status {
        "error" => format!("发现 {} 个可识别的启动问题", findings.len()),
        "healthy" => "日志显示游戏已进入渲染或声音初始化阶段".to_string(),
        "no-data" => "没有找到可分析的游戏日志".to_string(),
        _ => "日志存在，但没有匹配到已知故障特征".to_string(),
    };
    Ok(CrashAnalysis {
        instance_id: instance.id,
        status: status.to_string(),
        summary,
        source_files,
        findings,
        analyzed_epoch_ms: now_epoch_ms(),
    })
}

pub fn export_instance(
    instance_id: &str,
    destination: PathBuf,
) -> Result<ExportInstanceReport, MaintenanceError> {
    let paths = paths()?;
    let instance = instance_by_id(&paths, instance_id)?;
    let root = paths.instances_dir.join(&instance.id);
    let destination = if destination.extension().and_then(|value| value.to_str()) == Some("zip") {
        destination
    } else {
        destination.with_extension("zip")
    };
    let parent = destination
        .parent()
        .ok_or_else(|| MaintenanceError::UnsafeDestination(destination.clone()))?;
    fs::create_dir_all(parent).map_err(|source| io_error("创建导出目录", parent, source))?;
    let canonical_root =
        fs::canonicalize(&root).map_err(|source| io_error("读取实例目录", &root, source))?;
    let canonical_parent =
        fs::canonicalize(parent).map_err(|source| io_error("读取导出目录", parent, source))?;
    if canonical_parent.starts_with(&canonical_root) {
        return Err(MaintenanceError::UnsafeDestination(destination));
    }

    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|source| io_error("创建导出临时文件", parent, source))?;
    let (files, bytes) = write_instance_archive(&mut temporary, &root, &instance)?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|source| io_error("同步实例导出文件", temporary.path(), source))?;
    if destination.exists() {
        return Err(MaintenanceError::UnsafeDestination(destination));
    }
    temporary
        .persist(&destination)
        .map_err(|error| io_error("完成实例导出", &destination, error.error))?;
    Ok(ExportInstanceReport {
        destination,
        files,
        bytes,
    })
}

fn scan_instance_files_in(
    paths: &AppPaths,
    instance: &InstanceConfig,
) -> Result<InstanceFileReport, MaintenanceError> {
    let metadata_id = instance
        .loader
        .profile_id
        .as_deref()
        .unwrap_or(&instance.game_version);
    let metadata_path = paths.versions_dir.join(format!("{metadata_id}.json"));
    let metadata_contents = match fs::read_to_string(&metadata_path) {
        Ok(contents) => contents,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Ok(single_issue_report(
                instance,
                &metadata_path,
                "metadata",
                "missing",
                "缺少启动版本元数据",
            ));
        }
        Err(source) => return Err(io_error("读取版本元数据", &metadata_path, source)),
    };
    let metadata: VersionMetadata = match serde_json::from_str(&metadata_contents) {
        Ok(metadata) => metadata,
        Err(source) => {
            return Ok(single_issue_report(
                instance,
                &metadata_path,
                "metadata",
                "invalid",
                &format!("版本元数据无法解析：{source}"),
            ));
        }
    };

    let mut expected = HashMap::<PathBuf, ExpectedFile>::new();
    let base_client = paths
        .versions_dir
        .join(format!("{}.jar", instance.game_version));
    insert_expected(
        &mut expected,
        ExpectedFile {
            path: base_client,
            category: "client",
            sha1: metadata.downloads.client.sha1.clone(),
            size: metadata.downloads.client.size,
            repairable: metadata.downloads.client.url.starts_with("https://"),
        },
    );
    if metadata_id != instance.game_version {
        insert_expected(
            &mut expected,
            ExpectedFile {
                path: paths.versions_dir.join(format!("{metadata_id}.jar")),
                category: "loader-client",
                sha1: metadata.downloads.client.sha1.clone(),
                size: metadata.downloads.client.size,
                repairable: true,
            },
        );
    }
    for library in metadata
        .libraries
        .into_iter()
        .filter(library_allowed_on_windows)
    {
        if let Some(artifact) = library.downloads.artifact {
            if let Some(expected_file) = expected_library(paths, artifact, "library") {
                insert_expected(&mut expected, expected_file);
            }
        }
        if let Some(classifier) = library.natives.get("windows") {
            let classifier = classifier.replace("${arch}", windows_arch());
            if let Some(artifact) = library.downloads.classifiers.get(&classifier).cloned() {
                if let Some(expected_file) = expected_library(paths, artifact, "native") {
                    insert_expected(&mut expected, expected_file);
                }
            }
        }
    }

    let asset_index_path = paths
        .assets_dir
        .join("indexes")
        .join(format!("{}.json", metadata.asset_index.id));
    insert_expected(
        &mut expected,
        ExpectedFile {
            path: asset_index_path.clone(),
            category: "asset-index",
            sha1: metadata.asset_index.sha1.clone(),
            size: metadata.asset_index.size,
            repairable: metadata.asset_index.url.starts_with("https://"),
        },
    );
    if let Ok(contents) = fs::read_to_string(&asset_index_path) {
        if let Ok(index) = serde_json::from_str::<AssetIndex>(&contents) {
            for object in index.objects.into_values() {
                let prefix = object.hash.get(..2).unwrap_or_default();
                insert_expected(
                    &mut expected,
                    ExpectedFile {
                        path: paths
                            .assets_dir
                            .join("objects")
                            .join(prefix)
                            .join(&object.hash),
                        category: "asset",
                        sha1: object.hash,
                        size: object.size,
                        repairable: true,
                    },
                );
            }
        }
    }

    let checks = expected
        .into_values()
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(check_file)
        .collect::<Vec<_>>();
    let mut issues = Vec::new();
    let mut valid_files = 0;
    let mut missing_files = 0;
    let mut corrupted_files = 0;
    for check in checks {
        match check {
            Ok(()) => valid_files += 1,
            Err(issue) => {
                if issue.state == "missing" {
                    missing_files += 1;
                } else {
                    corrupted_files += 1;
                }
                issues.push(issue);
            }
        }
    }
    issues.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(InstanceFileReport {
        instance_id: instance.id.clone(),
        scanned_files: valid_files + issues.len(),
        valid_files,
        missing_files,
        corrupted_files,
        issues,
    })
}

fn single_issue_report(
    instance: &InstanceConfig,
    path: &Path,
    category: &str,
    state: &str,
    detail: &str,
) -> InstanceFileReport {
    InstanceFileReport {
        instance_id: instance.id.clone(),
        scanned_files: 1,
        valid_files: 0,
        missing_files: usize::from(state == "missing"),
        corrupted_files: usize::from(state != "missing"),
        issues: vec![InstanceFileIssue {
            path: path.to_string_lossy().into_owned(),
            category: category.to_string(),
            state: state.to_string(),
            detail: detail.to_string(),
            repairable: true,
        }],
    }
}

fn check_file(expected: ExpectedFile) -> Result<(), InstanceFileIssue> {
    let issue = |state: &str, detail: String| InstanceFileIssue {
        path: expected.path.to_string_lossy().into_owned(),
        category: expected.category.to_string(),
        state: state.to_string(),
        detail,
        repairable: expected.repairable,
    };
    let metadata = fs::metadata(&expected.path)
        .map_err(|error| issue("missing", format!("文件不存在：{error}")))?;
    if !metadata.is_file() {
        return Err(issue("invalid", "目标不是普通文件".to_string()));
    }
    if expected.size > 0 && metadata.len() != expected.size {
        return Err(issue(
            "size-mismatch",
            format!("大小应为 {}，实际为 {}", expected.size, metadata.len()),
        ));
    }
    if !expected.sha1.is_empty() {
        let actual = file_sha1(&expected.path)
            .map_err(|error| issue("read-error", format!("无法计算 SHA-1：{error}")))?;
        if actual != expected.sha1.to_ascii_lowercase() {
            return Err(issue("checksum-mismatch", "SHA-1 不匹配".to_string()));
        }
    }
    Ok(())
}

fn expected_library(
    paths: &AppPaths,
    artifact: Artifact,
    category: &'static str,
) -> Option<ExpectedFile> {
    let relative = safe_relative_path(&artifact.path).ok()?;
    Some(ExpectedFile {
        path: paths.libraries_dir.join(relative),
        category,
        sha1: artifact.sha1,
        size: artifact.size,
        repairable: artifact.url.starts_with("https://") || category == "library",
    })
}

fn insert_expected(expected: &mut HashMap<PathBuf, ExpectedFile>, file: ExpectedFile) {
    expected.insert(file.path.clone(), file);
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
        let matches = rule.os.as_ref().is_none_or(|os| {
            os.name.as_deref().is_none_or(|name| name == "windows")
                && os.arch.as_deref().is_none_or(|arch| {
                    arch == std::env::consts::ARCH
                        || (arch == "x86_64" && std::env::consts::ARCH == "x86_64")
                })
        });
        if matches {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn windows_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86" => "32",
        _ => "64",
    }
}

fn safe_relative_path(value: &str) -> Result<PathBuf, MaintenanceError> {
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
        return Err(MaintenanceError::InvalidMetadata(format!(
            "不安全的相对路径 {value}"
        )));
    }
    Ok(path.to_path_buf())
}

fn file_sha1(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(directory: &Path, extension: &str, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            output.push(path);
        }
    }
}

fn collect_named_file(path: &Path, output: &mut Vec<PathBuf>) {
    if path.is_file() {
        output.push(path.to_path_buf());
    }
}

fn read_tail(path: &Path, limit: u64) -> Result<String, MaintenanceError> {
    let mut file = File::open(path).map_err(|source| io_error("读取游戏日志", path, source))?;
    let length = file
        .metadata()
        .map_err(|source| io_error("读取游戏日志属性", path, source))?
        .len();
    if length > limit {
        file.seek(io::SeekFrom::Start(length - limit))
            .map_err(|source| io_error("定位游戏日志", path, source))?;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|source| io_error("读取游戏日志", path, source))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[allow(clippy::too_many_arguments)]
fn add_finding(
    findings: &mut Vec<CrashFinding>,
    original: &str,
    lower: &str,
    patterns: &[&str],
    code: &str,
    severity: &str,
    title: &str,
    description: &str,
    actions: &[&str],
) {
    if !patterns.iter().any(|pattern| lower.contains(pattern)) {
        return;
    }
    let evidence = original
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            patterns.iter().any(|pattern| lower.contains(pattern))
        })
        .take(3)
        .map(sanitize_evidence)
        .collect();
    findings.push(CrashFinding {
        code: code.to_string(),
        severity: severity.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        actions: actions.iter().map(|action| (*action).to_string()).collect(),
        evidence,
    });
}

fn sanitize_evidence(line: &str) -> String {
    let mut value = line.trim().replace('\0', "");
    let lower = value.to_ascii_lowercase();
    if lower.contains("accesstoken") || lower.contains("authorization:") {
        return "[包含认证字段的日志行已隐藏]".to_string();
    }
    if value.chars().count() > 240 {
        value = value.chars().take(240).collect::<String>() + "…";
    }
    value
}

fn write_instance_archive(
    temporary: &mut NamedTempFile,
    root: &Path,
    instance: &InstanceConfig,
) -> Result<(usize, u64), MaintenanceError> {
    let mut archive = zip::ZipWriter::new(temporary.as_file_mut());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let manifest = serde_json::to_vec_pretty(&serde_json::json!({
        "schemaVersion": 1,
        "exportedEpochMs": now_epoch_ms(),
        "instance": instance,
        "excluded": ["game/logs", "game/crash-reports", ".nacl/natives", ".nacl/backups", ".nacl/trash"]
    }))
    .map_err(|source| MaintenanceError::Json {
        path: root.join("export-manifest.json"),
        source,
    })?;
    archive
        .start_file("export-manifest.json", options)
        .map_err(|error| MaintenanceError::Archive(error.to_string()))?;
    archive
        .write_all(&manifest)
        .map_err(|source| io_error("写入导出清单", root, source))?;
    let mut files = 1;
    let mut bytes = manifest.len() as u64;
    for entry in WalkDir::new(root).min_depth(1).follow_links(false) {
        let entry = entry.map_err(|error| MaintenanceError::Archive(error.to_string()))?;
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| MaintenanceError::Archive(error.to_string()))?;
        if export_excluded(relative) {
            continue;
        }
        let archive_path = relative.to_string_lossy().replace('\\', "/");
        if archive_path.contains("../") || archive_path.starts_with('/') {
            return Err(MaintenanceError::Archive(format!(
                "不安全的导出路径 {archive_path}"
            )));
        }
        archive
            .start_file(&archive_path, options)
            .map_err(|error| MaintenanceError::Archive(error.to_string()))?;
        let mut source = File::open(entry.path())
            .map_err(|error| io_error("读取实例导出文件", entry.path(), error))?;
        let copied = io::copy(&mut source, &mut archive)
            .map_err(|error| io_error("写入实例导出文件", entry.path(), error))?;
        files += 1;
        bytes += copied;
    }
    archive
        .finish()
        .map_err(|error| MaintenanceError::Archive(error.to_string()))?;
    Ok((files, bytes))
}

fn export_excluded(relative: &Path) -> bool {
    let value = relative.to_string_lossy().replace('\\', "/");
    [
        "game/logs/",
        "game/crash-reports/",
        ".nacl/natives/",
        ".nacl/backups/",
        ".nacl/trash/",
    ]
    .iter()
    .any(|prefix| value.starts_with(prefix))
}

fn paths() -> Result<AppPaths, MaintenanceError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    Ok(paths)
}

fn instance_by_id(paths: &AppPaths, instance_id: &str) -> Result<InstanceConfig, MaintenanceError> {
    instance::list_instances_in(paths)?
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| InstanceError::InstanceNotFound(instance_id.to_string()).into())
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> MaintenanceError {
    MaintenanceError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::{add_finding, export_excluded, sanitize_evidence, CrashFinding};
    use std::path::Path;

    #[test]
    fn detects_memory_crashes() {
        let text = "java.lang.OutOfMemoryError: Java heap space";
        let mut findings: Vec<CrashFinding> = Vec::new();
        add_finding(
            &mut findings,
            text,
            &text.to_ascii_lowercase(),
            &["outofmemoryerror"],
            "memory",
            "error",
            "内存不足",
            "test",
            &["test"],
        );
        assert_eq!(findings[0].code, "memory");
    }

    #[test]
    fn redacts_authentication_evidence() {
        assert_eq!(
            sanitize_evidence("--accessToken secret"),
            "[包含认证字段的日志行已隐藏]"
        );
    }

    #[test]
    fn excludes_runtime_and_private_logs_from_exports() {
        assert!(export_excluded(Path::new("game/logs/latest.log")));
        assert!(export_excluded(Path::new(".nacl/natives/lwjgl.dll")));
        assert!(!export_excluded(Path::new("game/mods/example.jar")));
    }
}
