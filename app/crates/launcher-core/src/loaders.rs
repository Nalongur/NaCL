use crate::instance::{GameLoader, LoaderConfig};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

const USER_AGENT: &str = "Nalongur/NaCL/0.4.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
    pub recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderCatalog {
    pub game_version: String,
    pub loader: GameLoader,
    pub versions: Vec<LoaderVersion>,
}

#[derive(Debug, Clone)]
pub struct LoaderArtifact {
    pub label: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
    pub relative_path: String,
}

#[derive(Debug, Clone)]
pub struct LoaderInstallPlan {
    pub config: LoaderConfig,
    pub effective_profile: Value,
    pub artifacts: Vec<LoaderArtifact>,
}

#[derive(Debug, Clone)]
pub struct LoaderInstallerPlan {
    pub installer_url: String,
    pub installer_sha1: String,
    pub installer_file_name: String,
}

#[derive(Debug)]
pub enum LoaderError {
    UnsupportedLoader,
    UnsupportedGameVersion(String),
    VersionNotFound(String),
    Network(String),
    InvalidResponse(String),
    InvalidProfile(String),
    InvalidMavenCoordinate(String),
}

impl fmt::Display for LoaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedLoader => write!(formatter, "该加载器暂不支持此操作"),
            Self::UnsupportedGameVersion(version) => {
                write!(formatter, "加载器不支持 Minecraft {version}")
            }
            Self::VersionNotFound(version) => write!(formatter, "找不到加载器版本 {version}"),
            Self::Network(message) => write!(formatter, "加载器元数据请求失败：{message}"),
            Self::InvalidResponse(message) => write!(formatter, "加载器元数据无效：{message}"),
            Self::InvalidProfile(message) => write!(formatter, "加载器启动配置无效：{message}"),
            Self::InvalidMavenCoordinate(value) => write!(formatter, "无效的 Maven 坐标：{value}"),
        }
    }
}

impl std::error::Error for LoaderError {}

pub fn list_loader_versions(
    loader: GameLoader,
    game_version: &str,
) -> Result<LoaderCatalog, LoaderError> {
    if loader == GameLoader::Vanilla {
        return Ok(LoaderCatalog {
            game_version: game_version.to_owned(),
            loader,
            versions: Vec::new(),
        });
    }
    let client = client()?;
    let versions = match loader {
        GameLoader::Fabric => fabric_versions(&client, game_version)?,
        GameLoader::Quilt => quilt_versions(&client, game_version)?,
        GameLoader::Forge => forge_versions(&client, game_version)?,
        GameLoader::NeoForge => neoforge_versions(&client, game_version)?,
        GameLoader::Vanilla => Vec::new(),
    };
    Ok(LoaderCatalog {
        game_version: game_version.to_owned(),
        loader,
        versions,
    })
}

pub fn plan_profile_loader_install(
    loader: GameLoader,
    game_version: &str,
    loader_version: &str,
    base_profile: &Value,
) -> Result<LoaderInstallPlan, LoaderError> {
    let endpoint = match loader {
        GameLoader::Fabric => format!(
            "https://meta.fabricmc.net/v2/versions/loader/{game_version}/{loader_version}/profile/json"
        ),
        GameLoader::Quilt => format!(
            "https://meta.quiltmc.org/v3/versions/loader/{game_version}/{loader_version}/profile/json"
        ),
        _ => return Err(LoaderError::UnsupportedLoader),
    };
    let profile: Value = get_json(&client()?, &endpoint)?;
    let profile_id = profile
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| LoaderError::InvalidProfile("缺少 profile id".to_string()))?
        .to_owned();
    if profile.get("inheritsFrom").and_then(Value::as_str) != Some(game_version) {
        return Err(LoaderError::InvalidProfile(
            "inheritsFrom 与所选 Minecraft 版本不一致".to_string(),
        ));
    }

    let (loader_libraries, artifacts) = normalize_loader_libraries(&profile)?;
    let effective_profile = merge_profiles(base_profile, &profile, loader_libraries)?;
    Ok(LoaderInstallPlan {
        config: LoaderConfig::modded(loader, loader_version.to_owned(), profile_id),
        effective_profile,
        artifacts,
    })
}

