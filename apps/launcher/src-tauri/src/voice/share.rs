//! Screen sharing in the app (SHARE; the lead's contract `docs/tasks/share.md` §6): the
//! publisher behind `pl_voice::share::ShareCmd` (SHARE-N's `pl-share`, [`super::share_native`];
//! where it cannot start, a stand-in that cannot share, so the page offers watching only), the
//! service's word on sharing ([`service_of`]), the pages that watch (the main window and the
//! "Yayın" window, each through the channel it attaches), the "Yayın" window itself, and the
//! privacy of the player's own windows while they share:
//!
//! - "Launcher'ı yayında gizle": the launcher's windows leave every capture
//!   (`WDA_EXCLUDEFROMCAPTURE`), so a whole screen never shows its chats;
//! - "Paylaşırken bildirimleri gizle": the launcher shows no Windows notification meanwhile.
//!
//! The logic and its tests live in `pl-voice` (`share.rs`, `share_machine.rs`); this only carries
//! its effects out.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use pl_voice::Voice;
use pl_voice::share::{
    SHARE_PAGE, SHARE_WINDOW, ShareCmd, ShareEvent, ShareSources, ShareSurface, WatchCmd,
};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The publisher (capture, encoding, the SFU session for each share). SHARE-N's `pl-share`
/// implements it; its events come back through [`Voice::share_event`].
pub trait Publisher: Send + Sync + 'static {
    /// Whether this computer can share at all, share an app's own sound, and encode "Yüksek" in
    /// hardware.
    fn caps(&self) -> (bool, bool, bool);
    /// The picker's list (`game_pid`: the game the launcher started, while it runs). Blocking:
    /// called off the main thread.
    fn sources(&self, game_pid: Option<u32>) -> ShareSources;
    fn command(&self, cmd: ShareCmd, game_pid: Option<u32>);
}

/// Where the publisher cannot start: nothing can be shared here (watching works all the same).
struct Unavailable {
    voice: Voice,
}

impl Publisher for Unavailable {
    fn caps(&self) -> (bool, bool, bool) {
        (false, false, false)
    }

    fn sources(&self, _game_pid: Option<u32>) -> ShareSources {
        ShareSources::default()
    }

    fn command(&self, cmd: ShareCmd, _game_pid: Option<u32>) {
        if let ShareCmd::Prepare { n, .. } = cmd {
            self.voice.share_event(ShareEvent::Lost {
                n,
                why: pl_voice::share::ShareLostWhy::Other,
            });
        }
    }
}

/// The app's share parts.
struct Parts {
    app: AppHandle,
    publisher: Arc<dyn Publisher>,
    /// The watching pages' channels: [main, popout].
    channels: Mutex<[Option<Channel<WatchCmd>>; 2]>,
}

static PARTS: OnceLock<Arc<Parts>> = OnceLock::new();
/// "Paylaşırken bildirimleri gizle" holds now.
static QUIET: AtomicBool = AtomicBool::new(false);
/// "Launcher'ı yayında gizle" holds now (a "Yayın" window opened meanwhile hides too).
static HIDDEN: AtomicBool = AtomicBool::new(false);

