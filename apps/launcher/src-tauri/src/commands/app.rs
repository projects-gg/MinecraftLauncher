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
    }
}
