//! Desktop integration: tray icon, `projects://` links and the hidden-start mode used by
//! "start with the computer". Link parsing lives in `pl_desktop` (tested there).

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Where a `projects://` link (or a tray action) sends the user. Emitted to the UI as the
/// `deep-link` event.
pub use pl_desktop::DeepLink;

/// A link that started the app before the UI could listen; taken once by the UI.
static PENDING_LINK: Mutex<Option<DeepLink>> = Mutex::new(None);

/// True when the OS started us at login (autostart passes `--minimized`): stay in the tray.
pub fn start_minimized() -> bool {
    std::env::args().any(|arg| arg == "--minimized")
}

/// The window-state plugin's file (size, position, maximized), relative to the app config
/// folder. With `PROJECTS_LAUNCHER_DATA_DIR` (portable installs, development, tests) it is an
/// absolute path in that folder, so a test run never changes the player's window. "v2": sizes
/// saved under the old file are not reused (they had grown, see [`fit_main_window`]); the first
/// start picks a size for the screen instead.
pub fn window_state_file() -> String {
    const NAME: &str = ".window-state-v2.json";
    match std::env::var_os("PROJECTS_LAUNCHER_DATA_DIR").filter(|dir| !dir.is_empty()) {
        Some(dir) => std::path::Path::new(&dir)
            .join(NAME)
            .to_string_lossy()
            .into_owned(),
        None => NAME.to_owned(),
    }
}

/// The main window's saved state: physical inner size and whether it was maximized.
#[cfg(desktop)]
fn saved_main_window(app: &tauri::App) -> Option<(tauri::PhysicalSize<u32>, bool)> {
    use tauri_plugin_window_state::AppHandleExt;

    // The plugin writes its file only when the app exits, so this is still the saved state.
    let path = app
        .path()
        .app_config_dir()
        .ok()?
        .join(app.handle().filename());
    let states: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let main = &states["main"];
    let size = |key: &str| u32::try_from(main[key].as_u64().unwrap_or(0)).unwrap_or(0);
    let (width, height) = (size("width"), size("height"));
    (width > 0 && height > 0).then(|| {
        let maximized = main["maximized"].as_bool().unwrap_or(false);
        (tauri::PhysicalSize::new(width, height), maximized)
    })
}

/// Sets the window's inner size exactly. On Windows, tao's `set_size` for a window without
/// decorations adds the current outer-minus-inner difference of the frame. A window that has not
/// laid out its frame yet still counts the caption it is about to drop (measured: inner 1200 × 682
/// inside 1216 × 721 instead of 1200 × 712 inside 1216 × 721), so its first resize comes out 30 px
/// taller (742 for 712); the window-state plugin's restore hit exactly this and the window grew
/// 30 px on every start. The first call lays out the frame, the second one is exact (measured in
/// the real app on first start and on restore; harmless where the first one already was).
#[cfg(desktop)]
fn set_inner_size_exact(window: &tauri::WebviewWindow, target: tauri::PhysicalSize<u32>) {
    let _ = window.set_size(target);
    let _ = window.set_size(target);
    tracing::debug!(?target, inner = ?window.inner_size().ok(), "window inner size set");
}

/// Sizes the still hidden main window. First start (no saved state): a size for the screen it
/// opens on (`pl_desktop::default_window_size`), centred. Later starts: the player's own size,
/// which the window-state plugin restored when the window was created; it is set again exactly,
/// because the plugin's restore went through the same caption-height error and the window grew
/// 30 px on every start (the old file had reached 1280 × 1000).
#[cfg(desktop)]
pub fn fit_main_window(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Some((size, maximized)) = saved_main_window(app) {
        if !maximized {
            set_inner_size_exact(&window, size);
        }
        return;
    }
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let scale = monitor.as_ref().map_or(1.0, tauri::Monitor::scale_factor);
    let (width, height) = match &monitor {
        Some(monitor) => {
            let area = monitor.work_area().size.to_logical::<f64>(scale);
            pl_desktop::default_window_size(area.width, area.height)
        }
        None => pl_desktop::default_window_size(0.0, 0.0),
    };
    set_inner_size_exact(
        &window,
        tauri::LogicalSize::new(f64::from(width), f64::from(height)).to_physical(scale),
    );
    let _ = window.center();
    tracing::info!(width, height, "first start: window sized for this screen");
}

