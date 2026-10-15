//! Projects Launcher application shell: window, plugins and the typed command bridge.

mod commands;
mod error;
mod state;

use tauri::Manager;

pub fn run() {
    init_logging();

    let specta = specta_builder();
    #[cfg(debug_assertions)]
    export_bindings(&specta);

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        use tauri_plugin_window_state::StateFlags;
        // Single instance must be the first plugin so a second launch only focuses us.
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                focus_main_window(app);
            }))
            .plugin(
                tauri_plugin_window_state::Builder::new()
                    .with_state_flags(
                        StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED,
                    )
                    .build(),
            )
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--minimized"]),
            ));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(specta.invoke_handler())
        .setup(move |app| {
            specta.mount_events(app);
            let version = app.package_info().version.to_string();
            let state = state::AppState::init(app.handle(), &version)?;
            // Cached news art and instance files are shown through the asset protocol.
            app.asset_protocol_scope()
                .allow_directory(state.root.join("cache"), true)?;
            app.manage(state);
            reveal_window_fallback(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Projects Launcher");
}

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        // Byte counts, timestamps and play time are u64 but stay far below 2^53.
        .dangerously_cast_bigints_to_number()
        // Emitted as plain Tauri events ("afk-event"), so export the payload type explicitly.
        .typ::<pl_afk::AfkEvent>()
        .commands(tauri_specta::collect_commands![
            commands::app::app_info,
            commands::accounts::accounts_list,
            commands::accounts::account_add_offline,
            commands::accounts::account_login_microsoft,
            commands::accounts::account_cancel_login,
            commands::accounts::account_set_active,
            commands::accounts::account_remove,
            commands::accounts::account_import_legacy,
            commands::content::content_home,
            commands::discord::discord_update,
            commands::discord::discord_set_enabled,
            commands::instances::instances_list,
            commands::instances::instance_create,
            commands::instances::instance_delete,
            commands::instances::instance_duplicate,
            commands::instances::instance_open_folder,
            commands::instances::instance_detail,
            commands::instances::instance_update_settings,
            commands::instances::mod_set_enabled,
            commands::instances::minecraft_versions,
            commands::instances::loader_versions,
            commands::launch::launch_instance,
            commands::launch::launch_cancel,
            commands::launch::game_kill,
            commands::launch::game_running,
            commands::server::server_status,
            commands::afk::afk_overview,
            commands::afk::afk_add,
            commands::afk::afk_remove,
            commands::afk::afk_connect,
            commands::afk::afk_disconnect,
            commands::afk::afk_connect_all,
            commands::afk::afk_stop_all,
            commands::afk::afk_send,
            commands::afk::afk_console,
            commands::afk::afk_any_active,
            commands::migrate::legacy_scan,
            commands::migrate::legacy_import,
            commands::updates::update_check,
            commands::updates::update_download,
            commands::updates::update_apply,
            commands::updates::app_restart,
        ])
}

/// Writes the typed command/event bindings used by the React app. Runs automatically in debug
/// builds; `cargo run -p projects-launcher --bin export-bindings` does it without a window.
pub fn export_typescript_bindings(path: &std::path::Path) -> Result<(), String> {
    specta_builder()
        .export(specta_typescript::Typescript::default(), path)
        .map_err(|error| error.to_string())
}

pub fn default_bindings_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts")
}

#[cfg(debug_assertions)]
fn export_bindings(_builder: &tauri_specta::Builder<tauri::Wry>) {
    if let Err(error) = export_typescript_bindings(&default_bindings_path()) {
        tracing::error!(%error, "failed to export TypeScript bindings");
    }
}

fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,projects_launcher_lib=debug,pl_=debug"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The window starts hidden and the frontend reveals it after first paint.
/// If the frontend fails to load, show it anyway so the user is never left with nothing.
fn reveal_window_fallback(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        if let Some(window) = app.get_webview_window("main")
            && !window.is_visible().unwrap_or(true)
        {
            let _ = window.show();
        }
    });
}
