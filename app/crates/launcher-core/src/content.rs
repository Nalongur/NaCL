use crate::data::{load_or_create_download_settings, AppPaths, DataError, DownloadSettings};
use crate::downloader::{self, DownloadControl, DownloadOptions, DownloadRequest};
use crate::installer::{
    self, InstallControl, InstallError, InstallInstanceRequest, InstallProgress,
};
use crate::instance::{self, GameLoader, InstanceConfig, InstanceError};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha512};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, Cursor, Read, Seek};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;
use uuid::Uuid;

const CONTENT_SCHEMA_VERSION: u32 = 1;
const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const USER_AGENT: &str = "Nalongur/NaCL/0.4.0";
const FABRIC_NESTED_MAX_DEPTH: usize = 8;
const FABRIC_NESTED_MAX_ENTRIES: usize = 256;
const FABRIC_NESTED_MAX_JAR_SIZE: u64 = 64 * 1024 * 1024;
const FABRIC_NESTED_MAX_TOTAL_SIZE: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentKind {
    Mod,
    Resourcepack,
    Shader,
    Datapack,
    Modpack,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchContentRequest {
    pub query: String,
    pub kind: ContentKind,
    pub game_version: String,
    pub loader: Option<GameLoader>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_search_limit")]
    pub limit: u8,
}

fn default_search_limit() -> u8 {
    20
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentSearchPage {
    pub hits: Vec<ContentProject>,
    pub offset: u32,
    pub limit: u8,
    pub total_hits: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct ContentProject {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub date_modified: String,
    pub project_type: ContentKind,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct ContentVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    pub date_published: String,
    pub downloads: u64,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub dependencies: Vec<VersionDependency>,
    pub files: Vec<VersionFile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct VersionDependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub file_name: Option<String>,
    pub dependency_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionFile {
    pub hashes: HashMap<String, String>,
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallContentRequest {
    pub instance_id: String,
    pub version_id: String,
    pub kind: ContentKind,
    #[serde(default)]
    pub world_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentActionRequest {
    pub instance_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBatchActionRequest {
    pub instance_id: String,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportLocalContentRequest {
    pub instance_id: String,
    pub source_path: PathBuf,
    pub kind: ContentKind,
    #[serde(default)]
    pub world_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentUpdate {
    pub item_id: String,
    pub version_id: String,
    pub version_number: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallModpackRequest {
    pub name: String,
    pub version_id: Option<String>,
    pub local_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModpackIndex {
    format_version: u32,
    game: String,
    version_id: String,
    name: String,
    summary: Option<String>,
    files: Vec<ModpackFile>,
    dependencies: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModpackFile {
    path: String,
    hashes: HashMap<String, String>,
    env: Option<ModpackEnvironment>,
    downloads: Vec<String>,
    #[serde(default)]
    file_size: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct ModpackEnvironment {
    client: String,
    server: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedContent {
    pub id: String,
    pub kind: ContentKind,
    pub source: String,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub version_number: Option<String>,
    pub name: String,
    pub file_name: String,
    pub sha1: Option<String>,
    pub sha512: Option<String>,
    pub size: u64,
    pub enabled: bool,
    pub managed: bool,
    pub installed_epoch_ms: u128,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub mod_metadata: Option<ModMetadata>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub world_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldInfo {
    pub name: String,
    pub datapack_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModMetadata {
    pub format: String,
    pub mod_ids: Vec<String>,
    #[serde(default, skip)]
    pub provided_mod_ids: Vec<String>,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub dependencies: Vec<ModDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDependency {
    pub id: String,
    pub requirement: String,
    pub mandatory: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentManifest {
    schema_version: u32,
    items: Vec<ManagedContent>,
}

impl Default for ContentManifest {
    fn default() -> Self {
        Self {
            schema_version: CONTENT_SCHEMA_VERSION,
            items: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum ContentError {
    Data(DataError),
    Instance(InstanceError),
    Installer(InstallError),
    InstanceNotFound(String),
    ItemNotFound(String),
    Network(String),
    InvalidResponse(String),
    Incompatible(String),
    UnsafePath(String),
    ChecksumMismatch(String),
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    InvalidManifest {
        path: PathBuf,
        source: serde_json::Error,
    },
    Archive(String),
    Cancelled,
}

impl fmt::Display for ContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => error.fmt(formatter),
            Self::Instance(error) => error.fmt(formatter),
            Self::Installer(error) => error.fmt(formatter),
            Self::InstanceNotFound(id) => write!(formatter, "找不到实例 {id}"),
            Self::ItemNotFound(id) => write!(formatter, "找不到内容项目 {id}"),
            Self::Network(message) => write!(formatter, "社区内容请求失败：{message}"),
            Self::InvalidResponse(message) => write!(formatter, "社区内容响应无效：{message}"),
            Self::Incompatible(message) => write!(formatter, "内容不兼容：{message}"),
            Self::UnsafePath(path) => write!(formatter, "内容路径不安全：{path}"),
            Self::ChecksumMismatch(file) => write!(formatter, "内容文件校验失败：{file}"),
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "无法{action} {}：{source}", path.display()),
            Self::InvalidManifest { path, source } => {
                write!(formatter, "内容清单 {} 无效：{source}", path.display())
            }
            Self::Archive(message) => write!(formatter, "整合包无效：{message}"),
            Self::Cancelled => write!(formatter, "内容操作已取消"),
        }
    }
}

impl std::error::Error for ContentError {}

impl From<DataError> for ContentError {
    fn from(error: DataError) -> Self {
        Self::Data(error)
    }
}

impl From<InstanceError> for ContentError {
    fn from(error: InstanceError) -> Self {
        Self::Instance(error)
    }
}

impl From<InstallError> for ContentError {
    fn from(error: InstallError) -> Self {
        Self::Installer(error)
    }
}

pub fn search_content(request: SearchContentRequest) -> Result<ContentSearchPage, ContentError> {
    let project_type = if request.kind == ContentKind::Datapack {
        "mod"
    } else {
        kind_name(request.kind)
    };
    let mut facets = vec![vec![format!("project_type:{project_type}")]];
    if request.kind == ContentKind::Datapack {
        facets.push(vec!["categories:datapack".to_string()]);
    }
    if !request.game_version.is_empty() {
        facets.push(vec![format!("versions:{}", request.game_version)]);
    }
    if request.kind == ContentKind::Mod {
        if let Some(loader) = request
            .loader
            .filter(|loader| *loader != GameLoader::Vanilla)
        {
            facets.push(vec![format!("categories:{}", loader_name(loader))]);
        }
    }
    let limit = request.limit.clamp(1, 100);
    let response = client()?
        .get(format!("{MODRINTH_API}/search"))
        .query(&[
            ("query", request.query),
            (
                "facets",
                serde_json::to_string(&facets)
                    .map_err(|error| ContentError::InvalidResponse(error.to_string()))?,
            ),
            ("index", "relevance".to_string()),
            ("offset", request.offset.to_string()),
            ("limit", limit.to_string()),
        ])
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| ContentError::Network(error.to_string()))?;
    #[derive(Deserialize)]
    struct SearchResponse {
        hits: Vec<ContentProject>,
        offset: u32,
        limit: u8,
        total_hits: u32,
    }
    let response: SearchResponse = response
        .json()
        .map_err(|error| ContentError::InvalidResponse(error.to_string()))?;
    Ok(ContentSearchPage {
        hits: response.hits,
        offset: response.offset,
        limit: response.limit,
        total_hits: response.total_hits,
    })
}

pub fn list_project_versions(
    project_id: &str,
    game_version: &str,
    loader: Option<GameLoader>,
    kind: ContentKind,
) -> Result<Vec<ContentVersion>, ContentError> {
    let mut query = vec![("include_changelog", "false".to_string())];
    if !game_version.is_empty() {
        query.push((
            "game_versions",
            serde_json::to_string(&vec![game_version])
                .map_err(|error| ContentError::InvalidResponse(error.to_string()))?,
        ));
    }
    if kind == ContentKind::Mod {
        if let Some(loader) = loader.filter(|loader| *loader != GameLoader::Vanilla) {
            query.push((
                "loaders",
                serde_json::to_string(&vec![loader_name(loader)])
                    .map_err(|error| ContentError::InvalidResponse(error.to_string()))?,
            ));
        }
    } else if kind == ContentKind::Datapack {
        query.push((
            "loaders",
            serde_json::to_string(&vec!["datapack"])
                .map_err(|error| ContentError::InvalidResponse(error.to_string()))?,
        ));
    }
    get_json_with_query(
        &format!("{MODRINTH_API}/project/{project_id}/version"),
        &query,
    )
}

pub fn list_instance_content(instance_id: &str) -> Result<Vec<ManagedContent>, ContentError> {
    let paths = paths()?;
    let instance = instance_by_id(&paths, instance_id)?;
    let root = instance_root(&paths, &instance);
    let mut manifest = load_manifest(&root)?;
    let managed_paths = manifest
        .items
        .iter()
        .map(|item| {
            (
                item.kind,
                item.world_name
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                item.file_name.to_ascii_lowercase(),
            )
        })
        .collect::<HashSet<_>>();
    for kind in [
        ContentKind::Mod,
        ContentKind::Resourcepack,
        ContentKind::Shader,
    ] {
        let directory = content_directory(&root, kind, None)?;
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&directory)
            .map_err(|source| io_error("读取实例内容目录", &directory, source))?
        {
            let entry =
                entry.map_err(|source| io_error("读取实例内容目录项", &directory, source))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !managed_paths.contains(&(kind, String::new(), name.to_ascii_lowercase())) {
                let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                manifest.items.push(ManagedContent {
                    id: format!("local:{}:{}", kind_name(kind), stable_id(&name)),
                    kind,
                    source: "local".to_string(),
                    project_id: None,
                    version_id: None,
                    version_number: None,
                    name: name.clone(),
                    file_name: name,
                    sha1: None,
                    sha512: None,
                    size,
                    enabled: true,
                    managed: false,
                    installed_epoch_ms: 0,
                    dependencies: Vec::new(),
                    mod_metadata: None,
                    diagnostics: Vec::new(),
                    world_name: None,
                });
            }
        }
    }
    scan_world_datapacks(&root, &mut manifest)?;
    enrich_mod_diagnostics(&root, &instance, &mut manifest.items)?;
    Ok(manifest.items)
}

pub fn list_instance_worlds(instance_id: &str) -> Result<Vec<WorldInfo>, ContentError> {
    let paths = paths()?;
    let instance = instance_by_id(&paths, instance_id)?;
    let saves = instance_root(&paths, &instance).join("game").join("saves");
    if !saves.is_dir() {
        return Ok(Vec::new());
    }
    let mut worlds = Vec::new();
    for entry in fs::read_dir(&saves).map_err(|source| io_error("读取存档目录", &saves, source))?
    {
        let entry = entry.map_err(|source| io_error("读取存档目录项", &saves, source))?;
        let path = entry.path();
        if !path.is_dir() || !path.join("level.dat").is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if safe_file_name(&name).is_err() {
            continue;
        }
        let datapack_count = fs::read_dir(path.join("datapacks"))
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(0);
        worlds.push(WorldInfo {
            name,
            datapack_count,
        });
    }
    worlds.sort_by_key(|world| world.name.to_ascii_lowercase());
    Ok(worlds)
}

fn scan_world_datapacks(root: &Path, manifest: &mut ContentManifest) -> Result<(), ContentError> {
    let saves = root.join("game").join("saves");
    if !saves.is_dir() {
        return Ok(());
    }
    let managed = manifest
        .items
        .iter()
        .filter(|item| item.kind == ContentKind::Datapack)
        .map(|item| {
            (
                item.world_name
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                item.file_name.to_ascii_lowercase(),
            )
        })
        .collect::<HashSet<_>>();
    for world_entry in
        fs::read_dir(&saves).map_err(|source| io_error("读取存档目录", &saves, source))?
    {
        let world_entry =
            world_entry.map_err(|source| io_error("读取存档目录项", &saves, source))?;
        let world_path = world_entry.path();
        if !world_path.is_dir() || !world_path.join("level.dat").is_file() {
            continue;
        }
        let world_name = world_entry.file_name().to_string_lossy().into_owned();
        if safe_file_name(&world_name).is_err() {
            continue;
        }
        let directory = world_path.join("datapacks");
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&directory)
            .map_err(|source| io_error("读取数据包目录", &directory, source))?
        {
            let entry = entry.map_err(|source| io_error("读取数据包目录项", &directory, source))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if managed.contains(&(world_name.to_ascii_lowercase(), name.to_ascii_lowercase())) {
                continue;
            }
            let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            manifest.items.push(ManagedContent {
                id: format!(
                    "local:datapack:{}",
                    stable_id(&format!("{world_name}/{name}"))
                ),
                kind: ContentKind::Datapack,
                source: "local".to_string(),
                project_id: None,
                version_id: None,
                version_number: None,
                name: name.clone(),
                file_name: name,
                sha1: None,
                sha512: None,
                size,
                enabled: true,
                managed: false,
                installed_epoch_ms: 0,
                dependencies: Vec::new(),
                mod_metadata: None,
                diagnostics: Vec::new(),
                world_name: Some(world_name.clone()),
            });
        }
    }
    Ok(())
}

pub fn install_content(
    request: InstallContentRequest,
) -> Result<Vec<ManagedContent>, ContentError> {
    let paths = paths()?;
    let instance = instance_by_id(&paths, &request.instance_id)?;
    if request.kind == ContentKind::Modpack {
        return Err(ContentError::Incompatible(
            "整合包必须创建新实例".to_string(),
        ));
    }
    let root = instance_root(&paths, &instance);
    let mut manifest = load_manifest(&root)?;
    let selected: ContentVersion =
        get_json(&format!("{MODRINTH_API}/version/{}", request.version_id))?;
    ensure_compatible(&selected, &instance, request.kind)?;
    let mut visiting = HashSet::new();
    let mut resolved = Vec::new();
    resolve_required_versions(
        &selected,
        &instance,
        request.kind,
        request.world_name.as_deref(),
        &manifest,
        &mut visiting,
        &mut resolved,
    )?;
    resolved.push((selected, request.kind));

    let settings = load_or_create_download_settings(&paths)?;
    for (version, kind) in resolved {
        install_resolved_version(
            &paths,
            &root,
            &settings,
            &mut manifest,
            version,
            kind,
            request.world_name.as_deref(),
        )?;
    }
    save_manifest(&root, &manifest)?;
    list_instance_content(&request.instance_id)
}

pub fn import_local_content(
    request: ImportLocalContentRequest,
) -> Result<Vec<ManagedContent>, ContentError> {
    if request.kind == ContentKind::Modpack {
        return Err(ContentError::Incompatible(
            "整合包必须创建新实例".to_string(),
        ));
    }
    let paths = paths()?;
    let instance = instance_by_id(&paths, &request.instance_id)?;
    if !request.source_path.is_file() {
        return Err(ContentError::InvalidResponse(
            "请选择一个本地内容文件".to_string(),
        ));
    }
    let file_name = request
        .source_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ContentError::UnsafePath(request.source_path.display().to_string()))?;
    let file_name = safe_file_name(file_name)?;
    let extension = Path::new(&file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if (request.kind == ContentKind::Mod && extension != "jar")
        || (request.kind != ContentKind::Mod && extension != "zip")
    {
        return Err(ContentError::Incompatible(match request.kind {
            ContentKind::Mod => "Mod 必须是 .jar 文件".to_string(),
            _ => "资源包和光影包必须是 .zip 文件".to_string(),
        }));
    }
    let root = instance_root(&paths, &instance);
    let destination_directory =
        content_directory(&root, request.kind, request.world_name.as_deref())?;
    fs::create_dir_all(&destination_directory)
        .map_err(|source| io_error("创建实例内容目录", &destination_directory, source))?;
    let destination = destination_directory.join(&file_name);
    if destination.exists() {
        return Err(ContentError::Incompatible(format!(
            "实例中已存在同名文件 {file_name}"
        )));
    }
    let temporary = destination.with_extension(format!("{extension}.importing"));
    fs::copy(&request.source_path, &temporary)
        .map_err(|source| io_error("复制本地内容文件", &temporary, source))?;
    let sha1 = file_hash::<Sha1>(&temporary)?;
    let sha512 = file_hash::<Sha512>(&temporary)?;
    let size = fs::metadata(&temporary)
        .map_err(|source| io_error("读取本地内容文件", &temporary, source))?
        .len();
    fs::rename(&temporary, &destination)
        .map_err(|source| io_error("完成本地内容导入", &destination, source))?;
    let mut manifest = load_manifest(&root)?;
    manifest.items.push(ManagedContent {
        id: format!(
            "local:{}:{}:{}",
            kind_name(request.kind),
            request.world_name.as_deref().unwrap_or_default(),
            stable_id(&sha512)
        ),
        kind: request.kind,
        source: "local".to_string(),
        project_id: None,
        version_id: None,
        version_number: None,
        name: file_name.clone(),
        file_name,
        sha1: Some(sha1),
        sha512: Some(sha512),
        size,
        enabled: true,
        managed: true,
        installed_epoch_ms: now_epoch_ms(),
        dependencies: Vec::new(),
        mod_metadata: None,
        diagnostics: Vec::new(),
        world_name: request.world_name.clone(),
    });
    if let Err(error) = save_manifest(&root, &manifest) {
        let _ = fs::remove_file(&destination);
        return Err(error);
    }
    list_instance_content(&request.instance_id)
}

pub fn check_content_updates(instance_id: &str) -> Result<Vec<ContentUpdate>, ContentError> {
    let paths = paths()?;
    let instance = instance_by_id(&paths, instance_id)?;
    let root = instance_root(&paths, &instance);
    let manifest = load_manifest(&root)?;
    let mut updates = Vec::new();
    for item in manifest.items.iter().filter(|item| item.managed) {
        let (Some(project_id), Some(current_version_id)) = (&item.project_id, &item.version_id)
        else {
            continue;
        };
        let versions = list_project_versions(
            project_id,
            &instance.game_version,
            Some(instance.loader.kind),
            item.kind,
        )?;
        let latest = versions
            .iter()
            .find(|version| version.version_type == "release")
            .or_else(|| versions.first());
        if let Some(latest) = latest.filter(|latest| &latest.id != current_version_id) {
            updates.push(ContentUpdate {
                item_id: item.id.clone(),
                version_id: latest.id.clone(),
                version_number: latest.version_number.clone(),
            });
        }
    }
    Ok(updates)
}

pub fn install_modpack_controlled<F, C>(
    request: InstallModpackRequest,
    progress: F,
    control: C,
) -> Result<InstanceConfig, ContentError>
where
    F: Fn(InstallProgress) + Sync + Send,
    C: Fn() -> InstallControl + Sync + Send,
{
    let paths = paths()?;
    let settings = load_or_create_download_settings(&paths)?;
    let archive_path = match (&request.version_id, &request.local_path) {
        (Some(version_id), None) => {
            let version: ContentVersion =
                get_json(&format!("{MODRINTH_API}/version/{version_id}"))?;
            let file = version
                .files
                .iter()
                .find(|file| file.primary && file.filename.ends_with(".mrpack"))
                .or_else(|| file_with_extension(&version.files, ".mrpack"))
                .ok_or_else(|| {
                    ContentError::InvalidResponse("版本没有 .mrpack 文件".to_string())
                })?;
            let destination = paths.downloads_dir.join("modpacks").join(format!(
                "{}-{}",
                version.id,
                safe_file_name(&file.filename)?
            ));
            download_verified_controlled(&settings, file, &destination, &control)?;
            destination
        }
        (None, Some(path)) => {
            if !path.is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("mrpack")
            {
                return Err(ContentError::InvalidResponse(
                    "请选择有效的 .mrpack 文件".to_string(),
                ));
            }
            path.clone()
        }
        _ => {
            return Err(ContentError::InvalidResponse(
                "必须且只能指定一个在线版本或本地整合包".to_string(),
            ))
        }
    };
    let index = read_modpack_index(&archive_path)?;
    validate_modpack_index(&index)?;
    let (loader, loader_version) = modpack_loader(&index)?;
    let name = if request.name.trim().is_empty() {
        index.name.clone()
    } else {
        request.name.trim().to_string()
    };

    let instance = installer::install_instance_controlled(
        InstallInstanceRequest {
            name,
            version_id: index.dependencies["minecraft"].clone(),
            loader: Some(loader),
            loader_version,
        },
        |update| {
            if !matches!(update.stage, installer::InstallStage::Complete) {
                progress(update);
            }
        },
        &control,
    )?;
    let instance_id = instance.id.clone();
    let result = apply_modpack_archive(
        &paths,
        &settings,
        &instance,
        &archive_path,
        &index,
        &progress,
        &control,
    );
    match result {
        Ok(()) => Ok(instance),
        Err(error) => {
            let _ = instance::delete_instance_in(&paths, &instance_id);
            Err(error)
        }
    }
}

pub fn set_content_enabled(
    request: ContentActionRequest,
    enabled: bool,
) -> Result<Vec<ManagedContent>, ContentError> {
    let paths = paths()?;
    let instance = instance_by_id(&paths, &request.instance_id)?;
    let root = instance_root(&paths, &instance);
    let mut manifest = load_manifest(&root)?;
    let item = manifest
        .items
        .iter_mut()
        .find(|item| item.id == request.item_id)
        .ok_or_else(|| ContentError::ItemNotFound(request.item_id.clone()))?;
    if !item.managed {
        return Err(ContentError::Incompatible(
            "请先将本地文件纳入 NaCL 管理".to_string(),
        ));
    }
    if item.enabled != enabled {
        let enabled_path =
            content_directory(&root, item.kind, item.world_name.as_deref())?.join(&item.file_name);
        let disabled_directory = disabled_content_directory(&root, item)?;
        fs::create_dir_all(&disabled_directory)
            .map_err(|source| io_error("创建禁用内容目录", &disabled_directory, source))?;
        let disabled_path = disabled_directory.join(&item.file_name);
        let (source, destination) = if enabled {
            (&disabled_path, &enabled_path)
        } else {
            (&enabled_path, &disabled_path)
        };
        if destination.exists() {
            return Err(ContentError::Incompatible(format!(
                "目标文件已存在：{}",
                destination.display()
            )));
        }
        fs::rename(source, destination)
            .map_err(|error| io_error("切换内容启用状态", source, error))?;
        item.enabled = enabled;
        save_manifest(&root, &manifest)?;
    }
    list_instance_content(&request.instance_id)
}

pub fn remove_content(request: ContentActionRequest) -> Result<Vec<ManagedContent>, ContentError> {
    let paths = paths()?;
    let instance = instance_by_id(&paths, &request.instance_id)?;
    let root = instance_root(&paths, &instance);
    let mut manifest = load_manifest(&root)?;
    let index = manifest
        .items
        .iter()
        .position(|item| item.id == request.item_id)
        .ok_or_else(|| ContentError::ItemNotFound(request.item_id.clone()))?;
    if !manifest.items[index].managed {
        return Err(ContentError::Incompatible(
            "未托管文件只能从实例目录手动删除".to_string(),
        ));
    }
    let item = manifest.items.remove(index);
    let source = if item.enabled {
        content_directory(&root, item.kind, item.world_name.as_deref())?.join(&item.file_name)
    } else {
        disabled_content_directory(&root, &item)?.join(&item.file_name)
    };
    if source.exists() {
        let trash =
            root.join(".nacl")
                .join("trash")
                .join(format!("{}-{}", now_epoch_ms(), item.file_name));
        if let Some(parent) = trash.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("创建内容回收目录", parent, error))?;
        }
        fs::rename(&source, &trash)
            .map_err(|error| io_error("移入内容回收目录", &source, error))?;
    }
    save_manifest(&root, &manifest)?;
    list_instance_content(&request.instance_id)
}

pub fn set_content_enabled_batch(
    request: ContentBatchActionRequest,
    enabled: bool,
) -> Result<Vec<ManagedContent>, ContentError> {
    validate_batch(&request.item_ids)?;
    let mut items = Vec::new();
    for item_id in request.item_ids {
        items = set_content_enabled(
            ContentActionRequest {
                instance_id: request.instance_id.clone(),
                item_id,
            },
            enabled,
        )?;
    }
    Ok(items)
}

pub fn remove_content_batch(
    request: ContentBatchActionRequest,
) -> Result<Vec<ManagedContent>, ContentError> {
    validate_batch(&request.item_ids)?;
    let mut items = Vec::new();
    for item_id in request.item_ids {
        items = remove_content(ContentActionRequest {
            instance_id: request.instance_id.clone(),
            item_id,
        })?;
    }
    Ok(items)
}

pub fn update_content_batch(
    request: ContentBatchActionRequest,
) -> Result<Vec<ManagedContent>, ContentError> {
    validate_batch(&request.item_ids)?;
    let updates = check_content_updates(&request.instance_id)?;
    let selected = request.item_ids.into_iter().collect::<HashSet<_>>();
    let mut items = list_instance_content(&request.instance_id)?;
    for update in updates
        .into_iter()
        .filter(|update| selected.contains(&update.item_id))
    {
        let item = items
            .iter()
            .find(|item| item.id == update.item_id)
            .ok_or_else(|| ContentError::ItemNotFound(update.item_id.clone()))?;
        let kind = item.kind;
        let world_name = item.world_name.clone();
        items = install_content(InstallContentRequest {
            instance_id: request.instance_id.clone(),
            version_id: update.version_id,
            kind,
            world_name,
        })?;
    }
    Ok(items)
}

fn validate_batch(item_ids: &[String]) -> Result<(), ContentError> {
    if item_ids.is_empty() || item_ids.len() > 200 {
        return Err(ContentError::Incompatible(
            "批量操作必须包含 1–200 个项目".to_string(),
        ));
    }
    let unique = item_ids.iter().collect::<HashSet<_>>();
    if unique.len() != item_ids.len() {
        return Err(ContentError::Incompatible(
            "批量操作包含重复项目".to_string(),
        ));
    }
    Ok(())
}

fn resolve_required_versions(
    version: &ContentVersion,
    instance: &InstanceConfig,
    kind: ContentKind,
    world_name: Option<&str>,
    manifest: &ContentManifest,
    visiting: &mut HashSet<String>,
    output: &mut Vec<(ContentVersion, ContentKind)>,
) -> Result<(), ContentError> {
    if !visiting.insert(version.id.clone()) {
        return Err(ContentError::Incompatible(format!(
            "依赖关系存在循环：{}",
            version.name
        )));
    }
    for dependency in version
        .dependencies
        .iter()
        .filter(|dependency| dependency.dependency_type == "required")
    {
        let dependency_kind = if kind == ContentKind::Mod {
            ContentKind::Mod
        } else {
            kind
        };
        if dependency.project_id.as_ref().is_some_and(|project_id| {
            manifest.items.iter().any(|item| {
                item.project_id.as_ref() == Some(project_id)
                    && item.kind == kind
                    && (kind != ContentKind::Datapack || item.world_name.as_deref() == world_name)
            })
        }) {
            continue;
        }
        let dependency_version = if let Some(version_id) = &dependency.version_id {
            get_json(&format!("{MODRINTH_API}/version/{version_id}"))?
        } else if let Some(project_id) = &dependency.project_id {
            let versions = list_project_versions(
                project_id,
                &instance.game_version,
                Some(instance.loader.kind),
                dependency_kind,
            )?;
            versions
                .iter()
                .find(|candidate| candidate.version_type == "release")
                .or_else(|| versions.first())
                .cloned()
                .ok_or_else(|| ContentError::Incompatible(format!("缺少必需依赖 {project_id}")))?
        } else {
            return Err(ContentError::Incompatible(
                dependency
                    .file_name
                    .clone()
                    .unwrap_or_else(|| "存在无法解析的必需依赖".to_string()),
            ));
        };
        ensure_compatible(&dependency_version, instance, dependency_kind)?;
        resolve_required_versions(
            &dependency_version,
            instance,
            kind,
            world_name,
            manifest,
            visiting,
            output,
        )?;
        if !output
            .iter()
            .any(|(candidate, _)| candidate.id == dependency_version.id)
        {
            output.push((dependency_version, dependency_kind));
        }
    }
    visiting.remove(&version.id);
    Ok(())
}

fn install_resolved_version(
    paths: &AppPaths,
    root: &Path,
    settings: &DownloadSettings,
    manifest: &mut ContentManifest,
    version: ContentVersion,
    kind: ContentKind,
    world_name: Option<&str>,
) -> Result<(), ContentError> {
    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| ContentError::InvalidResponse("版本没有可下载文件".to_string()))?
        .clone();
    let file_name = safe_file_name(&file.filename)?;
    if kind == ContentKind::Datapack
        && Path::new(&file_name)
            .extension()
            .and_then(|value| value.to_str())
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("zip"))
    {
        return Err(ContentError::Incompatible(
            "数据包版本的主文件必须是 .zip".to_string(),
        ));
    }
    let destination_directory = content_directory(root, kind, world_name)?;
    fs::create_dir_all(&destination_directory)
        .map_err(|source| io_error("创建实例内容目录", &destination_directory, source))?;
    let destination = destination_directory.join(&file_name);
    let cache_file = paths
        .downloads_dir
        .join("content")
        .join(format!("{}-{file_name}", version.id));
    download_verified(settings, &file, &cache_file)?;

    if let Some(existing_index) = manifest.items.iter().position(|item| {
        item.project_id.as_ref() == Some(&version.project_id)
            && item.kind == kind
            && item.world_name.as_deref() == world_name
    }) {
        let existing = &manifest.items[existing_index];
        let existing_path = if existing.enabled {
            destination_directory.join(&existing.file_name)
        } else {
            disabled_content_directory(root, existing)?.join(&existing.file_name)
        };
        if existing_path.exists() {
            let backup = root.join(".nacl").join("backups").join(format!(
                "{}-{}",
                now_epoch_ms(),
                existing.file_name
            ));
            if let Some(parent) = backup.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| io_error("创建内容备份目录", parent, error))?;
            }
            fs::rename(&existing_path, &backup)
                .map_err(|error| io_error("备份旧内容文件", &existing_path, error))?;
        }
        manifest.items.remove(existing_index);
    }
    if destination.exists() {
        return Err(ContentError::Incompatible(format!(
            "实例中已存在同名文件 {file_name}"
        )));
    }
    fs::copy(&cache_file, &destination)
        .map_err(|source| io_error("安装内容文件", &destination, source))?;
    let dependency_projects = version
        .dependencies
        .iter()
        .filter(|dependency| dependency.dependency_type == "required")
        .filter_map(|dependency| dependency.project_id.clone())
        .collect();
    manifest.items.push(ManagedContent {
        id: format!(
            "modrinth:{}:{}",
            world_name.unwrap_or_default(),
            version.project_id
        ),
        kind,
        source: "modrinth".to_string(),
        project_id: Some(version.project_id),
        version_id: Some(version.id),
        version_number: Some(version.version_number),
        name: version.name,
        file_name,
        sha1: file.hashes.get("sha1").cloned(),
        sha512: file.hashes.get("sha512").cloned(),
        size: file.size,
        enabled: true,
        managed: true,
        installed_epoch_ms: now_epoch_ms(),
        dependencies: dependency_projects,
        mod_metadata: None,
        diagnostics: Vec::new(),
        world_name: world_name.map(str::to_string),
    });
    Ok(())
}

fn download_verified(
    settings: &DownloadSettings,
    file: &VersionFile,
    destination: &Path,
) -> Result<(), ContentError> {
    download_verified_controlled(settings, file, destination, &|| DownloadControl::Running)
}

fn download_verified_controlled<C>(
    settings: &DownloadSettings,
    file: &VersionFile,
    destination: &Path,
    control: &C,
) -> Result<(), ContentError>
where
    C: Fn() -> DownloadControl + Sync,
{
    if !file.url.starts_with("https://") {
        return Err(ContentError::InvalidResponse(
            "下载地址必须使用 HTTPS".to_string(),
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| io_error("创建内容缓存目录", parent, source))?;
    }
    if destination.is_file() && verify_file(destination, file)? {
        return Ok(());
    }
    let temporary = destination.with_extension("part");
    downloader::download(
        &client()?,
        DownloadRequest {
            label: &file.filename,
            url: &file.url,
            expected_size: file.size,
            identity: file
                .hashes
                .get("sha512")
                .or_else(|| file.hashes.get("sha1"))
                .map_or("", String::as_str),
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
        &|_| {},
    )
    .map_err(|error| match error {
        downloader::DownloadError::Cancelled => ContentError::Cancelled,
        error => ContentError::Network(error.to_string()),
    })?;
    if !verify_file(&temporary, file)? {
        downloader::cleanup_partial_state(&temporary);
        return Err(ContentError::ChecksumMismatch(file.filename.clone()));
    }
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|source| io_error("替换内容缓存文件", destination, source))?;
    }
    fs::rename(&temporary, destination)
        .map_err(|source| io_error("完成内容下载", destination, source))?;
    Ok(())
}

fn verify_file(path: &Path, file: &VersionFile) -> Result<bool, ContentError> {
    let metadata = fs::metadata(path).map_err(|source| io_error("读取内容文件", path, source))?;
    if file.size > 0 && metadata.len() != file.size {
        return Ok(false);
    }
    if let Some(expected) = file.hashes.get("sha512") {
        return Ok(file_hash::<Sha512>(path)? == expected.to_ascii_lowercase());
    }
    if let Some(expected) = file.hashes.get("sha1") {
        return Ok(file_hash::<Sha1>(path)? == expected.to_ascii_lowercase());
    }
    Err(ContentError::InvalidResponse(format!(
        "文件 {} 没有可用哈希",
        file.filename
    )))
}

fn file_hash<D: Digest + Default>(path: &Path) -> Result<String, ContentError> {
    let mut file = fs::File::open(path).map_err(|source| io_error("打开内容文件", path, source))?;
    let mut digest = D::default();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|source| io_error("读取内容文件", path, source))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn ensure_compatible(
    version: &ContentVersion,
    instance: &InstanceConfig,
    kind: ContentKind,
) -> Result<(), ContentError> {
    if !version
        .game_versions
        .iter()
        .any(|game_version| game_version == &instance.game_version)
    {
        return Err(ContentError::Incompatible(format!(
            "{} 不支持 Minecraft {}",
            version.name, instance.game_version
        )));
    }
    if kind == ContentKind::Mod {
        if instance.loader.kind == GameLoader::Vanilla {
            return Err(ContentError::Incompatible(
                "原版实例不能安装 Mod".to_string(),
            ));
        }
        let loader = loader_name(instance.loader.kind);
        if !version.loaders.iter().any(|candidate| candidate == loader) {
            return Err(ContentError::Incompatible(format!(
                "{} 不支持 {}",
                version.name, loader
            )));
        }
    } else if kind == ContentKind::Datapack
        && !version
            .loaders
            .iter()
            .any(|candidate| candidate == "datapack")
    {
        return Err(ContentError::Incompatible(format!(
            "{} 没有适用于数据包加载方式的版本",
            version.name
        )));
    }
    Ok(())
}

fn file_with_extension<'a>(files: &'a [VersionFile], extension: &str) -> Option<&'a VersionFile> {
    files.iter().find(|file| file.filename.ends_with(extension))
}

fn read_modpack_index(path: &Path) -> Result<ModpackIndex, ContentError> {
    let file = fs::File::open(path).map_err(|source| io_error("打开整合包", path, source))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| ContentError::Archive(error.to_string()))?;
    let mut entry = archive
        .by_name("modrinth.index.json")
        .map_err(|_| ContentError::Archive("缺少 modrinth.index.json".to_string()))?;
    if entry.size() > 8 * 1024 * 1024 {
        return Err(ContentError::Archive("索引文件超过 8 MiB".to_string()));
    }
    serde_json::from_reader(&mut entry)
        .map_err(|error| ContentError::Archive(format!("索引 JSON 无效：{error}")))
}

fn validate_modpack_index(index: &ModpackIndex) -> Result<(), ContentError> {
    if index.format_version != 1 || index.game != "minecraft" {
        return Err(ContentError::Archive(
            "仅支持 Modrinth formatVersion 1 的 Minecraft 整合包".to_string(),
        ));
    }
    if index.version_id.trim().is_empty() || index.name.trim().is_empty() {
        return Err(ContentError::Archive("整合包名称或版本为空".to_string()));
    }
    let game_version = index
        .dependencies
        .get("minecraft")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ContentError::Archive("缺少 Minecraft 版本依赖".to_string()))?;
    if game_version.len() > 64
        || !game_version
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_+".contains(character))
    {
        return Err(ContentError::Archive("Minecraft 版本值无效".to_string()));
    }
    for file in &index.files {
        safe_relative_path(&file.path)?;
        if file.downloads.is_empty()
            || !file.downloads.iter().any(|url| url.starts_with("https://"))
        {
            return Err(ContentError::Archive(format!(
                "文件 {} 没有 HTTPS 下载地址",
                file.path
            )));
        }
        if !file.hashes.contains_key("sha512") && !file.hashes.contains_key("sha1") {
            return Err(ContentError::Archive(format!(
                "文件 {} 缺少 SHA-512/SHA-1",
                file.path
            )));
        }
    }
    Ok(())
}

fn modpack_loader(index: &ModpackIndex) -> Result<(GameLoader, Option<String>), ContentError> {
    let candidates = [
        ("fabric-loader", GameLoader::Fabric),
        ("quilt-loader", GameLoader::Quilt),
        ("forge", GameLoader::Forge),
        ("neoforge", GameLoader::NeoForge),
    ]
    .into_iter()
    .filter_map(|(key, loader)| {
        index
            .dependencies
            .get(key)
            .map(|version| (loader, version.clone()))
    })
    .collect::<Vec<_>>();
    if candidates.len() > 1 {
        return Err(ContentError::Archive(
            "整合包声明了多个互斥的 Mod 加载器".to_string(),
        ));
    }
    let Some((loader, mut version)) = candidates.into_iter().next() else {
        return Ok((GameLoader::Vanilla, None));
    };
    if loader == GameLoader::Forge
        && !version.starts_with(&format!("{}-", index.dependencies["minecraft"]))
    {
        version = format!("{}-{version}", index.dependencies["minecraft"]);
    }
    Ok((loader, Some(version)))
}

fn apply_modpack_archive<F, C>(
    paths: &AppPaths,
    settings: &DownloadSettings,
    instance: &InstanceConfig,
    archive_path: &Path,
    index: &ModpackIndex,
    progress: &F,
    control: &C,
) -> Result<(), ContentError>
where
    F: Fn(InstallProgress) + Sync + Send,
    C: Fn() -> InstallControl + Sync + Send,
{
    let root = instance_root(paths, instance);
    let game_root = root.join("game");
    let staging = root
        .join(".nacl")
        .join(format!("modpack-stage-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&staging)
        .map_err(|source| io_error("创建整合包暂存目录", &staging, source))?;
    let result = (|| {
        let eligible_files = index
            .files
            .iter()
            .filter(|file| {
                file.env
                    .as_ref()
                    .is_none_or(|environment| environment.client != "unsupported")
            })
            .collect::<Vec<_>>();
        let total_files = eligible_files.len();
        for (position, file) in eligible_files.iter().enumerate() {
            if control() == DownloadControl::Cancelled {
                return Err(ContentError::Cancelled);
            }
            let relative = safe_relative_path(&file.path)?;
            let destination = staging.join(&relative);
            let url = file
                .downloads
                .iter()
                .find(|url| url.starts_with("https://"))
                .expect("validated HTTPS URL")
                .clone();
            let file_name = relative
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| ContentError::UnsafePath(file.path.clone()))?
                .to_string();
            progress(InstallProgress {
                stage: installer::InstallStage::Finalizing,
                completed_files: position,
                total_files,
                current_file: file.path.clone(),
                downloaded_bytes: 0,
                download_speed_bytes_per_second: 0,
                download_engine: downloader::DownloadEngine::Streaming,
                active_connections: 0,
            });
            download_verified_controlled(
                settings,
                &VersionFile {
                    hashes: file.hashes.clone(),
                    url,
                    filename: file_name,
                    primary: true,
                    size: file.file_size,
                },
                &destination,
                control,
            )?;
        }
        extract_modpack_overrides(archive_path, &staging)?;
        move_staged_tree(&staging, &game_root)?;

        let metadata_path = root.join(".nacl").join("modpack.json");
        let metadata_parent = metadata_path.parent().expect("metadata has parent");
        let mut temporary = NamedTempFile::new_in(metadata_parent)
            .map_err(|source| io_error("创建整合包元数据", metadata_parent, source))?;
        serde_json::to_writer_pretty(temporary.as_file_mut(), index)
            .map_err(|error| ContentError::Archive(error.to_string()))?;
        temporary
            .persist(&metadata_path)
            .map_err(|error| io_error("保存整合包元数据", &metadata_path, error.error))?;

        let mut manifest = ContentManifest::default();
        for file in &index.files {
            let relative = safe_relative_path(&file.path)?;
            let mut components = relative.components();
            let Some(Component::Normal(directory)) = components.next() else {
                continue;
            };
            if components.clone().count() != 1 {
                continue;
            }
            let kind = match directory.to_string_lossy().as_ref() {
                "mods" => ContentKind::Mod,
                "resourcepacks" => ContentKind::Resourcepack,
                "shaderpacks" => ContentKind::Shader,
                _ => continue,
            };
            let file_name = relative
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| ContentError::UnsafePath(file.path.clone()))?
                .to_string();
            manifest.items.push(ManagedContent {
                id: format!("mrpack:{}", stable_id(&file.path)),
                kind,
                source: "modrinth".to_string(),
                project_id: None,
                version_id: None,
                version_number: Some(index.version_id.clone()),
                name: file_name.clone(),
                file_name,
                sha1: file.hashes.get("sha1").cloned(),
                sha512: file.hashes.get("sha512").cloned(),
                size: file.file_size,
                enabled: true,
                managed: true,
                installed_epoch_ms: now_epoch_ms(),
                dependencies: Vec::new(),
                mod_metadata: None,
                diagnostics: Vec::new(),
                world_name: None,
            });
        }
        save_manifest(&root, &manifest)?;
        progress(InstallProgress {
            stage: installer::InstallStage::Complete,
            completed_files: total_files,
            total_files,
            current_file: index.name.clone(),
            downloaded_bytes: 0,
            download_speed_bytes_per_second: 0,
            download_engine: downloader::DownloadEngine::Cache,
            active_connections: 0,
        });
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

fn extract_modpack_overrides(archive_path: &Path, staging: &Path) -> Result<(), ContentError> {
    let file = fs::File::open(archive_path)
        .map_err(|source| io_error("打开整合包覆盖文件", archive_path, source))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| ContentError::Archive(error.to_string()))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ContentError::Archive(error.to_string()))?;
        let name = entry.name().replace('\\', "/");
        let relative = ["overrides/", "client-overrides/"]
            .into_iter()
            .find_map(|prefix| name.strip_prefix(prefix));
        let Some(relative) = relative else { continue };
        if relative.is_empty() {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(ContentError::Archive(format!(
                "覆盖文件不允许符号链接：{name}"
            )));
        }
        let destination = staging.join(safe_relative_path(relative)?);
        if entry.is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|source| io_error("创建整合包覆盖目录", &destination, source))?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| io_error("创建整合包覆盖目录", parent, source))?;
        }
        if destination.exists() {
            return Err(ContentError::Archive(format!(
                "覆盖文件与索引文件冲突：{relative}"
            )));
        }
        let mut output = fs::File::create(&destination)
            .map_err(|source| io_error("创建整合包覆盖文件", &destination, source))?;
        io::copy(&mut entry, &mut output)
            .map_err(|source| io_error("解压整合包覆盖文件", &destination, source))?;
    }
    Ok(())
}

fn move_staged_tree(staging: &Path, game_root: &Path) -> Result<(), ContentError> {
    for entry in walkdir::WalkDir::new(staging).min_depth(1) {
        let entry = entry.map_err(|error| {
            io_error(
                "读取整合包暂存目录",
                error.path().unwrap_or(staging),
                io::Error::other(error.to_string()),
            )
        })?;
        let relative = entry
            .path()
            .strip_prefix(staging)
            .map_err(|error| ContentError::UnsafePath(error.to_string()))?;
        let destination = game_root.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|source| io_error("创建整合包实例目录", &destination, source))?;
        } else {
            if destination.exists() {
                return Err(ContentError::Archive(format!(
                    "实例目标文件已存在：{}",
                    relative.display()
                )));
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|source| io_error("创建整合包实例目录", parent, source))?;
            }
            fs::rename(entry.path(), &destination)
                .map_err(|source| io_error("写入整合包实例文件", &destination, source))?;
        }
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<PathBuf, ContentError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 512
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || value.chars().any(char::is_control)
    {
        return Err(ContentError::UnsafePath(value.to_string()));
    }
    Ok(path.to_path_buf())
}

