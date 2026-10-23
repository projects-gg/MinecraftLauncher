//! Desktop integration: tray icon, `projects://` links, the close button and the hidden-start
//! mode used by "start with the computer". Link parsing and the close decision live in
//! `pl_desktop` (tested there).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use pl_desktop::close::{self, CloseAction, CloseChoice};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Where a `projects://` link (or a tray action) sends the user. Emitted to the UI as the
/// `deep-link` event.
pub use pl_desktop::DeepLink;

/// A link that started the app before the UI could listen; taken once by the UI.
static PENDING_LINK: Mutex<Option<DeepLink>> = Mutex::new(None);

/// True when the OS started us at login (autostart passes `--minimized`): stay in the tray.
///
/// The main window is created without the focus (`"focus": false` in `tauri.conf.json`): with it,
/// the hidden window and its WebView took the foreground at every start, so after a start in the
/// tray the keys typed in another window went to an invisible one (E2E1). Every way the window is
/// shown activates it where Windows allows it: the page's first reveal and the fallback (`show`)
/// and [`focus_main_window`] (tray, second launch, links, the end of a game).
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
    pl_desktop::parse_link(
        raw,
        pl_social::normalize_friend_code,
        pl_social::normalize_crew_link,
    )
}

/// Shows the main window and brings it to the front: the tray icon, a second launch, a
/// `projects://` link, the launcher coming back after a game ([`window_bring_forward`]). The
/// player is back, so a quit waiting for jobs is called off.
pub fn focus_main_window(app: &AppHandle) {
    QUIT_WAIT.store(0, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        bring_to_front(&window);
    }
}

/// Windows: `SetForegroundWindow`, and when Windows refuses, the taskbar button flashes
/// (`pl_desktop::window::raise_window`). Not tao's `set_focus`, which then presses Alt with
/// `SendInput` (a fake key in the app the player was in). Runs on the main thread, after the
/// show and unminimize queued above.
#[cfg(windows)]
fn bring_to_front(window: &tauri::WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    // A window handle is only a number; it travels to the main thread as one.
    let raw = hwnd.0 as isize;
    let _ = window.run_on_main_thread(move || {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            FLASHW_TRAY, FLASHWINFO, FlashWindowEx, GetForegroundWindow, SetForegroundWindow,
        };

        let hwnd = HWND(raw as *mut std::ffi::c_void);
        // SAFETY: plain Win32 calls on the main window's handle, alive for the whole run.
        let in_front = unsafe { GetForegroundWindow() } == hwnd;
        let outcome = pl_desktop::window::raise_window(
            in_front,
            || unsafe { SetForegroundWindow(hwnd) }.as_bool(),
            || {
                let flash = FLASHWINFO {
                    cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                    hwnd,
                    dwFlags: FLASHW_TRAY,
                    uCount: pl_desktop::window::FLASH_COUNT,
                    dwTimeout: 0,
                };
                let _ = unsafe { FlashWindowEx(&flash) };
            },
        );
        tracing::debug!(?outcome, "main window brought forward");
    });
}

#[cfg(not(windows))]
fn bring_to_front(window: &tauri::WebviewWindow) {
    let _ = window.set_focus();
}

/// The launcher comes back after a game (`stores/launch.ts` → `restoreWindow`): shown,
/// restored and brought forward without synthetic input ([`focus_main_window`]).
#[tauri::command]
#[specta::specta]
pub fn window_bring_forward(app: AppHandle) {
    focus_main_window(&app);
}

fn open_link(app: &AppHandle, link: DeepLink) {
    focus_main_window(app);
    let _ = app.emit("deep-link", &link);
}

/// Registers the `projects://` handler; a link that started the app waits for the UI.
pub fn setup_deep_links(app: &tauri::App) {
    use tauri_plugin_deep_link::DeepLinkExt;

    // Installers register the scheme; development builds only when asked (crate::identity).
    #[cfg(all(debug_assertions, any(windows, target_os = "linux")))]
    if crate::identity::register_links()
        && let Err(error) = app.deep_link().register_all()
    {
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

/// The tray's "Çıkış": while the player's world is open to friends it asks first (they would
/// drop), otherwise it quits at once.
#[cfg(desktop)]
fn quit_from_tray(app: &AppHandle) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    let hosting = app
        .try_state::<AppState>()
        .is_some_and(|state| state.worlds.hosting());
    if !hosting {
        quit(app);
        return;
    }
    let lang = pl_lang::Lang::current();
    let handle = app.clone();
    app.dialog()
        .message(lang.pick(
            "Dünyan açık. Çıkarsan arkadaşların düşer.",
            "Your world is open. Your friends will drop if you quit.",
        ))
        .title(TRAY_TITLE)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            lang.pick("Çık", "Quit").to_owned(),
            lang.pick("Vazgeç", "Cancel").to_owned(),
        ))
        .show(move |leave| {
            if leave {
                quit(&handle);
            }
        });
}

