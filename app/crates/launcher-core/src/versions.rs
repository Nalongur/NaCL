use crate::data::{AppPaths, DataError};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Deserialize)]
struct MojangManifest {
    latest: LatestVersions,
    versions: Vec<MinecraftVersion>,
}

#[derive(Debug, Clone, Deserialize)]
struct LatestVersions {
    release: String,
    snapshot: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
    pub release_time: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub compliance_level: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionCatalog {
    pub latest_release: String,
    pub latest_snapshot: String,
    pub versions: Vec<MinecraftVersion>,
    pub source: CatalogSource,
    pub refreshed_epoch_ms: u128,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CatalogSource {
    Network,
    Cache,
}

#[derive(Debug)]
pub enum VersionError {
    Data(DataError),
    Network(reqwest::Error),
    HttpStatus(reqwest::StatusCode),
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    InvalidManifest {
        path: Option<PathBuf>,
        source: serde_json::Error,
    },
}

impl fmt::Display for VersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => error.fmt(formatter),
            Self::Network(error) => write!(formatter, "连接 Mojang 版本服务失败：{error}"),
            Self::HttpStatus(status) => write!(formatter, "Mojang 版本服务返回 HTTP {status}"),
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "无法{action} {}：{source}", path.display()),
            Self::InvalidManifest { path, source } => {
                if let Some(path) = path {
                    write!(formatter, "版本清单 {} 无效：{source}", path.display())
                } else {
                    write!(formatter, "Mojang 返回的版本清单无效：{source}")
                }
            }
        }
    }
}

impl std::error::Error for VersionError {}

impl From<DataError> for VersionError {
    fn from(error: DataError) -> Self {
        Self::Data(error)
    }
}

pub fn load_version_catalog(force_refresh: bool) -> Result<VersionCatalog, VersionError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    let cache_path = paths.manifests_dir.join("version_manifest_v2.json");

    if !force_refresh && cache_is_fresh(&cache_path, Duration::from_secs(30 * 60)) {
        return read_catalog(&cache_path, CatalogSource::Cache);
    }

    match fetch_manifest() {
        Ok(contents) => {
            let parsed = parse_manifest(&contents, None, CatalogSource::Network)?;
            fs::write(&cache_path, contents)
                .map_err(|source| io_error("写入版本清单缓存", &cache_path, source))?;
            Ok(parsed)
        }
        Err(network_error) if cache_path.is_file() => {
            read_catalog(&cache_path, CatalogSource::Cache).map_err(|_| network_error)
        }
        Err(error) => Err(error),
    }
}

fn fetch_manifest() -> Result<String, VersionError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("NaCL/0.1")
        .build()
        .map_err(VersionError::Network)?;
    let response = client
        .get(VERSION_MANIFEST_URL)
        .send()
        .map_err(VersionError::Network)?;
    if !response.status().is_success() {
        return Err(VersionError::HttpStatus(response.status()));
    }
    response.text().map_err(VersionError::Network)
}

fn read_catalog(path: &Path, source: CatalogSource) -> Result<VersionCatalog, VersionError> {
    let contents =
        fs::read_to_string(path).map_err(|error| io_error("读取版本清单缓存", path, error))?;
    parse_manifest(&contents, Some(path.to_path_buf()), source)
}

fn parse_manifest(
    contents: &str,
    path: Option<PathBuf>,
    source: CatalogSource,
) -> Result<VersionCatalog, VersionError> {
    let manifest: MojangManifest = serde_json::from_str(contents)
        .map_err(|source| VersionError::InvalidManifest { path, source })?;
    Ok(VersionCatalog {
        latest_release: manifest.latest.release,
        latest_snapshot: manifest.latest.snapshot,
        versions: manifest.versions,
        source,
        refreshed_epoch_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis()),
    })
}

fn cache_is_fresh(path: &Path, maximum_age: Duration) -> bool {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age <= maximum_age)
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> VersionError {
    VersionError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_manifest, CatalogSource};

    #[test]
    fn parses_official_manifest_shape() {
        let catalog = parse_manifest(
            r#"{
              "latest": {"release": "1.21.1", "snapshot": "24w33a"},
              "versions": [{
                "id": "1.21.1",
                "type": "release",
                "url": "https://example.invalid/1.21.1.json",
                "releaseTime": "2024-08-08T12:00:00+00:00",
                "sha1": "abc",
                "complianceLevel": 1
              }]
            }"#,
            None,
            CatalogSource::Network,
        )
        .expect("manifest should parse");
        assert_eq!(catalog.latest_release, "1.21.1");
        assert_eq!(catalog.versions[0].version_type, "release");
    }
}