fn load_manifest(root: &Path) -> Result<ContentManifest, ContentError> {
    let path = manifest_path(root);
    if !path.is_file() {
        return Ok(ContentManifest::default());
    }
    let contents =
        fs::read_to_string(&path).map_err(|source| io_error("读取内容清单", &path, source))?;
    let manifest: ContentManifest =
        serde_json::from_str(&contents).map_err(|source| ContentError::InvalidManifest {
            path: path.clone(),
            source,
        })?;
    if manifest.schema_version != CONTENT_SCHEMA_VERSION {
        return Err(ContentError::InvalidResponse(format!(
            "不支持的内容清单版本 {}",
            manifest.schema_version
        )));
    }
    Ok(manifest)
}

fn save_manifest(root: &Path, manifest: &ContentManifest) -> Result<(), ContentError> {
    let path = manifest_path(root);
    let parent = path.parent().expect("manifest has a parent");
    fs::create_dir_all(parent).map_err(|source| io_error("创建内容清单目录", parent, source))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|source| io_error("创建内容清单临时文件", parent, source))?;
    serde_json::to_writer_pretty(temporary.as_file_mut(), manifest).map_err(|source| {
        ContentError::InvalidManifest {
            path: path.clone(),
            source,
        }
    })?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|source| io_error("同步内容清单", temporary.path(), source))?;
    temporary
        .persist(&path)
        .map_err(|error| io_error("替换内容清单", &path, error.error))?;
    Ok(())
}