pub fn plan_installer_loader_install(
    loader: GameLoader,
    loader_version: &str,
) -> Result<LoaderInstallerPlan, LoaderError> {
    if !valid_version_value(loader_version) {
        return Err(LoaderError::VersionNotFound(loader_version.to_owned()));
    }
    let (installer_url, installer_file_name) = match loader {
        GameLoader::Forge => (
            format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{loader_version}/forge-{loader_version}-installer.jar"
            ),
            format!("forge-{loader_version}-installer.jar"),
        ),
        GameLoader::NeoForge => (
            format!(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
            ),
            format!("neoforge-{loader_version}-installer.jar"),
        ),
        _ => return Err(LoaderError::UnsupportedLoader),
    };
    let sha1 = get_text(&client()?, &format!("{installer_url}.sha1"))?
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if sha1.len() != 40 || !sha1.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(LoaderError::InvalidResponse(
            "安装器 SHA-1 响应无效".to_string(),
        ));
    }
    Ok(LoaderInstallerPlan {
        installer_url,
        installer_sha1: sha1,
        installer_file_name,
    })
}

pub fn merge_installed_loader_profile(
    loader: GameLoader,
    game_version: &str,
    loader_version: &str,
    base_profile: &Value,
    installed_profile: &Value,
) -> Result<LoaderInstallPlan, LoaderError> {
    let profile_id = installed_profile
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| LoaderError::InvalidProfile("安装器 profile 缺少 id".to_string()))?
        .to_owned();
    if installed_profile
        .get("inheritsFrom")
        .and_then(Value::as_str)
        != Some(game_version)
    {
        return Err(LoaderError::InvalidProfile(
            "安装器 profile 的 inheritsFrom 不匹配".to_string(),
        ));
    }
    let libraries = installed_profile
        .get("libraries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let effective_profile = merge_profiles(base_profile, installed_profile, libraries)?;
    Ok(LoaderInstallPlan {
        config: LoaderConfig::modded(loader, loader_version.to_owned(), profile_id),
        effective_profile,
        artifacts: Vec::new(),
    })
}

fn client() -> Result<reqwest::blocking::Client, LoaderError> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| LoaderError::Network(error.to_string()))
}

fn get_text(client: &reqwest::blocking::Client, url: &str) -> Result<String, LoaderError> {
    let response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| LoaderError::Network(error.to_string()))?;
    response
        .text()
        .map_err(|error| LoaderError::Network(error.to_string()))
}

fn get_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<T, LoaderError> {
    let response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| LoaderError::Network(error.to_string()))?;
    response
        .json()
        .map_err(|error| LoaderError::InvalidResponse(error.to_string()))
}

#[derive(Deserialize)]
struct FabricEntry {
    loader: FabricLoader,
}

#[derive(Deserialize)]
struct FabricLoader {
    version: String,
    stable: bool,
}

fn fabric_versions(
    client: &reqwest::blocking::Client,
    game_version: &str,
) -> Result<Vec<LoaderVersion>, LoaderError> {
    let entries: Vec<FabricEntry> = get_json(
        client,
        &format!("https://meta.fabricmc.net/v2/versions/loader/{game_version}"),
    )?;
    Ok(entries
        .into_iter()
        .map(|entry| LoaderVersion {
            version: entry.loader.version,
            stable: entry.loader.stable,
            recommended: entry.loader.stable,
        })
        .collect())
}

#[derive(Deserialize)]
struct QuiltEntry {
    loader: QuiltLoader,
}

#[derive(Deserialize)]
struct QuiltLoader {
    version: String,
}

fn quilt_versions(
    client: &reqwest::blocking::Client,
    game_version: &str,
) -> Result<Vec<LoaderVersion>, LoaderError> {
    let entries: Vec<QuiltEntry> = get_json(
        client,
        &format!("https://meta.quiltmc.org/v3/versions/loader/{game_version}"),
    )?;
    Ok(entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| LoaderVersion {
            stable: !entry.loader.version.contains("beta")
                && !entry.loader.version.contains("alpha"),
            recommended: index == 0,
            version: entry.loader.version,
        })
        .collect())
}

fn forge_versions(
    client: &reqwest::blocking::Client,
    game_version: &str,
) -> Result<Vec<LoaderVersion>, LoaderError> {
    let xml = get_text(
        client,
        "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml",
    )?;
    let prefix = format!("{game_version}-");
    Ok(xml_versions(&xml)
        .into_iter()
        .filter(|version| version.starts_with(&prefix))
        .rev()
        .enumerate()
        .map(|(index, version)| LoaderVersion {
            stable: true,
            recommended: index == 0,
            version,
        })
        .collect())
}

fn neoforge_versions(
    client: &reqwest::blocking::Client,
    game_version: &str,
) -> Result<Vec<LoaderVersion>, LoaderError> {
    let xml = get_text(
        client,
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
    )?;
    let prefix = neoforge_prefix(game_version)
        .ok_or_else(|| LoaderError::UnsupportedGameVersion(game_version.to_owned()))?;
    Ok(xml_versions(&xml)
        .into_iter()
        .filter(|version| version.starts_with(&prefix))
        .rev()
        .enumerate()
        .map(|(index, version)| LoaderVersion {
            stable: !version.contains("beta"),
            recommended: index == 0,
            version,
        })
        .collect())
}

