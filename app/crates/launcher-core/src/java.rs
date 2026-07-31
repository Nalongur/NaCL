use crate::data::{AppPaths, DataError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::{NamedTempFile, TempDir};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntime {
    pub path: String,
    pub home: String,
    pub version: Option<String>,
    pub major_version: Option<u16>,
    pub architecture: Option<String>,
    pub vendor: Option<String>,
    pub source: String,
    pub managed: bool,
}

#[derive(Debug)]
pub enum JavaError {
    Data(DataError),
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Http {
        action: &'static str,
        source: reqwest::Error,
    },
    InvalidExecutable(PathBuf),
    UnsupportedMajor(u16),
    RuntimeUnavailable(u16),
    ChecksumMismatch,
    Archive(zip::result::ZipError),
    UnsafeArchiveEntry,
}

impl fmt::Display for JavaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => error.fmt(formatter),
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "无法{action} {}：{source}", path.display()),
            Self::Http { action, source } => write!(formatter, "{action}失败：{source}"),
            Self::InvalidExecutable(path) => {
                write!(formatter, "{} 不是可用的 Java 运行环境", path.display())
            }
            Self::UnsupportedMajor(major) => write!(
                formatter,
                "暂不支持由 NaCL 管理 Java {major}，当前支持 Java 8–25"
            ),
            Self::RuntimeUnavailable(major) => {
                write!(
                    formatter,
                    "Temurin 暂未提供适用于本机的 Java {major} 运行环境"
                )
            }
            Self::ChecksumMismatch => write!(formatter, "Java 下载校验失败，文件可能不完整"),
            Self::Archive(error) => write!(formatter, "无法读取 Java 压缩包：{error}"),
            Self::UnsafeArchiveEntry => write!(formatter, "Java 压缩包包含不安全的文件路径"),
        }
    }
}

impl std::error::Error for JavaError {}

impl From<DataError> for JavaError {
    fn from(error: DataError) -> Self {
        Self::Data(error)
    }
}

impl From<zip::result::ZipError> for JavaError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Archive(error)
    }
}

#[derive(Debug)]
struct JavaCandidate {
    executable: PathBuf,
    source: String,
    managed: bool,
}

#[derive(Debug, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Debug, Deserialize)]
struct AdoptiumPackage {
    checksum: String,
    link: String,
}

pub fn detect_java_runtimes() -> Vec<JavaRuntime> {
    let mut candidates = Vec::new();

    if let Some(java_home) = env::var_os("JAVA_HOME") {
        candidates.push(JavaCandidate {
            executable: PathBuf::from(java_home).join("bin").join("java.exe"),
            source: "JAVA_HOME".to_string(),
            managed: false,
        });
    }

    if let Some(path_value) = env::var_os("PATH") {
        for directory in env::split_paths(&path_value) {
            candidates.push(JavaCandidate {
                executable: directory.join("java.exe"),
                source: "PATH".to_string(),
                managed: false,
            });
        }
    }

    for (root, source, max_depth, managed) in known_installation_roots() {
        candidates.extend(scan_root(&root, source, max_depth, managed));
    }

    collect_runtimes(candidates)
}

pub fn inspect_java_runtime(path: &Path) -> Result<JavaRuntime, JavaError> {
    inspect_candidate(JavaCandidate {
        executable: path.to_path_buf(),
        source: "手动选择".to_string(),
        managed: is_managed_path(path),
    })
    .ok_or_else(|| JavaError::InvalidExecutable(path.to_path_buf()))
}