fn client() -> Result<reqwest::blocking::Client, ContentError> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| ContentError::Network(error.to_string()))
}

fn get_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, ContentError> {
    get_json_with_query(url, &[])
}

fn get_json_with_query<T: for<'de> Deserialize<'de>>(
    url: &str,
    query: &[(&str, String)],
) -> Result<T, ContentError> {
    let pairs = query
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect::<Vec<_>>();
    client()?
        .get(url)
        .query(&pairs)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| ContentError::Network(error.to_string()))?
        .json()
        .map_err(|error| ContentError::InvalidResponse(error.to_string()))
}

fn paths() -> Result<AppPaths, ContentError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    Ok(paths)
}

fn instance_by_id(paths: &AppPaths, instance_id: &str) -> Result<InstanceConfig, ContentError> {
    instance::list_instances_in(paths)?
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| ContentError::InstanceNotFound(instance_id.to_owned()))
}

fn instance_root(paths: &AppPaths, instance: &InstanceConfig) -> PathBuf {
    paths.instances_dir.join(&instance.id)
}

fn enrich_mod_diagnostics(
    root: &Path,
    instance: &InstanceConfig,
    items: &mut [ManagedContent],
) -> Result<(), ContentError> {
    for item in items
        .iter_mut()
        .filter(|item| item.kind == ContentKind::Mod)
    {
        item.diagnostics.clear();
        let path = if item.enabled {
            content_directory(root, ContentKind::Mod, None)?.join(&item.file_name)
        } else {
            root.join(".nacl")
                .join("disabled")
                .join("mod")
                .join(&item.file_name)
        };
        match inspect_mod_file(&path) {
            Ok(metadata) => {
                let compatible = match metadata.format.as_str() {
                    "fabric" => {
                        matches!(instance.loader.kind, GameLoader::Fabric | GameLoader::Quilt)
                    }
                    "quilt" => instance.loader.kind == GameLoader::Quilt,
                    "forge" => instance.loader.kind == GameLoader::Forge,
                    "neoforge" => instance.loader.kind == GameLoader::NeoForge,
                    _ => true,
                };
                if !compatible {
                    item.diagnostics.push(format!(
                        "该文件是 {} Mod，但实例加载器为 {}",
                        metadata.format,
                        loader_name(instance.loader.kind)
                    ));
                }
                if item.source == "local" {
                    item.name = metadata.name.clone();
                    item.version_number = Some(metadata.version.clone());
                }
                item.mod_metadata = Some(metadata);
            }
            Err(message) => {
                item.mod_metadata = None;
                item.diagnostics.push(message);
            }
        }
    }

    let mut owners = HashMap::<String, Vec<usize>>::new();
    for (index, item) in items.iter().enumerate() {
        if item.kind != ContentKind::Mod || !item.enabled {
            continue;
        }
        if let Some(metadata) = &item.mod_metadata {
            for id in &metadata.mod_ids {
                owners
                    .entry(id.to_ascii_lowercase())
                    .or_default()
                    .push(index);
            }
        }
    }
    let mut builtin = HashSet::from(["minecraft", "java"]);
    builtin.insert(loader_name(instance.loader.kind));
    match instance.loader.kind {
        GameLoader::Fabric => {
            builtin.insert("fabricloader");
            builtin.insert("fabric-loader");
        }
        GameLoader::Quilt => {
            builtin.insert("quilt_loader");
            builtin.insert("fabricloader");
        }
        GameLoader::Forge => {
            builtin.insert("forge");
        }
        GameLoader::NeoForge => {
            builtin.insert("neoforge");
        }
        GameLoader::Vanilla => {}
    }

    for (id, indexes) in &owners {
        if indexes.len() > 1 {
            for index in indexes {
                items[*index].diagnostics.push(format!("重复 Mod ID：{id}"));
            }
        }
    }
    let mut present = owners.keys().cloned().collect::<HashSet<_>>();
    for item in items
        .iter()
        .filter(|item| item.kind == ContentKind::Mod && item.enabled)
    {
        if let Some(metadata) = &item.mod_metadata {
            present.extend(
                metadata
                    .provided_mod_ids
                    .iter()
                    .map(|id| id.to_ascii_lowercase()),
            );
        }
    }
    for item in items
        .iter_mut()
        .filter(|item| item.kind == ContentKind::Mod && item.enabled)
    {
        let Some(metadata) = &item.mod_metadata else {
            continue;
        };
        for dependency in metadata
            .dependencies
            .iter()
            .filter(|dependency| dependency.mandatory)
        {
            let id = dependency.id.to_ascii_lowercase();
            if !present.contains(&id) && !builtin.contains(id.as_str()) {
                item.diagnostics.push(if dependency.requirement.is_empty() {
                    format!("缺少前置：{}", dependency.id)
                } else {
                    format!("缺少前置：{} {}", dependency.id, dependency.requirement)
                });
            }
        }
        item.diagnostics.sort();
        item.diagnostics.dedup();
    }
    Ok(())
}