/// Leaves for real (the tray's "Çıkış", the close button when nothing keeps the launcher,
/// "Oyun açılınca launcher: Kapansın"): AFK sessions are stopped cleanly first. A game that runs
/// plays on by itself; the next start follows it again (`commands::launch::detach_running`).
/// Exiting runs the plugins' last steps (the window's size and place are saved) and removes the
/// tray icon; the single-instance lock goes with the process.
pub(crate) fn quit(app: &AppHandle) {
    quit_for(app, pl_overlay::protocol::ByeReason::LauncherExit);
}

/// [`quit`], telling the running game's overlay why the launcher goes (`reason`).
fn quit_for(app: &AppHandle, reason: pl_overlay::protocol::ByeReason) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // World sharing first (HOST1): the mod hears `host-stop{launcher-exit}` while its bridge
        // is still there, guests hear the world closed, joins are left.
        if let Some(state) = app.try_state::<AppState>() {
            state.worlds.shutdown().await;
        }
        // The overlay next: its goodbye leaves before anything else can hold the exit up.
        crate::overlay::leaving(&app, reason).await;
        let state = app.state::<AppState>();
        if state.afk.any_active() {
            state.afk.cancel_bulk();
            state.afk.stop_all().await;
        }
        crate::commands::launch::detach_running(&state);
        app.exit(0);
    });
}

/// "Oyun açılınca launcher: Kapansın" (SMALL7), asked by the page once the game's window is on
/// screen and "Oyunda" has settled: the launcher quits and leaves the game alone. AFK accounts keep
/// it in the tray (with their one-time notice, as the close button does); a job the player started
/// finishes in the tray first (`pl_desktop::close::leave_for_game`).
#[tauri::command]
#[specta::specta]
pub fn launcher_leave_for_game(app: AppHandle) {
    let activity = close_activity(&app);
    let action = close::leave_for_game(activity);
    // The player's world open to friends (or opening), or a friend's world the player joined,
    // keeps the launcher in the tray: the way to it lives here (HOST1, host1.md §6.1).
    let action = match app.try_state::<AppState>() {
        Some(state) if state.worlds.keeps_launcher() => CloseAction::Hide {
            hint: Some(close::TrayHint::Tray),
        },
        _ => action,
    };
    tracing::info!(
        ?action,
        ?activity,
        "the game is on screen: the launcher leaves"
    );
    // The game's overlay hears that the launcher leaves for good (`bye{game-start-close}`).
    let leave = pl_overlay::protocol::ByeReason::GameStartClose;
    let Some(window) = app.get_webview_window("main") else {
        if action == CloseAction::Quit {
            quit_for(&app, leave);
        }
        return;
    };
    let _ = window.hide();
    if action == CloseAction::Quit {
        quit_for(&app, leave);
        return;
    }
    if let Some(closed) = action.closed() {
        let _ = app.emit(close::CLOSED_EVENT, closed);
    }
    if action == CloseAction::HideUntilDone {
        quit_when_done(&app, close::QuitWhenDone::beside_the_game());
    }
}

// --- the close button (TRAY1) -------------------------------------------------------------------

/// The player's choice in Settings → Genel, told by the page at its start and on every change
/// (`pl_desktop::close::CHOICE_EVENT`). Until then (or without a page) the default: quit unless
/// something keeps the launcher.
static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(false);

/// The quit that waits for jobs to end ([`CloseAction::HideUntilDone`]), by number; 0: none.
static QUIT_WAIT: AtomicU64 = AtomicU64::new(0);
static NEXT_QUIT_WAIT: AtomicU64 = AtomicU64::new(1);

/// Hears the page's close choice (call once, in setup, before the page runs).
pub fn setup_close(app: &tauri::App) {
    use tauri::Listener;

    app.listen_any(close::CHOICE_EVENT, |event| {
        if let Some(choice) = close::parse_choice(event.payload()) {
            CLOSE_TO_TRAY.store(choice == CloseChoice::Tray, Ordering::Relaxed);
        }
    });
}

