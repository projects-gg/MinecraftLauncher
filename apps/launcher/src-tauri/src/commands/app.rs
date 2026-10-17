use serde::Serialize;

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub debug: bool,
    /// False when the OS keychain is unavailable (Linux without Secret Service): account
    /// sign-ins then last only for this session.
    pub secure_storage: bool,
    pub data_dir: String,
    /// Started at login: the window stays in the tray until the user opens it.
    pub start_minimized: bool,
    /// Installed RAM, for memory sliders.
    pub system_memory_mb: u64,
}

/// Basic facts about the running launcher, shown in Settings → About.
#[tauri::command]
#[specta::specta]
pub fn app_info(app: tauri::AppHandle, state: tauri::State<'_, crate::state::AppState>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        debug: cfg!(debug_assertions),
        secure_storage: !state.ephemeral_secrets,
        data_dir: state.root.to_string_lossy().into_owned(),
        start_minimized: crate::commands::updates::start_hidden(),
        system_memory_mb: pl_mc::instance::system_memory_mb(),
    }
}
