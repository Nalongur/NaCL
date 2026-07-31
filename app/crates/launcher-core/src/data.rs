use serde::{Deserialize, Serialize};
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub enum DataError {
    MissingEnvironment(&'static str),
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    InvalidSettings {
        path: PathBuf,
        source: serde_json::Error,
    },
    InvalidSettingsValue(&'static str),
    SerializeSettings(serde_json::Error),
}

impl fmt::Display for DataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEnvironment(variable) => {
                write!(
                    formatter,
                    "required environment variable {variable} is missing"
                )
            }
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "failed to {action} {}: {source}", path.display()),
            Self::InvalidSettings { path, source } => write!(
                formatter,
                "settings file {} is invalid: {source}",
                path.display()
            ),
            Self::InvalidSettingsValue(message) => {
                write!(formatter, "settings value is invalid: {message}")
            }
            Self::SerializeSettings(source) => {
                write!(formatter, "failed to serialize settings: {source}")
            }
        }
    }
}

impl std::error::Error for DataError {}

pub type DataResult<T> = Result<T, DataError>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub roaming_root: PathBuf,
    pub config_dir: PathBuf,
    pub settings_file: PathBuf,
    pub download_settings_file: PathBuf,
    pub instances_dir: PathBuf,
    pub local_root: PathBuf,
    pub cache_dir: PathBuf,
    pub manifests_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub libraries_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub runtimes_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> DataResult<Self> {
        let roaming = env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or(DataError::MissingEnvironment("APPDATA"))?;
        let local = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or(DataError::MissingEnvironment("LOCALAPPDATA"))?;

        Ok(Self::from_roots(roaming, local))
    }

    pub fn from_roots(roaming: PathBuf, local: PathBuf) -> Self {
        let roaming_root = roaming.join("NaCL");
        let config_dir = roaming_root.join("config");
        let local_root = local.join("NaCL");
        let cache_dir = local_root.join("cache");

        Self {
            settings_file: config_dir.join("settings.json"),
            download_settings_file: config_dir.join("download-settings.json"),
            instances_dir: roaming_root.join("instances"),
            manifests_dir: cache_dir.join("manifests"),
            versions_dir: cache_dir.join("versions"),
            assets_dir: cache_dir.join("assets"),
            libraries_dir: cache_dir.join("libraries"),
            downloads_dir: cache_dir.join("downloads"),
            runtimes_dir: local_root.join("runtimes"),
            logs_dir: local_root.join("logs"),
            roaming_root,
            config_dir,
            local_root,
            cache_dir,
        }
    }

    pub fn initialize(&self) -> DataResult<()> {
        for directory in [
            &self.config_dir,
            &self.instances_dir,
            &self.manifests_dir,
            &self.versions_dir,
            &self.assets_dir,
            &self.libraries_dir,
            &self.downloads_dir,
            &self.runtimes_dir,
            &self.logs_dir,
        ] {
            fs::create_dir_all(directory)
                .map_err(|source| io_error("create directory", directory, source))?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultPage {
    #[default]
    Home,
    Instances,
    Downloads,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseBehavior {
    #[default]
    Exit,
    Minimize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimationSpeed {
    Fast,
    #[default]
    Normal,
    Relaxed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InterfaceDensity {
    Compact,
    #[default]
    Comfortable,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct LauncherSettings {
    pub schema_version: u32,
    pub theme: Theme,
    pub selected_instance_id: Option<String>,
    pub default_page: DefaultPage,
    pub remember_last_instance: bool,
    pub close_behavior: CloseBehavior,
    pub check_updates: bool,
    pub notifications: bool,
    pub animations_enabled: bool,
    pub animation_speed: AnimationSpeed,
    pub interface_density: InterfaceDensity,
    pub use_smiley_headings: bool,
    pub auto_detect_java: bool,
    pub preferred_java_path: Option<PathBuf>,
    pub manage_runtimes: bool,
    pub compatibility_warnings: bool,
    pub default_memory_mb: u32,
    pub default_window_width: u32,
    pub default_window_height: u32,
    pub log_level: LogLevel,
    pub log_retention_days: u16,
    pub auto_diagnostics: bool,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            theme: Theme::Dark,
            selected_instance_id: None,
            default_page: DefaultPage::Home,
            remember_last_instance: true,
            close_behavior: CloseBehavior::Exit,
            check_updates: true,
            notifications: true,
            animations_enabled: true,
            animation_speed: AnimationSpeed::Normal,
            interface_density: InterfaceDensity::Comfortable,
            use_smiley_headings: true,
            auto_detect_java: true,
            preferred_java_path: None,
            manage_runtimes: true,
            compatibility_warnings: true,
            default_memory_mb: 4096,
            default_window_width: 1280,
            default_window_height: 720,
            log_level: LogLevel::Info,
            log_retention_days: 14,
            auto_diagnostics: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct DownloadSettings {
    pub schema_version: u32,
    pub concurrent_downloads: u8,
    pub retry_count: u8,
    pub connection_timeout_seconds: u16,
    pub speed_limit_kib_per_second: u32,
    pub show_snapshots: bool,
    pub verify_after_download: bool,
}

impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            concurrent_downloads: 4,
            retry_count: 3,
            connection_timeout_seconds: 30,
            speed_limit_kib_per_second: 0,
            show_snapshots: false,
            verify_after_download: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrap {
    pub paths: AppPaths,
    pub settings: LauncherSettings,
    pub download_settings: DownloadSettings,
}

pub fn bootstrap_app() -> DataResult<AppBootstrap> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    let settings = load_or_create_settings(&paths)?;
    let download_settings = load_or_create_download_settings(&paths)?;

    Ok(AppBootstrap {
        paths,
        settings,
        download_settings,
    })
}

pub fn set_theme(theme: Theme) -> DataResult<LauncherSettings> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    let mut settings = load_or_create_settings(&paths)?;
    settings.theme = theme;
    save_settings(&paths, &settings)?;
    Ok(settings)
}

pub fn update_settings(settings: LauncherSettings) -> DataResult<LauncherSettings> {
    validate_settings(&settings)?;
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    save_settings(&paths, &settings)?;
    Ok(settings)
}

pub fn update_download_settings(settings: DownloadSettings) -> DataResult<DownloadSettings> {
    validate_download_settings(&settings)?;
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    save_download_settings(&paths, &settings)?;
    Ok(settings)
}

pub fn load_or_create_settings(paths: &AppPaths) -> DataResult<LauncherSettings> {
    if !paths.settings_file.is_file() {
        let settings = LauncherSettings::default();
        save_settings(paths, &settings)?;
        return Ok(settings);
    }

    let contents = fs::read_to_string(&paths.settings_file)
        .map_err(|source| io_error("read settings file", &paths.settings_file, source))?;
    serde_json::from_str(&contents).map_err(|source| DataError::InvalidSettings {
        path: paths.settings_file.clone(),
        source,
    })
}

pub fn save_settings(paths: &AppPaths, settings: &LauncherSettings) -> DataResult<()> {
    validate_settings(settings)?;
    save_json_atomically(paths, &paths.settings_file, settings)
}

pub fn load_or_create_download_settings(paths: &AppPaths) -> DataResult<DownloadSettings> {
    if !paths.download_settings_file.is_file() {
        let settings = DownloadSettings::default();
        save_download_settings(paths, &settings)?;
        return Ok(settings);
    }

    let contents = fs::read_to_string(&paths.download_settings_file).map_err(|source| {
        io_error(
            "read download settings file",
            &paths.download_settings_file,
            source,
        )
    })?;
    serde_json::from_str(&contents).map_err(|source| DataError::InvalidSettings {
        path: paths.download_settings_file.clone(),
        source,
    })
}

pub fn save_download_settings(paths: &AppPaths, settings: &DownloadSettings) -> DataResult<()> {
    validate_download_settings(settings)?;
    save_json_atomically(paths, &paths.download_settings_file, settings)
}

fn save_json_atomically<T: Serialize>(
    paths: &AppPaths,
    destination: &Path,
    value: &T,
) -> DataResult<()> {
    fs::create_dir_all(&paths.config_dir)
        .map_err(|source| io_error("create config directory", &paths.config_dir, source))?;

    let contents = serde_json::to_vec_pretty(value).map_err(DataError::SerializeSettings)?;
    let mut temporary = NamedTempFile::new_in(&paths.config_dir)
        .map_err(|source| io_error("create temporary settings file", &paths.config_dir, source))?;
    temporary
        .write_all(&contents)
        .and_then(|_| temporary.write_all(b"\n"))
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|source| io_error("write temporary settings file", temporary.path(), source))?;

    temporary
        .persist(destination)
        .map_err(|error| io_error("replace settings file", destination, error.error))?;
    Ok(())
}

fn validate_settings(settings: &LauncherSettings) -> DataResult<()> {
    if settings.schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(DataError::InvalidSettingsValue(
            "unsupported settings schema version",
        ));
    }
    if !(1024..=32768).contains(&settings.default_memory_mb) {
        return Err(DataError::InvalidSettingsValue(
            "default memory must be between 1024 MB and 32768 MB",
        ));
    }
    if !(854..=7680).contains(&settings.default_window_width)
        || !(480..=4320).contains(&settings.default_window_height)
    {
        return Err(DataError::InvalidSettingsValue(
            "default game window size is outside the supported range",
        ));
    }
    if !(1..=365).contains(&settings.log_retention_days) {
        return Err(DataError::InvalidSettingsValue(
            "log retention must be between 1 and 365 days",
        ));
    }
    if settings
        .preferred_java_path
        .as_ref()
        .is_some_and(|path| path.as_os_str().is_empty())
    {
        return Err(DataError::InvalidSettingsValue(
            "preferred Java path cannot be empty",
        ));
    }
    Ok(())
}

fn validate_download_settings(settings: &DownloadSettings) -> DataResult<()> {
    if settings.schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(DataError::InvalidSettingsValue(
            "unsupported download settings schema version",
        ));
    }
    if !(1..=16).contains(&settings.concurrent_downloads) {
        return Err(DataError::InvalidSettingsValue(
            "download concurrency must be between 1 and 16",
        ));
    }
    if settings.retry_count > 10 {
        return Err(DataError::InvalidSettingsValue(
            "download retry count must be between 0 and 10",
        ));
    }
    if !(5..=300).contains(&settings.connection_timeout_seconds) {
        return Err(DataError::InvalidSettingsValue(
            "connection timeout must be between 5 and 300 seconds",
        ));
    }
    if !settings.verify_after_download {
        return Err(DataError::InvalidSettingsValue(
            "download verification cannot be disabled",
        ));
    }
    Ok(())
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> DataError {
    DataError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        load_or_create_download_settings, load_or_create_settings, save_download_settings,
        save_settings, AppPaths, DownloadSettings, LauncherSettings, Theme,
        SETTINGS_SCHEMA_VERSION,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_roots() -> (std::path::PathBuf, AppPaths) {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nacl-launcher-core-test-{}-{suffix}",
            std::process::id()
        ));
        let paths = AppPaths::from_roots(root.join("roaming"), root.join("local"));
        (root, paths)
    }

    #[test]
    fn initializes_clear_directory_tree() {
        let (root, paths) = temporary_roots();
        paths
            .initialize()
            .expect("directory tree should initialize");

        for directory in [
            &paths.config_dir,
            &paths.instances_dir,
            &paths.manifests_dir,
            &paths.versions_dir,
            &paths.assets_dir,
            &paths.libraries_dir,
            &paths.downloads_dir,
            &paths.runtimes_dir,
            &paths.logs_dir,
        ] {
            assert!(directory.is_dir(), "{} should exist", directory.display());
        }

        fs::remove_dir_all(root).expect("temporary tree should be removable");
    }

    #[test]
    fn creates_and_updates_settings_atomically() {
        let (root, paths) = temporary_roots();
        paths
            .initialize()
            .expect("directory tree should initialize");

        let default_settings =
            load_or_create_settings(&paths).expect("default settings should be created");
        assert_eq!(default_settings.schema_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(default_settings.theme, Theme::Dark);

        let updated = LauncherSettings {
            theme: Theme::Light,
            ..default_settings
        };
        save_settings(&paths, &updated).expect("settings should be replaced");
        let reloaded = load_or_create_settings(&paths).expect("settings should reload");
        assert_eq!(reloaded, updated);

        let mut download_settings =
            load_or_create_download_settings(&paths).expect("download settings should be created");
        download_settings.concurrent_downloads = 8;
        save_download_settings(&paths, &download_settings)
            .expect("download settings should be replaced");
        assert_eq!(
            load_or_create_download_settings(&paths).expect("download settings should reload"),
            DownloadSettings {
                concurrent_downloads: 8,
                ..DownloadSettings::default()
            }
        );

        fs::remove_dir_all(root).expect("temporary tree should be removable");
    }

    #[test]
    fn loads_older_settings_with_new_defaults() {
        let (root, paths) = temporary_roots();
        paths
            .initialize()
            .expect("directory tree should initialize");
        fs::write(
            &paths.settings_file,
            r#"{
  "schemaVersion": 1,
  "theme": "light",
  "selectedInstanceId": null
}"#,
        )
        .expect("older settings fixture should be written");

        let settings = load_or_create_settings(&paths).expect("older settings should load");
        assert_eq!(settings.theme, Theme::Light);
        assert_eq!(settings.default_memory_mb, 4096);
        assert_eq!(settings.default_window_width, 1280);

        fs::remove_dir_all(root).expect("temporary tree should be removable");
    }
}
