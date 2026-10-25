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

/// The language the UI shows, resolved there (`pl_lang`): the player's pick in Settings → Genel →
/// Dil, else Turkish in Turkey or on a Turkish system, English otherwise. The backend's own texts
/// follow it from now on: command errors, the tray menu, the Discord presence, the browser
/// sign-in page and the game's first language. Async, so the tray's main-thread updates are never
/// waited for on the main thread.
///
/// `pick` is the player's own pick (`None`: "Otomatik"), kept in the data folder for the next
/// start (`pl_lang::write_pick`): AFK accounts that start automatically connect before the page
/// names the language.
#[tauri::command]
#[specta::specta]
pub async fn app_set_language(
    app: tauri::AppHandle,
    lang: pl_lang::Lang,
    pick: Option<pl_lang::Lang>,
) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<crate::state::AppState>()
        && let Err(error) = pl_lang::write_pick(&state.root.join(pl_lang::PICK_FILE), pick)
    {
        tracing::warn!(%error, "could not keep the language pick");
    }
    if lang == pl_lang::Lang::current() {
        return;
    }
    lang.set_current();
    tracing::info!(lang = lang.code(), "interface language changed");
    #[cfg(desktop)]
    crate::shell::retitle_tray(&app);
    crate::commands::discord::refresh_language(&app);
    // The in-game overlay follows while the player has picked no language of its own (OVL-LANG).
    crate::overlay::launcher_language(&app, lang);
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