/// Jobs the player started that must not stop halfway: content jobs (Modrinth installs and
/// updates, modpacks, version switches, repairs, imports, ready-made sets, a copy of an instance),
/// world backups and restores, and data jobs (a data folder move, cleanups). Not here: background
/// work that simply resumes at the next start (the chosen instance's preparation, Java runtime
/// updates) and a launcher update download, which is kept in memory: finishing it before quitting
/// would gain nothing.
fn player_jobs_running(app: &AppHandle) -> bool {
    crate::commands::modrinth::jobs_running(app)
        || crate::commands::library::jobs_running(app)
        || crate::commands::data::job_running(app)
}

/// What keeps the launcher busy now, for the close button and the quit that waits.
fn close_activity(app: &AppHandle) -> close::Activity {
    let Some(state) = app.try_state::<AppState>() else {
        return close::Activity::default();
    };
    // A game the last start left running does not keep the launcher: the player already left it
    // to play on alone ("Kapansın"), so closing the window leaves it alone again.
    let game = state
        .running
        .lock()
        .as_ref()
        .is_some_and(|game| !game.adopted);
    close::Activity {
        game: game || state.launch_cancel.lock().is_some(),
        afk: state.afk.any_active(),
        jobs: player_jobs_running(app),
    }
}

/// The main window's close button, Alt+F4 and the taskbar's "Close window" (lib.rs keeps the
/// window open and sends `CloseRequested` here, on the main thread). `pl_desktop::close` decides:
/// quit, stay in the tray, or go to the tray and quit once the player's jobs are done. The page
/// has no part in the decision, so a suspended page (`webview_rest`) never holds a close up.
pub fn close_requested(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let choice = if CLOSE_TO_TRAY.load(Ordering::Relaxed) {
        CloseChoice::Tray
    } else {
        CloseChoice::Quit
    };
    let activity = close_activity(app);
    let action = close::decide(close::Platform::current(), choice, activity);
    tracing::info!(?action, ?choice, ?activity, "window closed");
    if action == CloseAction::Quit {
        // Out of sight at once; the process ends right after.
        let _ = window.hide();
        quit(app);
        return;
    }
    // The page of a window that rested minimized may be suspended: it wakes for the notice.
    if window.is_minimized().unwrap_or(false) {
        crate::webview_rest::wake_if_suspended(app, "the window was closed");
    }
    let _ = window.hide();
    if let Some(closed) = action.closed() {
        let _ = app.emit(close::CLOSED_EVENT, closed);
    }
    if action == CloseAction::HideUntilDone {
        quit_when_done(app, close::QuitWhenDone::default());
    }
}

/// Quits once the player's jobs are done, unless the player comes back first (the window shows
/// again: [`focus_main_window`] calls it off at once) or a game or an AFK account starts (a
/// launcher that left for a game waits for the jobs only: `QuitWhenDone::beside_the_game`). Looks
/// every [`close::CHECK_EVERY`], and only while it waits.
fn quit_when_done(app: &AppHandle, mut wait: close::QuitWhenDone) {
    let number = NEXT_QUIT_WAIT.fetch_add(1, Ordering::Relaxed);
    QUIT_WAIT.store(number, Ordering::Relaxed);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(close::CHECK_EVERY).await;
            if QUIT_WAIT.load(Ordering::Relaxed) != number {
                return;
            }
            let shown = app
                .get_webview_window("main")
                .is_some_and(|window| window.is_visible().unwrap_or(false));
            let step = wait.check(std::time::Instant::now(), close_activity(&app), shown);
            if step == close::Waiting::Wait {
                continue;
            }
            // Only the wait still current acts (the player may have come back meanwhile).
            let current = QUIT_WAIT
                .compare_exchange(number, 0, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok();
            if current && step == close::Waiting::Quit {
                tracing::info!("the jobs are done: quitting as the closed window asked");
                quit(&app);
            } else if current {
                tracing::info!("the launcher stays: the window came back or a game started");
            }
            return;
        }
    });
}

/// The window was closed while a job ran, and the launcher quits once it is done
/// (`commands::data`: a finished data folder move then quits instead of restarting).
pub fn quit_pending() -> bool {
    QUIT_WAIT.load(Ordering::Relaxed) != 0
}