fn neoforge_prefix(game_version: &str) -> Option<String> {
    let parts = game_version.split('.').collect::<Vec<_>>();
    match parts.as_slice() {
        ["1", minor] => Some(format!("{minor}.0.")),
        ["1", minor, patch] => Some(format!("{minor}.{patch}.")),
        [year, drop] if year.len() == 2 => Some(format!("{year}.{drop}.")),
        _ => None,
    }
}

fn valid_version_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_+".contains(character))
}

fn xml_versions(xml: &str) -> Vec<String> {
    let mut versions = Vec::new();
    let mut remaining = xml;
    while let Some(start) = remaining.find("<version>") {
        remaining = &remaining[start + "<version>".len()..];
        let Some(end) = remaining.find("</version>") else {
            break;
        };
        let value = remaining[..end].trim();
        if !value.is_empty() {
            versions.push(value.to_owned());
        }
        remaining = &remaining[end + "</version>".len()..];
    }
    versions
}

fn normalize_loader_libraries(
    profile: &Value,
) -> Result<(Vec<Value>, Vec<LoaderArtifact>), LoaderError> {
    let libraries = profile
        .get("libraries")
        .and_then(Value::as_array)
        .ok_or_else(|| LoaderError::InvalidProfile("缺少 libraries".to_string()))?;
    let mut normalized = Vec::with_capacity(libraries.len());
    let mut artifacts = Vec::with_capacity(libraries.len());
    for library in libraries {
        let mut library = library
            .as_object()
            .cloned()
            .ok_or_else(|| LoaderError::InvalidProfile("library 不是对象".to_string()))?;
        let coordinate = library
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| LoaderError::InvalidProfile("library 缺少 name".to_string()))?;
        let coordinate = coordinate.to_owned();
        let repository = library
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("https://libraries.minecraft.net/");
        if !repository.starts_with("https://") {
            return Err(LoaderError::InvalidProfile(
                "library 仓库必须使用 HTTPS".to_string(),
            ));
        }
        let relative_path = maven_path(&coordinate)?;
        let url = format!("{}/{}", repository.trim_end_matches('/'), relative_path);
        let sha1 = library
            .get("sha1")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let size = library.get("size").and_then(Value::as_u64).unwrap_or(0);
        let artifact = serde_json::json!({
            "path": relative_path,
            "url": url,
            "sha1": sha1,
            "size": size,
        });
        library.insert(
            "downloads".to_string(),
            serde_json::json!({ "artifact": artifact }),
        );
        artifacts.push(LoaderArtifact {
            label: coordinate,
            url,
            sha1,
            size,
            relative_path,
        });
        normalized.push(Value::Object(library));
    }
    Ok((normalized, artifacts))
}

fn maven_path(coordinate: &str) -> Result<String, LoaderError> {
    let (coordinate, extension) = coordinate
        .split_once('@')
        .map_or((coordinate, "jar"), |(value, extension)| (value, extension));
    if extension.is_empty()
        || !extension
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(LoaderError::InvalidMavenCoordinate(coordinate.to_owned()));
    }
    let parts = coordinate.split(':').collect::<Vec<_>>();
    if !(3..=4).contains(&parts.len())
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || ".-_+".contains(character)
                })
        })
    {
        return Err(LoaderError::InvalidMavenCoordinate(coordinate.to_owned()));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts
        .get(3)
        .map_or(String::new(), |value| format!("-{value}"));
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}"
    ))
}

fn merge_profiles(
    base_profile: &Value,
    child_profile: &Value,
    child_libraries: Vec<Value>,
) -> Result<Value, LoaderError> {
    let mut merged = base_profile
        .as_object()
        .cloned()
        .ok_or_else(|| LoaderError::InvalidProfile("基础版本不是对象".to_string()))?;
    let child = child_profile
        .as_object()
        .ok_or_else(|| LoaderError::InvalidProfile("加载器版本不是对象".to_string()))?;
    for key in [
        "id",
        "inheritsFrom",
        "mainClass",
        "releaseTime",
        "time",
        "type",
    ] {
        if let Some(value) = child.get(key) {
            merged.insert(key.to_string(), value.clone());
        }
    }

    let mut libraries = merged
        .remove("libraries")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    libraries.extend(child_libraries);
    let mut seen = HashSet::new();
    libraries.reverse();
    libraries.retain(|library| {
        library
            .get("name")
            .and_then(Value::as_str)
            .is_none_or(|name| seen.insert(name.to_owned()))
    });
    libraries.reverse();
    merged.insert("libraries".to_string(), Value::Array(libraries));

    let base_arguments = merged.remove("arguments").unwrap_or_else(|| {
        Value::Object(Map::from_iter([
            ("game".to_string(), Value::Array(Vec::new())),
            ("jvm".to_string(), Value::Array(Vec::new())),
        ]))
    });
    let child_arguments = child.get("arguments").cloned().unwrap_or_else(|| {
        Value::Object(Map::from_iter([
            ("game".to_string(), Value::Array(Vec::new())),
            ("jvm".to_string(), Value::Array(Vec::new())),
        ]))
    });
    merged.insert(
        "arguments".to_string(),
        merge_arguments(&base_arguments, &child_arguments),
    );
    Ok(Value::Object(merged))
}