fn inspect_mod_file(path: &Path) -> Result<ModMetadata, String> {
    let file = fs::File::open(path).map_err(|error| format!("无法读取 Mod 文件：{error}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("不是有效的 Mod JAR：{error}"))?;
    if let Some(contents) = read_archive_text(&mut archive, "fabric.mod.json")? {
        let value: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|error| format!("fabric.mod.json 无效：{error}"))?;
        let mut metadata = parse_fabric_metadata_value(&value)?;
        let mut limits = FabricNestedLimits::default();
        collect_fabric_nested_mod_ids(
            &mut archive,
            &value,
            0,
            &mut limits,
            &mut metadata.provided_mod_ids,
        )?;
        let mut seen = HashSet::new();
        metadata
            .provided_mod_ids
            .retain(|id| seen.insert(id.to_ascii_lowercase()));
        return Ok(metadata);
    }
    if let Some(contents) = read_archive_text(&mut archive, "quilt.mod.json")? {
        return parse_quilt_metadata(&contents);
    }
    if let Some(contents) = read_archive_text(&mut archive, "META-INF/neoforge.mods.toml")? {
        return parse_forge_metadata(&contents, "neoforge");
    }
    if let Some(contents) = read_archive_text(&mut archive, "META-INF/mods.toml")? {
        return parse_forge_metadata(&contents, "forge");
    }
    Err("未找到 Fabric、Quilt、Forge 或 NeoForge 元数据".to_string())
}

fn read_archive_text<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Option<String>, String> {
    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(format!("无法读取 {name}：{error}")),
    };
    if entry.size() > 2 * 1024 * 1024 {
        return Err(format!("{name} 超过 2 MiB 限制"));
    }
    let mut contents = String::new();
    entry
        .read_to_string(&mut contents)
        .map_err(|error| format!("{name} 不是有效 UTF-8 文本：{error}"))?;
    Ok(Some(contents))
}