pub fn install_managed_runtime(major: u16) -> Result<JavaRuntime, JavaError> {
    if !(8..=25).contains(&major) {
        return Err(JavaError::UnsupportedMajor(major));
    }

    if let Some(runtime) = detect_java_runtimes()
        .into_iter()
        .find(|runtime| runtime.managed && runtime.major_version == Some(major))
    {
        return Ok(runtime);
    }

    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent("NaCL/0.1")
        .build()
        .map_err(|source| JavaError::Http {
            action: "创建 Java 下载连接",
            source,
        })?;

    let architecture = match env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        "x86" => "x32",
        other => other,
    };
    let assets_url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture={architecture}&heap_size=normal&image_type=jre&os=windows&page_size=1&project=jdk&sort_order=DESC&vendor=eclipse"
    );
    let assets = client
        .get(assets_url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|source| JavaError::Http {
            action: "读取 Temurin 运行环境信息",
            source,
        })?
        .json::<Vec<AdoptiumAsset>>()
        .map_err(|source| JavaError::Http {
            action: "解析 Temurin 运行环境信息",
            source,
        })?;
    let package = assets
        .into_iter()
        .next()
        .ok_or(JavaError::RuntimeUnavailable(major))?
        .binary
        .package;

    let mut response = client
        .get(&package.link)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|source| JavaError::Http {
            action: "下载 Temurin Java",
            source,
        })?;
    let mut archive_file =
        NamedTempFile::new_in(&paths.downloads_dir).map_err(|source| JavaError::Io {
            action: "创建 Java 临时下载文件",
            path: paths.downloads_dir.clone(),
            source,
        })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = response.read(&mut buffer).map_err(|source| JavaError::Io {
            action: "读取 Java 下载内容",
            path: archive_file.path().to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        archive_file
            .as_file_mut()
            .write_all(&buffer[..read])
            .map_err(|source| JavaError::Io {
                action: "写入 Java 临时下载文件",
                path: archive_file.path().to_path_buf(),
                source,
            })?;
    }
    let actual_checksum = format!("{:x}", hasher.finalize());
    if !actual_checksum.eq_ignore_ascii_case(&package.checksum) {
        return Err(JavaError::ChecksumMismatch);
    }

    let staging = TempDir::new_in(&paths.runtimes_dir).map_err(|source| JavaError::Io {
        action: "创建 Java 解压目录",
        path: paths.runtimes_dir.clone(),
        source,
    })?;
    let archive_reader = archive_file.reopen().map_err(|source| JavaError::Io {
        action: "打开 Java 压缩包",
        path: archive_file.path().to_path_buf(),
        source,
    })?;
    let mut archive = zip::ZipArchive::new(archive_reader)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let relative = entry.enclosed_name().ok_or(JavaError::UnsafeArchiveEntry)?;
        let destination = staging.path().join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&destination).map_err(|source| JavaError::Io {
                action: "创建 Java 目录",
                path: destination,
                source,
            })?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|source| JavaError::Io {
                action: "创建 Java 目录",
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let mut output = fs::File::create(&destination).map_err(|source| JavaError::Io {
            action: "解压 Java 文件",
            path: destination.clone(),
            source,
        })?;
        io::copy(&mut entry, &mut output).map_err(|source| JavaError::Io {
            action: "写入 Java 文件",
            path: destination,
            source,
        })?;
    }

    let extracted_java = find_java_executable(staging.path())
        .ok_or_else(|| JavaError::InvalidExecutable(staging.path().to_path_buf()))?;
    let extracted_home = java_home_from_executable(&extracted_java)
        .ok_or_else(|| JavaError::InvalidExecutable(extracted_java.clone()))?;
    let mut destination = paths.runtimes_dir.join(format!("temurin-{major}"));
    if destination.exists() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        destination = paths.runtimes_dir.join(format!("temurin-{major}-{suffix}"));
    }
    fs::rename(&extracted_home, &destination).map_err(|source| JavaError::Io {
        action: "安装 Java 运行环境",
        path: destination.clone(),
        source,
    })?;

    inspect_candidate(JavaCandidate {
        executable: destination.join("bin").join("java.exe"),
        source: "NaCL 托管 · Eclipse Temurin".to_string(),
        managed: true,
    })
    .ok_or_else(|| JavaError::InvalidExecutable(destination.join("bin").join("java.exe")))
}