fn merge_arguments(base: &Value, child: &Value) -> Value {
    let mut result = Map::new();
    for key in ["game", "jvm"] {
        let mut values = base
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        values.extend(
            child
                .get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
        result.insert(key.to_string(), Value::Array(values));
    }
    Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::{
        maven_path, merge_profiles, neoforge_prefix, plan_installer_loader_install,
        plan_profile_loader_install, xml_versions,
    };
    use crate::instance::GameLoader;

    #[test]
    fn builds_safe_maven_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.14").unwrap(),
            "net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar"
        );
        assert_eq!(
            maven_path("group:artifact:1.0:client@zip").unwrap(),
            "group/artifact/1.0/artifact-1.0-client.zip"
        );
        assert!(maven_path("group:../bad:1.0").is_err());
    }

    #[test]
    fn extracts_maven_metadata_versions() {
        assert_eq!(
            xml_versions("<versions><version>1.0</version><version>2.0</version></versions>"),
            vec!["1.0", "2.0"]
        );
    }

    #[test]
    fn maps_neoforge_game_versions() {
        assert_eq!(neoforge_prefix("1.20.4").as_deref(), Some("20.4."));
        assert_eq!(neoforge_prefix("1.21").as_deref(), Some("21.0."));
        assert_eq!(neoforge_prefix("1.21.1").as_deref(), Some("21.1."));
        assert_eq!(neoforge_prefix("26.2").as_deref(), Some("26.2."));
    }

    #[test]
    fn merges_child_main_class_libraries_and_arguments() {
        let base = serde_json::json!({
            "id": "1.21.1",
            "mainClass": "vanilla.Main",
            "libraries": [{"name": "base:lib:1", "downloads": {}}],
            "arguments": {"game": ["--base"], "jvm": ["-cp", "${classpath}"]},
            "assetIndex": {"id": "1"}
        });
        let child = serde_json::json!({
            "id": "fabric-loader-test",
            "inheritsFrom": "1.21.1",
            "mainClass": "fabric.Main",
            "arguments": {"game": ["--child"], "jvm": ["-Dchild=true"]}
        });
        let merged = merge_profiles(
            &base,
            &child,
            vec![serde_json::json!({"name": "loader:lib:1", "downloads": {}})],
        )
        .unwrap();
        assert_eq!(merged["mainClass"], "fabric.Main");
        assert_eq!(merged["libraries"].as_array().unwrap().len(), 2);
        assert_eq!(merged["arguments"]["game"].as_array().unwrap().len(), 2);
    }

    #[test]
    #[ignore = "live official Fabric and Quilt metadata probe"]
    fn plans_live_profile_loaders() {
        let base = serde_json::json!({
            "id": "1.21.1",
            "libraries": [],
            "arguments": {"game": [], "jvm": []}
        });
        for (loader, version) in [
            (GameLoader::Fabric, "0.19.3"),
            (GameLoader::Quilt, "0.20.0-beta.9"),
        ] {
            let plan = plan_profile_loader_install(loader, "1.21.1", version, &base)
                .expect("official profile should produce an install plan");
            assert_eq!(plan.config.kind, loader);
            assert!(!plan.artifacts.is_empty());
            assert!(plan
                .artifacts
                .iter()
                .all(|artifact| artifact.url.starts_with("https://")));
        }
    }

    #[test]
    #[ignore = "live official Forge and NeoForge metadata probe"]
    fn plans_live_installer_loaders() {
        for (loader, version) in [
            (GameLoader::Forge, "1.21.1-52.1.16"),
            (GameLoader::NeoForge, "21.1.248"),
        ] {
            let plan = plan_installer_loader_install(loader, version)
                .expect("official installer and SHA-1 should be available");
            assert!(plan.installer_url.starts_with("https://"));
            assert_eq!(plan.installer_sha1.len(), 40);
        }
    }
}