#[cfg(test)]
fn parse_fabric_metadata(contents: &str) -> Result<ModMetadata, String> {
    let value: serde_json::Value =
        serde_json::from_str(contents).map_err(|error| format!("fabric.mod.json 无效：{error}"))?;
    parse_fabric_metadata_value(&value)
}

fn parse_fabric_metadata_value(value: &serde_json::Value) -> Result<ModMetadata, String> {
    let id = json_string(value.get("id")).ok_or_else(|| "Fabric Mod 缺少 id".to_string())?;
    let name = json_string(value.get("name")).unwrap_or_else(|| id.clone());
    let version = json_string(value.get("version")).unwrap_or_else(|| "未知版本".to_string());
    let description = json_string(value.get("description")).unwrap_or_default();
    let authors = json_people(value.get("authors"));
    let mut dependencies = Vec::new();
    for (field, mandatory) in [
        ("depends", true),
        ("recommends", false),
        ("suggests", false),
    ] {
        if let Some(entries) = value.get(field).and_then(serde_json::Value::as_object) {
            dependencies.extend(entries.iter().map(|(id, requirement)| ModDependency {
                id: id.clone(),
                requirement: json_requirement(requirement),
                mandatory,
            }));
        }
    }
    Ok(ModMetadata {
        format: "fabric".to_string(),
        mod_ids: vec![id],
        provided_mod_ids: Vec::new(),
        name,
        version,
        description,
        authors,
        dependencies,
    })
}