/// The tray icon's id and the first line of its tooltip.
#[cfg(desktop)]
const TRAY_ID: &str = "main";
#[cfg(desktop)]
const TRAY_TITLE: &str = "Projects Launcher";

/// The tray tooltip: the app's name and, while AFK accounts are in game or on their way, a second
/// line saying how many (the player sees at a glance whether the night's AFK still runs). A debug
/// build names its instance (crate::identity).
#[cfg(desktop)]
fn tray_tooltip(activity: pl_afk::AfkActivity, lang: pl_lang::Lang) -> String {
    let title = crate::identity::current().label(TRAY_TITLE);
    let accounts = |count: usize| match (lang, count) {
        (pl_lang::Lang::Tr, _) => format!("{count} AFK hesabı"),
        (pl_lang::Lang::En, 1) => "1 AFK account".to_owned(),
        (pl_lang::Lang::En, _) => format!("{count} AFK accounts"),
    };
    let line = match (activity.in_game, activity.connecting) {
        (0, 0) => return title,
        (0, connecting) => lang.pick(
            format!("{} bağlanıyor", accounts(connecting)),
            format!("{} connecting", accounts(connecting)),
        ),
        (in_game, 0) => lang.pick(
            format!("{} oyunda", accounts(in_game)),
            format!("{} in game", accounts(in_game)),
        ),
        (in_game, connecting) => lang.pick(
            format!("{} oyunda, {connecting} bağlanıyor", accounts(in_game)),
            format!("{} in game, {connecting} connecting", accounts(in_game)),
        ),
    };
    format!("{title}\n{line}")
}

/// The tray menu in the interface's language (rebuilt when it changes, [`retitle_tray`]).
#[cfg(desktop)]
fn tray_menu<R: tauri::Runtime, M: Manager<R>>(
    manager: &M,
    lang: pl_lang::Lang,
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};

    let open = lang.pick("Projects Launcher'ı aç", "Open Projects Launcher");
    let open = MenuItem::with_id(manager, "open", open, true, None::<&str>)?;
    let play = MenuItem::with_id(
        manager,
        "play",
        lang.pick("Oyna", "Play"),
        true,
        None::<&str>,
    )?;
    let afk = lang.pick("AFK hesapları", "AFK accounts");
    let afk = MenuItem::with_id(manager, "afk", afk, true, None::<&str>)?;
    let quit = lang.pick("Çıkış", "Quit");
    let quit_item = MenuItem::with_id(manager, "quit", quit, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(manager)?;
    Menu::with_items(manager, &[&open, &play, &afk, &separator, &quit_item])
}

/// The interface's language changed (`commands::app::app_set_language`): the tray menu and its
/// tooltip follow. Waits for the main thread, so it must not run on it.
#[cfg(desktop)]
pub fn retitle_tray(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        match tray_menu(app, pl_lang::Lang::current()) {
            Ok(menu) => {
                if let Err(error) = tray.set_menu(Some(menu)) {
                    tracing::debug!(%error, "tray menu not renamed");
                }
            }
            Err(error) => tracing::debug!(%error, "tray menu not rebuilt"),
        }
    }
    refresh_afk_tray(app);
}

/// "Start with the computer": a login item that starts the launcher with `--minimized`. A debug
/// build names its own (crate::identity), so turning the setting on or off in a test never
/// changes the installed launcher's login item.
#[cfg(desktop)]
pub fn autostart_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let mut builder = tauri_plugin_autostart::Builder::new().arg("--minimized");
    #[cfg(target_os = "macos")]
    {
        builder = builder.macos_launcher(tauri_plugin_autostart::MacosLauncher::LaunchAgent);
    }
    let identity = crate::identity::current();
    if identity.is_isolated() {
        builder = builder.app_name(identity.label(TRAY_TITLE));
    }
    builder.build()
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
    let text = tray_tooltip(state.afk.activity(), pl_lang::Lang::current());
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

/// Tray icon: left click opens the launcher; the menu offers play, AFK accounts and quit. Built in
/// the system's language; the UI's language replaces it as soon as the page has started.
#[cfg(desktop)]
pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let menu = tray_menu(app, pl_lang::Lang::current())?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(crate::identity::current().label(TRAY_TITLE))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => focus_main_window(app),
            "play" => open_link(app, DeepLink::Play),
            "afk" => open_link(app, DeepLink::Afk),
            "quit" => quit_from_tray(app),
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
