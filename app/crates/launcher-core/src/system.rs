use crate::data::{AppPaths, DataError};
use crate::instance;
use crate::java;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DirectoryTarget {
    Data,
    Logs,
    Cache,
    Downloads,
    Instance,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    pub app_version: &'static str,
    pub operating_system: &'static str,
    pub architecture: &'static str,
    pub java_runtime_count: usize,
    pub instance_count: usize,
    pub roaming_data_path: PathBuf,
    pub local_data_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageReport {
    pub configuration_bytes: u64,
    pub instance_bytes: u64,
    pub cache_bytes: u64,
    pub runtime_bytes: u64,
    pub log_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryReport {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub recommended_mb: u32,
    pub maximum_assignable_mb: u32,
}

#[derive(Debug)]
pub enum SystemError {
    Data(DataError),
    Instance(instance::InstanceError),
    MissingInstanceId,
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    QueryMemory(io::Error),
    UnsupportedPlatform,
}

impl fmt::Display for SystemError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => error.fmt(formatter),
            Self::Instance(error) => error.fmt(formatter),
            Self::MissingInstanceId => write!(formatter, "打开实例目录时缺少实例标识"),
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "无法{action} {}：{source}", path.display()),
            Self::QueryMemory(source) => write!(formatter, "无法读取系统内存：{source}"),
            Self::UnsupportedPlatform => write!(formatter, "当前平台不支持打开目录"),
        }
    }
}

impl std::error::Error for SystemError {}

impl From<DataError> for SystemError {
    fn from(error: DataError) -> Self {
        Self::Data(error)
    }
}

impl From<instance::InstanceError> for SystemError {
    fn from(error: instance::InstanceError) -> Self {
        Self::Instance(error)
    }
}

pub fn diagnostics() -> Result<DiagnosticReport, SystemError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    Ok(DiagnosticReport {
        app_version: env!("CARGO_PKG_VERSION"),
        operating_system: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        java_runtime_count: java::detect_java_runtimes().len(),
        instance_count: instance::list_instances_in(&paths)?.len(),
        roaming_data_path: paths.roaming_root,
        local_data_path: paths.local_root,
    })
}

pub fn storage_report() -> Result<StorageReport, SystemError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    Ok(StorageReport {
        configuration_bytes: directory_size(&paths.config_dir),
        instance_bytes: directory_size(&paths.instances_dir),
        cache_bytes: directory_size(&paths.cache_dir),
        runtime_bytes: directory_size(&paths.runtimes_dir),
        log_bytes: directory_size(&paths.logs_dir),
    })
}

#[cfg(target_os = "windows")]
pub fn memory_report() -> Result<MemoryReport, SystemError> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status: MEMORYSTATUSEX = unsafe { zeroed() };
    status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return Err(SystemError::QueryMemory(io::Error::last_os_error()));
    }

    let total_mb = status.ullTotalPhys / (1024 * 1024);
    let maximum_assignable_mb =
        ((total_mb.saturating_mul(3) / 4) / 512 * 512).clamp(1024, 32768) as u32;
    let recommended_mb = match total_mb {
        0..=8192 => 2048,
        8193..=16384 => 4096,
        16385..=32768 => 6144,
        _ => 8192,
    }
    .min(maximum_assignable_mb);

    Ok(MemoryReport {
        total_bytes: status.ullTotalPhys,
        available_bytes: status.ullAvailPhys,
        recommended_mb,
        maximum_assignable_mb,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn memory_report() -> Result<MemoryReport, SystemError> {
    Err(SystemError::UnsupportedPlatform)
}

pub fn clean_temporary_downloads() -> Result<u64, SystemError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    let mut removed_bytes = 0_u64;
    for entry in walkdir::WalkDir::new(&paths.downloads_dir)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension != "part" && extension != "tmp" {
            continue;
        }
        removed_bytes = removed_bytes.saturating_add(entry.metadata().map_or(0, |item| item.len()));
        std::fs::remove_file(entry.path()).map_err(|source| SystemError::Io {
            action: "清理临时下载",
            path: entry.path().to_path_buf(),
            source,
        })?;
    }
    Ok(removed_bytes)
}

pub fn open_directory(
    target: DirectoryTarget,
    instance_id: Option<String>,
) -> Result<PathBuf, SystemError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    let path = match target {
        DirectoryTarget::Data => paths.roaming_root,
        DirectoryTarget::Logs => paths.logs_dir,
        DirectoryTarget::Cache => paths.cache_dir,
        DirectoryTarget::Downloads => paths.downloads_dir,
        DirectoryTarget::Instance => {
            let id = instance_id.ok_or(SystemError::MissingInstanceId)?;
            let exists = instance::list_instances_in(&paths)?
                .iter()
                .any(|candidate| candidate.id == id);
            if !exists {
                return Err(instance::InstanceError::InstanceNotFound(id).into());
            }
            paths.instances_dir.join(id).join("game")
        }
    };

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer.exe")
            .arg(&path)
            .spawn()
            .map_err(|source| SystemError::Io {
                action: "打开目录",
                path: path.clone(),
                source,
            })?;
        Ok(path)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Err(SystemError::UnsupportedPlatform)
    }
}

fn directory_size(path: &std::path::Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .fold(0_u64, |total, metadata| {
            total.saturating_add(metadata.len())
        })
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::memory_report;

    #[test]
    fn reports_physical_memory_with_safe_limits() {
        let report = memory_report().expect("Windows should report physical memory");
        assert!(report.total_bytes > 0);
        assert!(report.available_bytes <= report.total_bytes);
        assert!(report.recommended_mb >= 1024);
        assert!(report.maximum_assignable_mb >= report.recommended_mb);
        assert_eq!(report.maximum_assignable_mb % 512, 0);
    }
}