#[derive(Default)]
struct FabricNestedLimits {
    entries: usize,
    total_size: u64,
}

fn collect_fabric_nested_mod_ids<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    metadata: &serde_json::Value,
    depth: usize,
    limits: &mut FabricNestedLimits,
    mod_ids: &mut Vec<String>,
) -> Result<(), String> {
    let Some(jars) = metadata.get("jars") else {
        return Ok(());
    };
    let jars = jars
        .as_array()
        .ok_or_else(|| "Fabric Mod 的 jars 必须是数组".to_string())?;
    if depth >= FABRIC_NESTED_MAX_DEPTH && !jars.is_empty() {
        return Err(format!(
            "Fabric 内嵌 Mod 超过 {FABRIC_NESTED_MAX_DEPTH} 层限制"
        ));
    }

    for nested in jars {
        limits.entries += 1;
        if limits.entries > FABRIC_NESTED_MAX_ENTRIES {
            return Err(format!(
                "Fabric 内嵌 Mod 超过 {FABRIC_NESTED_MAX_ENTRIES} 个限制"
            ));
        }
        let path = nested
            .get("file")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Fabric 内嵌 Mod 缺少 file".to_string())?;
        validate_fabric_nested_path(path)?;

        let bytes = read_archive_bytes(archive, path, FABRIC_NESTED_MAX_JAR_SIZE)?
            .ok_or_else(|| format!("Fabric 内嵌 Mod 不存在：{path}"))?;
        limits.total_size = limits
            .total_size
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| "Fabric 内嵌 Mod 总大小溢出".to_string())?;
        if limits.total_size > FABRIC_NESTED_MAX_TOTAL_SIZE {
            return Err(format!(
                "Fabric 内嵌 Mod 解压后总大小超过 {} MiB 限制",
                FABRIC_NESTED_MAX_TOTAL_SIZE / 1024 / 1024
            ));
        }

        let mut nested_archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| format!("Fabric 内嵌 Mod {path} 不是有效 JAR：{error}"))?;
        let Some(contents) = read_archive_text(&mut nested_archive, "fabric.mod.json")? else {
            continue;
        };
        let value: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|error| format!("{path} 中的 fabric.mod.json 无效：{error}"))?;
        let nested_metadata = parse_fabric_metadata_value(&value)?;
        mod_ids.extend(nested_metadata.mod_ids);
        collect_fabric_nested_mod_ids(&mut nested_archive, &value, depth + 1, limits, mod_ids)?;
    }
    Ok(())
}

