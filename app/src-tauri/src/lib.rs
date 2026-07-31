#[tauri::command]
fn detect_java_runtimes() -> Vec<launcher_core::java::JavaRuntime> {
    launcher_core::java::detect_java_runtimes()
}

#[tauri::command]
fn inspect_java_runtime(
    path: std::path::PathBuf,
) -> Result<launcher_core::java::JavaRuntime, String> {
    launcher_core::java::inspect_java_runtime(&path).map_err(|error| error.to_string())
}

#[tauri::command]
async fn install_managed_java(
    major_version: u16,
) -> Result<launcher_core::java::JavaRuntime, String> {
    tauri::async_runtime::spawn_blocking(move || {
        launcher_core::java::install_managed_runtime(major_version)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn bootstrap_app() -> Result<launcher_core::data::AppBootstrap, String> {
    launcher_core::data::bootstrap_app().map_err(|error| error.to_string())
}

#[tauri::command]
fn set_theme(
    theme: launcher_core::data::Theme,
) -> Result<launcher_core::data::LauncherSettings, String> {
    launcher_core::data::set_theme(theme).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_settings(
    settings: launcher_core::data::LauncherSettings,
) -> Result<launcher_core::data::LauncherSettings, String> {
    launcher_core::data::update_settings(settings).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_download_settings(
    settings: launcher_core::data::DownloadSettings,
) -> Result<launcher_core::data::DownloadSettings, String> {
    launcher_core::data::update_download_settings(settings).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_instances() -> Result<Vec<launcher_core::instance::InstanceConfig>, String> {
    launcher_core::instance::list_instances().map_err(|error| error.to_string())
}

#[tauri::command]
fn create_instance(
    request: launcher_core::instance::CreateInstanceRequest,
) -> Result<launcher_core::instance::InstanceConfig, String> {
    launcher_core::instance::create_instance(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn select_instance(instance_id: String) -> Result<launcher_core::data::LauncherSettings, String> {
    launcher_core::instance::select_instance(instance_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_instance(
    instance: launcher_core::instance::InstanceConfig,
) -> Result<launcher_core::instance::InstanceConfig, String> {
    launcher_core::instance::update_instance(instance).map_err(|error| error.to_string())
}

#[tauri::command]
fn duplicate_instance(
    instance_id: String,
) -> Result<launcher_core::instance::InstanceConfig, String> {
    launcher_core::instance::duplicate_instance(instance_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_instance(instance_id: String) -> Result<launcher_core::data::LauncherSettings, String> {
    launcher_core::instance::delete_instance(instance_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_logs() -> Result<Vec<launcher_core::logs::LogFile>, String> {
    launcher_core::logs::list_logs().map_err(|error| error.to_string())
}

#[tauri::command]
fn read_log(name: String) -> Result<launcher_core::logs::LogContent, String> {
    launcher_core::logs::read_log(name).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_log(name: String) -> Result<(), String> {
    launcher_core::logs::delete_log(name).map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_old_logs(retention_days: u16) -> Result<usize, String> {
    launcher_core::logs::clear_old_logs(retention_days).map_err(|error| error.to_string())
}

#[tauri::command]
fn diagnostics() -> Result<launcher_core::system::DiagnosticReport, String> {
    launcher_core::system::diagnostics().map_err(|error| error.to_string())
}

#[tauri::command]
fn storage_report() -> Result<launcher_core::system::StorageReport, String> {
    launcher_core::system::storage_report().map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_report() -> Result<launcher_core::system::MemoryReport, String> {
    launcher_core::system::memory_report().map_err(|error| error.to_string())
}

#[tauri::command]
fn clean_temporary_downloads() -> Result<u64, String> {
    launcher_core::system::clean_temporary_downloads().map_err(|error| error.to_string())
}

#[tauri::command]
async fn load_version_catalog(
    force_refresh: bool,
) -> Result<launcher_core::versions::VersionCatalog, String> {
    tauri::async_runtime::spawn_blocking(move || {
        launcher_core::versions::load_version_catalog(force_refresh)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn install_instance(
    app: tauri::AppHandle,
    state: tauri::State<'_, InstallTaskState>,
    request: launcher_core::installer::InstallInstanceRequest,
) -> Result<launcher_core::instance::InstanceConfig, String> {
    state
        .0
        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
        .map_err(|_| "已有安装任务正在运行".to_string())?;
    let control = Arc::clone(&state.0);
    tauri::async_runtime::spawn_blocking(move || {
        let result = launcher_core::installer::install_instance_controlled(
            request,
            |progress| {
                let _ = app.emit("install-progress", progress);
            },
            || match control.load(Ordering::SeqCst) {
                2 => launcher_core::installer::InstallControl::Paused,
                3 => launcher_core::installer::InstallControl::Cancelled,
                _ => launcher_core::installer::InstallControl::Running,
            },
        )
        .map_err(|error| error.to_string());
        control.store(0, Ordering::SeqCst);
        result
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn pause_install(state: tauri::State<'_, InstallTaskState>) -> Result<(), String> {
    state
        .0
        .compare_exchange(1, 2, Ordering::SeqCst, Ordering::SeqCst)
        .map(|_| ())
        .map_err(|_| "当前安装任务无法暂停".to_string())
}

#[tauri::command]
fn resume_install(state: tauri::State<'_, InstallTaskState>) -> Result<(), String> {
    state
        .0
        .compare_exchange(2, 1, Ordering::SeqCst, Ordering::SeqCst)
        .map(|_| ())
        .map_err(|_| "当前安装任务没有暂停".to_string())
}

#[tauri::command]
fn cancel_install(state: tauri::State<'_, InstallTaskState>) -> Result<(), String> {
    let current = state.0.load(Ordering::SeqCst);
    if current != 1 && current != 2 {
        return Err("当前没有可取消的安装任务".to_string());
    }
    state.0.store(3, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
fn open_directory(
    target: launcher_core::system::DirectoryTarget,
    instance_id: Option<String>,
) -> Result<std::path::PathBuf, String> {
    launcher_core::system::open_directory(target, instance_id).map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(InstallTaskState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            bootstrap_app,
            cancel_install,
            clean_temporary_downloads,
            clear_old_logs,
            create_instance,
            delete_instance,
            delete_log,
            detect_java_runtimes,
            diagnostics,
            duplicate_instance,
            inspect_java_runtime,
            install_managed_java,
            install_instance,
            list_instances,
            list_logs,
            load_version_catalog,
            memory_report,
            open_directory,
            pause_install,
            read_log,
            resume_install,
            select_instance,
            set_theme,
            storage_report,
            update_instance,
            update_download_settings,
            update_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tauri::Emitter;

#[derive(Default)]
struct InstallTaskState(Arc<AtomicU8>);