fn slot(surface: ShareSurface) -> usize {
    match surface {
        ShareSurface::Main => 0,
        ShareSurface::Popout => 1,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Wires the publisher to voice chat (in setup, after `voice::create`).
pub fn install(app: &AppHandle, voice: &Voice) {
    let publisher: Arc<dyn Publisher> = match super::share_native::Native::start(app, voice) {
        Some(native) => native,
        None => Arc::new(Unavailable {
            voice: voice.clone(),
        }),
    };
    let (available, audio, high) = publisher.caps();
    let _ = PARTS.set(Arc::new(Parts {
        app: app.clone(),
        publisher,
        channels: Mutex::new([None, None]),
    }));
    voice.set_share_publisher(available, audio, high);
}

/// What the service allows of screen sharing (`hello.voice.share`, SHARE-S): none while Studio's
/// switch is off or the service is older.
pub fn service_of(status: &pl_social::VoiceStatus) -> Option<pl_voice::share::ServiceShare> {
    use pl_voice::share::SharePreset;
    let share = status.share.filter(|share| share.on)?;
    Some(pl_voice::share::ServiceShare {
        per_user: share.per_user,
        max_preset: match share.max_preset {
            pl_social::SharePreset::Low => SharePreset::Low,
            pl_social::SharePreset::Normal => SharePreset::Normal,
            pl_social::SharePreset::High => SharePreset::High,
        },
        open: share.open,
        watch: share.watch,
    })
}

/// The game the launcher started and still runs (the first one), for "Oyunu paylaş".
fn game_pid(app: &AppHandle) -> Option<u32> {
    let state = app.try_state::<crate::state::AppState>()?;
    state
        .games
        .lock()
        .iter()
        .filter(|game| !game.adopted)
        .find_map(|game| game.process.pid())
}

/// The picker's list (blocking).
pub fn sources() -> ShareSources {
    let Some(parts) = PARTS.get() else {
        return ShareSources::default();
    };
    let mut sources = parts.publisher.sources(game_pid(&parts.app));
    for info in sources
        .windows
        .iter_mut()
        .chain(sources.screens.iter_mut())
        .chain(sources.game.iter_mut())
    {
        info.label = pl_voice::share::clip_label(&info.label);
    }
    sources
}

/// To the publisher (the voice effects' thread).
pub fn command(cmd: ShareCmd) {
    if let Some(parts) = PARTS.get() {
        let game = matches!(
            &cmd,
            ShareCmd::Prepare {
                source: pl_voice::share::ShareSource::Game,
                ..
            }
        )
        .then(|| game_pid(&parts.app))
        .flatten();
        parts.publisher.command(cmd, game);
    }
}

/// A watching page attached its channel.
pub fn attach(voice: &Voice, surface: ShareSurface, channel: Channel<WatchCmd>) {
    if let Some(parts) = PARTS.get() {
        lock(&parts.channels)[slot(surface)] = Some(channel);
    }
    voice.share_attach(surface);
}

/// To the page that watches on `surface`.
pub fn watch(surface: ShareSurface, cmd: WatchCmd) {
    let Some(parts) = PARTS.get() else {
        return;
    };
    let mut channels = lock(&parts.channels);
    if let Some(channel) = channels[slot(surface)].as_ref()
        && let Err(error) = channel.send(cmd)
    {
        tracing::debug!(%error, "share: the watching page did not take a command");
        channels[slot(surface)] = None;
    }
}

/// While the player shares (and as they chose): the launcher's windows out of every capture, its
/// Windows notifications quiet.
pub fn privacy(app: &AppHandle, hide: bool, quiet: bool) {
    QUIET.store(quiet, Ordering::SeqCst);
    if HIDDEN.swap(hide, Ordering::SeqCst) == hide {
        return;
    }
    for label in ["main", SHARE_WINDOW] {
        if let Some(window) = app.get_webview_window(label) {
            exclude_from_capture(&window, hide);
        }
    }
}

/// The launcher shows no Windows notification now ("Paylaşırken bildirimleri gizle").
pub fn quiet() -> bool {
    QUIET.load(Ordering::SeqCst)
}

/// Out of every capture (`WDA_EXCLUDEFROMCAPTURE`, pl-share's helper), on the window's own thread.
fn exclude_from_capture(window: &tauri::WebviewWindow, on: bool) {
    #[cfg(windows)]
    {
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let raw = hwnd.0 as isize;
        let _ = window.run_on_main_thread(move || {
            if !pl_share::exclude_from_capture(raw, on) {
                tracing::debug!("share: the window's capture rule did not change");
            }
        });
    }
    #[cfg(not(windows))]
    let _ = (window, on);
}

/// Opens (or brings to the front) or closes the "Yayın" window.
pub fn popout(app: &AppHandle, open: bool) {
    if !open {
        if let Some(window) = app.get_webview_window(SHARE_WINDOW) {
            let _ = window.destroy();
        }
        return;
    }
    if let Some(window) = app.get_webview_window(SHARE_WINDOW) {
        // No `set_focus` (CLAUDE.md): shown and restored, Windows decides the front.
        let _ = window.unminimize();
        let _ = window.show();
        return;
    }
    // Off the calling thread: a window built inside a synchronous command deadlocks WebView2.
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = open_window(&app) {
            tracing::warn!(%error, "share: the \"Yayın\" window did not open");
            if let Some(state) = app.try_state::<crate::state::AppState>() {
                state.voice.share_popout(false);
            }
        }
    });
}

fn open_window(app: &AppHandle) -> Result<(), String> {
    let tr = pl_lang::Lang::current() == pl_lang::Lang::Tr;
    let on_top = app
        .try_state::<crate::state::AppState>()
        .is_some_and(|state| state.voice.settings().share.always_on_top);
    let window = WebviewWindowBuilder::new(app, SHARE_WINDOW, WebviewUrl::App(SHARE_PAGE.into()))
        .title(if tr { "Yayın" } else { "Stream" })
        .inner_size(960.0, 580.0)
        .min_inner_size(480.0, 300.0)
        .always_on_top(on_top)
        // Its own page only (the launcher's origin, `/share.html`).
        .on_navigation(|url| {
            pl_voice::permission::is_app_origin(url.as_str())
                && url.path().trim_start_matches('/') == SHARE_PAGE
        })
        .build()
        .map_err(|error| error.to_string())?;
    // No microphone, no camera; the page never asks for the screen either.
    super::window::guard_media(&window);
    if HIDDEN.load(Ordering::SeqCst) {
        exclude_from_capture(&window, true);
    }
    Ok(())
}

/// The "Yayın" window went (closed by the player, or by the launcher): watching moves back.
pub fn popout_destroyed(app: &AppHandle) {
    if let Some(parts) = PARTS.get() {
        lock(&parts.channels)[slot(ShareSurface::Popout)] = None;
    }
    if let Some(state) = app.try_state::<crate::state::AppState>() {
        state.voice.share_detach(ShareSurface::Popout);
    }
}

/// "Her zaman üstte" on the "Yayın" window (kept for the next one).
pub fn set_on_top(app: &AppHandle, on: bool) {
    if let Some(window) = app.get_webview_window(SHARE_WINDOW) {
        let _ = window.set_always_on_top(on);
    }
}

/// Fullscreen for the window that watches (the stage fills it; Esc or the button leaves).
pub fn set_fullscreen(app: &AppHandle, label: &str, on: bool) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.set_fullscreen(on);
    }
}