fn read_archive_bytes<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    max_size: u64,
) -> Result<Option<Vec<u8>>, String> {
    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(format!("无法读取 {name}：{error}")),
    };
    if entry.size() > max_size {
        return Err(format!(
            "{name} 解压后超过 {} MiB 限制",
            max_size / 1024 / 1024
        ));
    }
    let mut contents = Vec::with_capacity(entry.size() as usize);
    entry
        .by_ref()
        .take(max_size + 1)
        .read_to_end(&mut contents)
        .map_err(|error| format!("无法读取 {name}：{error}"))?;
    if contents.len() as u64 > max_size {
        return Err(format!(
            "{name} 解压后超过 {} MiB 限制",
            max_size / 1024 / 1024
        ));
    }
    Ok(Some(contents))
}

fn validate_fabric_nested_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    let safe = !value.is_empty()
        && value.len() <= 512
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"));
    if safe {
        Ok(())
    } else {
        Err(format!("Fabric 内嵌 Mod 路径不安全：{value}"))
    }
}

fn parse_quilt_metadata(contents: &str) -> Result<ModMetadata, String> {
    let value: serde_json::Value =
        serde_json::from_str(contents).map_err(|error| format!("quilt.mod.json 无效：{error}"))?;
    let loader = value
        .get("quilt_loader")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Quilt Mod 缺少 quilt_loader".to_string())?;
    let id = json_string(loader.get("id")).ok_or_else(|| "Quilt Mod 缺少 id".to_string())?;
    let version = json_string(loader.get("version")).unwrap_or_else(|| "未知版本".to_string());
    let metadata = loader
        .get("metadata")
        .and_then(serde_json::Value::as_object);
    let name = metadata
        .and_then(|metadata| json_string(metadata.get("name")))
        .unwrap_or_else(|| id.clone());
    let description = metadata
        .and_then(|metadata| json_string(metadata.get("description")))
        .unwrap_or_default();
    let authors = metadata
        .map(|metadata| json_people(metadata.get("contributors")))
        .unwrap_or_default();
    let mut dependencies = Vec::new();
    if let Some(depends) = loader.get("depends").and_then(serde_json::Value::as_array) {
        for dependency in depends {
            if let Some(id) = dependency.as_str() {
                dependencies.push(ModDependency {
                    id: id.to_string(),
                    requirement: String::new(),
                    mandatory: true,
                });
            } else if let Some(object) = dependency.as_object() {
                if let Some(id) = json_string(object.get("id")) {
                    dependencies.push(ModDependency {
                        id,
                        requirement: object
                            .get("versions")
                            .map(json_requirement)
                            .unwrap_or_default(),
                        mandatory: !object
                            .get("optional")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                    });
                }
            }
        }
    }
    Ok(ModMetadata {
        format: "quilt".to_string(),
        mod_ids: vec![id],
        provided_mod_ids: Vec::new(),
        name,
        version,
        description,
        authors,
        dependencies,
    })
}