fn known_installation_roots() -> Vec<(PathBuf, &'static str, usize, bool)> {
    let mut roots = Vec::new();

    if let Ok(paths) = AppPaths::resolve() {
        roots.push((paths.runtimes_dir, "NaCL 托管", 5, true));
    }

    if let Some(program_files) = env::var_os("ProgramFiles") {
        let program_files = PathBuf::from(program_files);
        roots.push((program_files.join("Java"), "Java installation", 3, false));
        roots.push((
            program_files.join("Eclipse Adoptium"),
            "Eclipse Adoptium",
            3,
            false,
        ));
        roots.push((
            program_files.join("Microsoft"),
            "Microsoft OpenJDK",
            3,
            false,
        ));
        roots.push((program_files.join("Zulu"), "Azul Zulu", 3, false));
    }

    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        roots.push((
            PathBuf::from(program_files_x86)
                .join("Minecraft Launcher")
                .join("runtime"),
            "Minecraft Launcher",
            7,
            false,
        ));
    }

    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        roots.push((
            PathBuf::from(local_app_data)
                .join("Programs")
                .join("Eclipse Adoptium"),
            "Eclipse Adoptium",
            3,
            false,
        ));
    }

    if let Some(user_profile) = env::var_os("USERPROFILE") {
        let user_profile = PathBuf::from(user_profile);
        roots.push((user_profile.join(".jdks"), "User JDK", 3, false));
        roots.push((
            user_profile
                .join("AppData")
                .join("Roaming")
                .join(".minecraft")
                .join("runtime"),
            "Minecraft runtime",
            7,
            false,
        ));
    }

    roots
}

fn scan_root(root: &Path, source: &str, max_depth: usize, managed: bool) -> Vec<JavaCandidate> {
    if !root.is_dir() {
        return Vec::new();
    }

    WalkDir::new(root)
        .follow_links(false)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("java.exe")
        })
        .filter(|entry| {
            entry
                .path()
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("bin"))
        })
        .map(|entry| JavaCandidate {
            executable: entry.into_path(),
            source: source.to_string(),
            managed,
        })
        .collect()
}

fn collect_runtimes(candidates: Vec<JavaCandidate>) -> Vec<JavaRuntime> {
    let mut seen = HashSet::new();
    let mut runtimes = Vec::new();

    for candidate in candidates {
        if !candidate.executable.is_file() {
            continue;
        }
        let executable = candidate
            .executable
            .canonicalize()
            .unwrap_or_else(|_| candidate.executable.clone());
        let identity = executable.to_string_lossy().to_lowercase();
        if !seen.insert(identity) {
            continue;
        }
        if let Some(runtime) = inspect_candidate(JavaCandidate {
            executable,
            source: candidate.source,
            managed: candidate.managed,
        }) {
            runtimes.push(runtime);
        }
    }

    runtimes.sort_by(|left, right| {
        right
            .major_version
            .cmp(&left.major_version)
            .then_with(|| left.path.cmp(&right.path))
    });
    runtimes
}

fn inspect_candidate(candidate: JavaCandidate) -> Option<JavaRuntime> {
    if !candidate.executable.is_file() {
        return None;
    }
    let executable = candidate
        .executable
        .canonicalize()
        .unwrap_or(candidate.executable);
    let home = java_home_from_executable(&executable)?;
    let process = read_runtime_properties(&executable)?;
    let release = read_release_metadata(&home);
    let version = process.version.or_else(|| {
        release
            .as_deref()
            .and_then(|contents| release_value(contents, "JAVA_VERSION"))
    });
    let architecture = process.architecture.or_else(|| {
        release
            .as_deref()
            .and_then(|contents| release_value(contents, "OS_ARCH"))
    });
    let vendor = process.vendor.or_else(|| {
        release
            .as_deref()
            .and_then(|contents| release_value(contents, "IMPLEMENTOR"))
    });

    Some(JavaRuntime {
        path: display_path(&executable),
        home: display_path(&home),
        major_version: version.as_deref().and_then(java_major_version),
        version,
        architecture,
        vendor,
        source: candidate.source,
        managed: candidate.managed || is_managed_path(&executable),
    })
}

#[derive(Default)]
struct RuntimeProperties {
    version: Option<String>,
    architecture: Option<String>,
    vendor: Option<String>,
}