/// Parses `projects://<route>[/<id>]` (desktop shortcuts pass `projects://play/<instance>`).
/// Anything unknown is ignored rather than guessed.
pub fn parse_link(raw: &str) -> Option<DeepLink> {
    pl_desktop::parse_link(raw, pl_social::normalize_friend_code)
}

pub fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn open_link(app: &AppHandle, link: DeepLink) {
    focus_main_window(app);
    let _ = app.emit("deep-link", &link);
}

/// Registers the `projects://` handler; a link that started the app waits for the UI.
pub fn setup_deep_links(app: &tauri::App) {
    use tauri_plugin_deep_link::DeepLinkExt;

    // Installers register the scheme; development builds register it here.
    #[cfg(all(debug_assertions, any(windows, target_os = "linux")))]
    if let Err(error) = app.deep_link().register_all() {
        tracing::debug!(%error, "could not register projects:// for development");
    }
    if let Ok(Some(urls)) = app.deep_link().get_current()
        && let Some(link) = urls.iter().find_map(|u| parse_link(u.as_str()))
    {
        *PENDING_LINK.lock().unwrap_or_else(|e| e.into_inner()) = Some(link);
    }
    let handle = app.handle().clone();
    app.deep_link().on_open_url(move |event| {
        let urls = event.urls();
        // The friends sign-in page hands back a one-time code (`projects://auth/done?c=…`); it
        // is used only by a sign-in this launcher started, never shown to the UI.
        if let Some(state) = handle.try_state::<AppState>()
            && urls
                .iter()
                .any(|u| state.social.complete_sign_in(u.as_str()))
        {
            focus_main_window(&handle);
            return;
        }
        if let Some(link) = urls.iter().find_map(|u| parse_link(u.as_str())) {
            open_link(&handle, link);
        }
    });
}

/// The link that started the app, if any (taken once).
#[tauri::command]
#[specta::specta]
pub fn deep_link_pending() -> Option<DeepLink> {
    PENDING_LINK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

/// Leaves for real: AFK sessions are stopped cleanly first.
#[tauri::command]
#[specta::specta]
pub fn app_quit(app: AppHandle) {
    quit(&app);
}

fn quit(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if state.afk.any_active() {
            state.afk.cancel_bulk();
            state.afk.stop_all().await;
        }
        app.exit(0);
    });
}

/// The tray icon's id and the first line of its tooltip.
#[cfg(desktop)]
const TRAY_ID: &str = "main";
#[cfg(desktop)]
const TRAY_TITLE: &str = "Projects Launcher";

/// The tray tooltip: the app's name and, while AFK accounts are in game or on their way, a second
/// line saying how many (the player sees at a glance whether the night's AFK still runs).
#[cfg(desktop)]
fn tray_tooltip(activity: pl_afk::AfkActivity) -> String {
    let line = match (activity.in_game, activity.connecting) {
        (0, 0) => return TRAY_TITLE.to_owned(),
        (0, connecting) => format!("{connecting} AFK hesabı bağlanıyor"),
        (in_game, 0) => format!("{in_game} AFK hesabı oyunda"),
        (in_game, connecting) => format!("{in_game} AFK hesabı oyunda, {connecting} bağlanıyor"),
    };
    format!("{TRAY_TITLE}\n{line}")
}

/// Brings the tray tooltip up to date with the AFK sessions (called after each AFK state change).
/// Blocks until the main thread has set it, so it must not run on the main thread.
#[cfg(desktop)]
pub fn refresh_afk_tray(app: &AppHandle) {
    // The tooltip last set; the lock also keeps refreshes in order, so the newest state wins.
    static SHOWN: Mutex<String> = Mutex::new(String::new());
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    let text = tray_tooltip(state.afk.activity());
    if *shown == text {
        return;
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        match tray.set_tooltip(Some(&text)) {
            Ok(()) => *shown = text,
            Err(error) => tracing::debug!(%error, "tray tooltip not updated"),
        }
    }
}

/// Tray icon: left click opens the launcher; the menu offers play, AFK accounts and quit.
#[cfg(desktop)]
pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let open = MenuItem::with_id(app, "open", "Projects Launcher'ı aç", true, None::<&str>)?;
    let play = MenuItem::with_id(app, "play", "Oyna", true, None::<&str>)?;
    let afk = MenuItem::with_id(app, "afk", "AFK hesapları", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Çıkış", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &play, &afk, &separator, &quit_item])?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TRAY_TITLE)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => focus_main_window(app),
            "play" => open_link(app, DeepLink::Play),
            "afk" => open_link(app, DeepLink::Afk),
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                focus_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
