use crate::data::{AppPaths, DataError};
use serde::Serialize;
use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    pub name: String,
    pub size_bytes: u64,
    pub modified_epoch_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogContent {
    pub name: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug)]
pub enum LogError {
    Data(DataError),
    InvalidName,
    NotFound(String),
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for LogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => error.fmt(formatter),
            Self::InvalidName => write!(formatter, "日志文件名无效"),
            Self::NotFound(name) => write!(formatter, "找不到日志 {name}"),
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "无法{action} {}：{source}", path.display()),
        }
    }
}

impl std::error::Error for LogError {}

impl From<DataError> for LogError {
    fn from(error: DataError) -> Self {
        Self::Data(error)
    }
}

pub type LogResult<T> = Result<T, LogError>;

pub fn list_logs() -> LogResult<Vec<LogFile>> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    list_logs_in(&paths)
}

pub fn read_log(name: String) -> LogResult<LogContent> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    read_log_in(&paths, &name)
}

pub fn delete_log(name: String) -> LogResult<()> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    delete_log_in(&paths, &name)
}

pub fn clear_old_logs(retention_days: u16) -> LogResult<usize> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    clear_old_logs_in(&paths, retention_days)
}

pub fn append_launcher_log(level: &str, message: &str) -> LogResult<()> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    append_launcher_log_in(&paths, level, message)
}

pub fn append_launcher_log_in(paths: &AppPaths, level: &str, message: &str) -> LogResult<()> {
    let path = paths.logs_dir.join("launcher.log");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|source| io_error("打开启动器日志", &path, source))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let safe_level: String = level
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(12)
        .collect();
    let safe_message = message.replace(['\r', '\n'], " ");
    writeln!(
        file,
        "[{timestamp}] [{}] {safe_message}",
        safe_level.to_uppercase()
    )
    .map_err(|source| io_error("写入启动器日志", &path, source))
}

pub fn list_logs_in(paths: &AppPaths) -> LogResult<Vec<LogFile>> {
    let mut logs = Vec::new();
    let entries = fs::read_dir(&paths.logs_dir)
        .map_err(|source| io_error("读取日志目录", &paths.logs_dir, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| io_error("读取日志目录项", &paths.logs_dir, source))?;
        let file_type = entry
            .file_type()
            .map_err(|source| io_error("读取日志类型", &entry.path(), source))?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_supported_log_name(&name) {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|source| io_error("读取日志信息", &entry.path(), source))?;
        let modified_epoch_ms = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_millis());
        logs.push(LogFile {
            name,
            size_bytes: metadata.len(),
            modified_epoch_ms,
        });
    }
    logs.sort_by(|left, right| {
        right
            .modified_epoch_ms
            .cmp(&left.modified_epoch_ms)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(logs)
}

pub fn read_log_in(paths: &AppPaths, name: &str) -> LogResult<LogContent> {
    if !is_supported_log_name(name)
        || Path::new(name)
            .file_name()
            .is_none_or(|file_name| file_name != name)
    {
        return Err(LogError::InvalidName);
    }
    let path = paths.logs_dir.join(name);
    if !path.is_file() {
        return Err(LogError::NotFound(name.to_owned()));
    }
    let bytes = fs::read(&path).map_err(|source| io_error("读取日志", &path, source))?;
    let truncated = bytes.len() > MAX_LOG_BYTES;
    let visible = if truncated {
        &bytes[bytes.len() - MAX_LOG_BYTES..]
    } else {
        &bytes
    };
    Ok(LogContent {
        name: name.to_owned(),
        content: String::from_utf8_lossy(visible).into_owned(),
        truncated,
    })
}

pub fn delete_log_in(paths: &AppPaths, name: &str) -> LogResult<()> {
    validate_log_name(name)?;
    let path = paths.logs_dir.join(name);
    if !path.is_file() {
        return Err(LogError::NotFound(name.to_owned()));
    }
    fs::remove_file(&path).map_err(|source| io_error("删除日志", &path, source))
}

pub fn clear_old_logs_in(paths: &AppPaths, retention_days: u16) -> LogResult<usize> {
    if !(1..=365).contains(&retention_days) {
        return Err(LogError::InvalidName);
    }
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(
            u64::from(retention_days) * 24 * 60 * 60,
        ))
        .unwrap_or(UNIX_EPOCH);
    let mut deleted = 0;
    for log in list_logs_in(paths)? {
        let path = paths.logs_dir.join(&log.name);
        let modified = path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map_err(|source| io_error("读取日志信息", &path, source))?;
        if modified < cutoff {
            fs::remove_file(&path).map_err(|source| io_error("删除旧日志", &path, source))?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

fn validate_log_name(name: &str) -> LogResult<()> {
    if !is_supported_log_name(name)
        || Path::new(name)
            .file_name()
            .is_none_or(|file_name| file_name != name)
    {
        return Err(LogError::InvalidName);
    }
    Ok(())
}

fn is_supported_log_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    (lower.ends_with(".log") || lower.ends_with(".txt"))
        && !name.is_empty()
        && !name.chars().any(char::is_control)
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> LogError {
    LogError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_launcher_log_in, clear_old_logs_in, delete_log_in, list_logs_in, read_log_in,
    };
    use crate::data::AppPaths;
    use std::fs;

    #[test]
    fn lists_and_reads_supported_logs_only() {
        let root = tempfile::tempdir().expect("temporary directory should be created");
        let paths = AppPaths::from_roots(root.path().join("roaming"), root.path().join("local"));
        paths.initialize().expect("paths should initialize");
        fs::write(paths.logs_dir.join("launcher.log"), "NaCL ready")
            .expect("log should be written");
        fs::write(paths.logs_dir.join("ignore.bin"), [0_u8, 1_u8])
            .expect("binary fixture should be written");

        let logs = list_logs_in(&paths).expect("logs should list");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].name, "launcher.log");
        let content = read_log_in(&paths, "launcher.log").expect("log should read");
        assert_eq!(content.content, "NaCL ready");
        assert!(!content.truncated);
        delete_log_in(&paths, "launcher.log").expect("log should delete");
        assert!(list_logs_in(&paths).expect("logs should list").is_empty());
    }

    #[test]
    fn rejects_log_path_traversal() {
        let root = tempfile::tempdir().expect("temporary directory should be created");
        let paths = AppPaths::from_roots(root.path().join("roaming"), root.path().join("local"));
        paths.initialize().expect("paths should initialize");
        assert!(read_log_in(&paths, "../launcher.log").is_err());
    }

    #[test]
    fn keeps_recent_logs_during_retention_cleanup() {
        let root = tempfile::tempdir().expect("temporary directory should be created");
        let paths = AppPaths::from_roots(root.path().join("roaming"), root.path().join("local"));
        paths.initialize().expect("paths should initialize");
        fs::write(paths.logs_dir.join("recent.log"), "recent").expect("log should be written");
        assert_eq!(
            clear_old_logs_in(&paths, 14).expect("cleanup should succeed"),
            0
        );
    }

    #[test]
    fn appends_sanitized_launcher_events() {
        let root = tempfile::tempdir().expect("temporary directory should be created");
        let paths = AppPaths::from_roots(root.path().join("roaming"), root.path().join("local"));
        paths.initialize().expect("paths should initialize");
        append_launcher_log_in(&paths, "info", "launch requested\ninstance=test")
            .expect("launcher event should append");
        let content = read_log_in(&paths, "launcher.log").expect("launcher log should read");
        assert!(content
            .content
            .contains("[INFO] launch requested instance=test"));
    }
}