fn read_runtime_properties(executable: &Path) -> Option<RuntimeProperties> {
    let mut command = Command::new(executable);
    command.args(["-XshowSettings:properties", "-version"]);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut properties = RuntimeProperties {
        version: property_value(&combined, "java.version"),
        architecture: property_value(&combined, "os.arch"),
        vendor: property_value(&combined, "java.vendor"),
    };
    if properties.version.is_none() {
        properties.version = quoted_version(&combined);
    }
    Some(properties)
}

fn property_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (candidate_key, value) = line.trim().split_once('=')?;
        (candidate_key.trim() == key).then(|| value.trim().to_string())
    })
}

fn quoted_version(contents: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (_, rest) = line.split_once("version")?;
        let start = rest.find('"')? + 1;
        let tail = &rest[start..];
        let end = tail.find('"')?;
        Some(tail[..end].to_string())
    })
}

fn java_major_version(version: &str) -> Option<u16> {
    let mut parts = version
        .trim_start_matches(|character: char| !character.is_ascii_digit())
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty());
    let first = parts.next()?.parse::<u16>().ok()?;
    if first == 1 {
        parts.next()?.parse::<u16>().ok()
    } else {
        Some(first)
    }
}

fn find_java_executable(root: &Path) -> Option<PathBuf> {
    WalkDir::new(root)
        .follow_links(false)
        .max_depth(5)
        .into_iter()
        .filter_map(Result::ok)
        .find(|entry| {
            entry.file_type().is_file()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("java.exe")
                && entry
                    .path()
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("bin"))
        })
        .map(walkdir::DirEntry::into_path)
}

fn is_managed_path(path: &Path) -> bool {
    AppPaths::resolve()
        .ok()
        .and_then(|paths| {
            path.canonicalize()
                .ok()
                .zip(paths.runtimes_dir.canonicalize().ok())
        })
        .is_some_and(|(candidate, runtimes)| candidate.starts_with(runtimes))
}

fn java_home_from_executable(executable: &Path) -> Option<PathBuf> {
    executable.parent()?.parent().map(Path::to_path_buf)
}

fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc_path) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{unc_path}");
    }

    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}

fn read_release_metadata(home: &Path) -> Option<String> {
    fs::read_to_string(home.join("release")).ok()
}

fn release_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (candidate_key, value) = line.split_once('=')?;
        if candidate_key.trim() != key {
            return None;
        }

        Some(value.trim().trim_matches('"').to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        display_path, java_home_from_executable, java_major_version, property_value, release_value,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn parses_release_metadata() {
        let release = "JAVA_VERSION=\"21.0.7\"\nOS_ARCH=\"amd64\"\n";
        assert_eq!(
            release_value(release, "JAVA_VERSION"),
            Some("21.0.7".to_string())
        );
        assert_eq!(release_value(release, "OS_ARCH"), Some("amd64".to_string()));
    }

    #[test]
    fn parses_runtime_properties() {
        let output =
            "Property settings:\n    java.vendor = Eclipse Adoptium\n    java.version = 21.0.7\n";
        assert_eq!(
            property_value(output, "java.vendor"),
            Some("Eclipse Adoptium".to_string())
        );
        assert_eq!(
            property_value(output, "java.version"),
            Some("21.0.7".to_string())
        );
    }

    #[test]
    fn parses_legacy_and_modern_major_versions() {
        assert_eq!(java_major_version("1.8.0_452"), Some(8));
        assert_eq!(java_major_version("17.0.15"), Some(17));
        assert_eq!(java_major_version("21"), Some(21));
    }

    #[test]
    fn derives_java_home_from_executable() {
        let executable = Path::new(r"C:\Java\jdk-21\bin\java.exe");
        assert_eq!(
            java_home_from_executable(executable),
            Some(PathBuf::from(r"C:\Java\jdk-21"))
        );
    }

    #[test]
    fn removes_windows_extended_path_prefix_for_display() {
        assert_eq!(
            display_path(Path::new(r"\\?\C:\Java\jdk-21\bin\java.exe")),
            r"C:\Java\jdk-21\bin\java.exe"
        );
    }
}