fn parse_forge_metadata(contents: &str, format: &str) -> Result<ModMetadata, String> {
    let value: toml::Value = contents
        .parse()
        .map_err(|error| format!("{format} Mod 元数据无效：{error}"))?;
    let mods = value
        .get("mods")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| format!("{format} Mod 缺少 [[mods]]"))?;
    let mod_ids = mods
        .iter()
        .filter_map(|item| item.get("modId").and_then(toml::Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    let first = mods
        .first()
        .ok_or_else(|| format!("{format} Mod 的 [[mods]] 为空"))?;
    let first_id = mod_ids
        .first()
        .cloned()
        .ok_or_else(|| format!("{format} Mod 缺少 modId"))?;
    let name = first
        .get("displayName")
        .and_then(toml::Value::as_str)
        .unwrap_or(&first_id)
        .to_string();
    let version = first
        .get("version")
        .and_then(toml::Value::as_str)
        .unwrap_or("未知版本")
        .to_string();
    let description = first
        .get("description")
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let authors = first
        .get("authors")
        .and_then(toml::Value::as_str)
        .map(|authors| {
            authors
                .split(',')
                .map(str::trim)
                .filter(|author| !author.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut dependencies = Vec::new();
    if let Some(tables) = value.get("dependencies").and_then(toml::Value::as_table) {
        for values in tables.values().filter_map(toml::Value::as_array) {
            for dependency in values {
                let Some(id) = dependency.get("modId").and_then(toml::Value::as_str) else {
                    continue;
                };
                let mandatory = dependency
                    .get("mandatory")
                    .and_then(toml::Value::as_bool)
                    .or_else(|| {
                        dependency
                            .get("type")
                            .and_then(toml::Value::as_str)
                            .map(|kind| kind == "required")
                    })
                    .unwrap_or(true);
                dependencies.push(ModDependency {
                    id: id.to_string(),
                    requirement: dependency
                        .get("versionRange")
                        .and_then(toml::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    mandatory,
                });
            }
        }
    }
    Ok(ModMetadata {
        format: format.to_string(),
        mod_ids,
        provided_mod_ids: Vec::new(),
        name,
        version,
        description,
        authors,
        dependencies,
    })
}

fn json_string(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn json_people(value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let values = value
        .as_array()
        .map_or_else(|| vec![value], |values| values.iter().collect());
    values
        .into_iter()
        .filter_map(|value| {
            value.as_str().map(str::to_string).or_else(|| {
                value
                    .as_object()
                    .and_then(|person| json_string(person.get("name")))
            })
        })
        .collect()
}

fn json_requirement(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>()
            .join(" 或 "),
        value => value.to_string(),
    }
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join(".nacl").join("content.json")
}

fn content_directory(
    root: &Path,
    kind: ContentKind,
    world_name: Option<&str>,
) -> Result<PathBuf, ContentError> {
    if kind == ContentKind::Datapack {
        let world_name = world_name.ok_or_else(|| {
            ContentError::Incompatible("安装数据包前必须选择一个存档".to_string())
        })?;
        let world_name = safe_file_name(world_name)?;
        let world = root.join("game").join("saves").join(&world_name);
        if !world.is_dir() || !world.join("level.dat").is_file() {
            return Err(ContentError::Incompatible(format!(
                "找不到有效存档 {world_name}"
            )));
        }
        return Ok(world.join("datapacks"));
    }
    if world_name.is_some() {
        return Err(ContentError::Incompatible(
            "只有数据包可以指定目标存档".to_string(),
        ));
    }
    let name = match kind {
        ContentKind::Mod => "mods",
        ContentKind::Resourcepack => "resourcepacks",
        ContentKind::Shader => "shaderpacks",
        ContentKind::Modpack => "modpacks",
        ContentKind::Datapack => unreachable!(),
    };
    Ok(root.join("game").join(name))
}

fn disabled_content_directory(root: &Path, item: &ManagedContent) -> Result<PathBuf, ContentError> {
    let mut directory = root
        .join(".nacl")
        .join("disabled")
        .join(kind_name(item.kind));
    if item.kind == ContentKind::Datapack {
        let world_name = item
            .world_name
            .as_deref()
            .ok_or_else(|| ContentError::InvalidResponse("数据包清单缺少目标存档".to_string()))?;
        directory = directory.join(safe_file_name(world_name)?);
    }
    Ok(directory)
}

fn safe_file_name(value: &str) -> Result<String, ContentError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 240
        || path.is_absolute()
        || path.components().count() != 1
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || value.chars().any(char::is_control)
    {
        return Err(ContentError::UnsafePath(value.to_owned()));
    }
    Ok(value.to_owned())
}

fn kind_name(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Mod => "mod",
        ContentKind::Resourcepack => "resourcepack",
        ContentKind::Shader => "shader",
        ContentKind::Datapack => "datapack",
        ContentKind::Modpack => "modpack",
    }
}

fn loader_name(loader: GameLoader) -> &'static str {
    match loader {
        GameLoader::Vanilla => "minecraft",
        GameLoader::Fabric => "fabric",
        GameLoader::Quilt => "quilt",
        GameLoader::Forge => "forge",
        GameLoader::NeoForge => "neoforge",
    }
}

fn stable_id(value: &str) -> String {
    let mut digest = Sha1::new();
    digest.update(value.as_bytes());
    format!("{:x}", digest.finalize())
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> ContentError {
    ContentError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        inspect_mod_file, modpack_loader, parse_fabric_metadata, parse_forge_metadata,
        safe_file_name, safe_relative_path, validate_modpack_index, ContentKind, ContentManifest,
        ModpackFile, ModpackIndex, CONTENT_SCHEMA_VERSION,
    };
    use crate::instance::GameLoader;
    use std::collections::HashMap;
    use std::io::{Cursor, Write};
    use tempfile::NamedTempFile;
    use zip::write::SimpleFileOptions;

    #[test]
    fn rejects_unsafe_content_file_names() {
        assert!(safe_file_name("../escape.jar").is_err());
        assert!(safe_file_name("folder/mod.jar").is_err());
        assert!(safe_file_name("C:\\mod.jar").is_err());
        assert_eq!(
            safe_file_name("sodium+1.21.jar").unwrap(),
            "sodium+1.21.jar"
        );
    }

    #[test]
    fn empty_manifest_uses_current_schema() {
        let manifest = ContentManifest::default();
        assert_eq!(manifest.schema_version, CONTENT_SCHEMA_VERSION);
        assert!(manifest.items.is_empty());
        let _ = ContentKind::Mod;
    }

    #[test]
    fn rejects_modpack_path_traversal_and_missing_hashes() {
        assert!(safe_relative_path("../mods/escape.jar").is_err());
        assert!(safe_relative_path("C:\\mods\\escape.jar").is_err());
        let index = ModpackIndex {
            format_version: 1,
            game: "minecraft".to_string(),
            version_id: "1.0.0".to_string(),
            name: "Test Pack".to_string(),
            summary: None,
            files: vec![ModpackFile {
                path: "mods/example.jar".to_string(),
                hashes: HashMap::new(),
                env: None,
                downloads: vec!["https://cdn.modrinth.com/example.jar".to_string()],
                file_size: 1,
            }],
            dependencies: HashMap::from([("minecraft".to_string(), "1.20.1".to_string())]),
        };
        assert!(validate_modpack_index(&index).is_err());
    }

    #[test]
    fn maps_modpack_forge_version_to_official_maven_coordinate() {
        let index = ModpackIndex {
            format_version: 1,
            game: "minecraft".to_string(),
            version_id: "1".to_string(),
            name: "Pack".to_string(),
            summary: None,
            files: Vec::new(),
            dependencies: HashMap::from([
                ("minecraft".to_string(), "1.20.1".to_string()),
                ("forge".to_string(), "47.4.0".to_string()),
            ]),
        };
        assert_eq!(
            modpack_loader(&index).unwrap(),
            (GameLoader::Forge, Some("1.20.1-47.4.0".to_string()))
        );
    }

    #[test]
    fn reads_modrinth_camel_case_file_size() {
        let index: ModpackIndex = serde_json::from_value(serde_json::json!({
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1",
            "name": "Pack",
            "files": [{
                "path": "mods/example.jar",
                "hashes": { "sha1": "0000000000000000000000000000000000000000" },
                "downloads": ["https://cdn.modrinth.com/example.jar"],
                "fileSize": 42
            }],
            "dependencies": { "minecraft": "1.20.1" }
        }))
        .expect("Modrinth index should deserialize");
        assert_eq!(index.files[0].file_size, 42);
    }

    #[test]
    fn reads_fabric_mod_metadata_and_dependencies() {
        let metadata = parse_fabric_metadata(
            r#"{
                "id":"example", "name":"Example Mod", "version":"1.2.3",
                "description":"A local mod", "authors":["NaCL"],
                "depends":{"fabricloader":">=0.16", "minecraft":["1.21.x"]},
                "suggests":{"modmenu":"*"}
            }"#,
        )
        .expect("Fabric metadata should parse");
        assert_eq!(metadata.mod_ids, vec!["example"]);
        assert_eq!(metadata.name, "Example Mod");
        assert_eq!(metadata.dependencies.len(), 3);
        assert!(metadata
            .dependencies
            .iter()
            .any(|dependency| { dependency.id == "fabricloader" && dependency.mandatory }));
        assert!(metadata
            .dependencies
            .iter()
            .any(|dependency| { dependency.id == "modmenu" && !dependency.mandatory }));
    }

    #[test]
    fn discovers_fabric_nested_mod_ids() {
        let nested = fabric_test_jar(
            r#"{"schemaVersion":1,"id":"fabric-resource-loader-v0","version":"1.0.0"}"#,
            None,
        );
        let outer = fabric_test_jar(
            r#"{
                "schemaVersion":1,
                "id":"fabric-api",
                "version":"1.0.0",
                "jars":[{"file":"META-INF/jars/resource-loader.jar"}]
            }"#,
            Some(("META-INF/jars/resource-loader.jar", nested)),
        );
        let mut file = NamedTempFile::new().expect("temporary mod should be created");
        file.write_all(&outer)
            .expect("temporary mod should be written");

        let metadata = inspect_mod_file(file.path()).expect("nested Fabric metadata should parse");
        assert_eq!(metadata.mod_ids, vec!["fabric-api"]);
        assert_eq!(metadata.provided_mod_ids, vec!["fabric-resource-loader-v0"]);
    }

    #[test]
    fn rejects_unsafe_fabric_nested_mod_paths() {
        let outer = fabric_test_jar(
            r#"{
                "schemaVersion":1,
                "id":"unsafe-example",
                "version":"1.0.0",
                "jars":[{"file":"../escape.jar"}]
            }"#,
            None,
        );
        let mut file = NamedTempFile::new().expect("temporary mod should be created");
        file.write_all(&outer)
            .expect("temporary mod should be written");

        let error = inspect_mod_file(file.path()).expect_err("unsafe nested path must be rejected");
        assert!(error.contains("路径不安全"));
    }

    fn fabric_test_jar(metadata: &str, nested: Option<(&str, Vec<u8>)>) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("fabric.mod.json", SimpleFileOptions::default())
            .expect("metadata entry should start");
        writer
            .write_all(metadata.as_bytes())
            .expect("metadata should be written");
        if let Some((path, contents)) = nested {
            writer
                .start_file(path, SimpleFileOptions::default())
                .expect("nested entry should start");
            writer
                .write_all(&contents)
                .expect("nested jar should be written");
        }
        writer
            .finish()
            .expect("test jar should finish")
            .into_inner()
    }

    #[test]
    fn reads_forge_toml_mod_metadata() {
        let metadata = parse_forge_metadata(
            r#"
                modLoader="javafml"
                loaderVersion="[52,)"
                license="MIT"
                [[mods]]
                modId="example"
                version="2.0.0"
                displayName="Forge Example"
                authors="NaCL Team"
                description='''Example description'''
                [[dependencies.example]]
                modId="forge"
                mandatory=true
                versionRange="[52,)"
            "#,
            "forge",
        )
        .expect("Forge metadata should parse");
        assert_eq!(metadata.mod_ids, vec!["example"]);
        assert_eq!(metadata.name, "Forge Example");
        assert_eq!(metadata.dependencies[0].id, "forge");
        assert!(metadata.dependencies[0].mandatory);
    }
}
