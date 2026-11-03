//! "Sesli sohbet" in the app (VOICE, design in the task note `voice.md`): `pl_voice::Voice`
//! wired to the friends service (rings, calls, crew room tickets on the presence socket and its
//! HTTP routes), the hidden voice window ([`window`]), Raw Input for push-to-talk and the
//! shortcuts ([`input`]), Windows notifications and the page's events. The logic and its tests
//! live in `pl-voice`; this module only implements its traits with the app's parts.
//!
//! The feature shows only while the signed config's `features.voice` (with `friends`) is on and
//! the service took `voice1` with Studio's switch on (`VoiceState.available`). Debug builds may
//! force the signed side with `PROJECTS_SOCIAL_VOICE=1` for a local `wrangler dev`.
//!
//! Without a call nothing runs: no window, no socket, no input registration, no timer.
//!
//! The in-game overlay (VOICE-O) hears every state, speaking change and notice too (`pl_overlay`
//! keeps the newest for games that start later and tells only a mod that shows voice chat); the
//! game's presses come back to the same `Voice` methods as the page's commands ([`wire_overlay`]).
//! While a game's mod shows voice chat, its cards stand in for the Windows notifications of rings,
//! removals and a moderator's mute, as its toasts do for messages.

pub(crate) mod input;
mod probe;
mod window;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use pl_social::{SocialService, VoiceError, VoiceEvent, VoiceFrame};
use pl_voice::engine::EngineCommand;
use pl_voice::lifecycle;
use pl_voice::machine::{CrewRoom, IncomingRing, RingEnd, ServiceVoice, SystemNotice};
use pl_voice::manager::BoxFuture;
use pl_voice::settings::{Store, VoiceKey};
use pl_voice::socket::SocketTarget;
use pl_voice::view::{
    VoiceAnswer, VoiceLeftReason, VoiceLevel, VoiceNotice, VoiceRefusal, VoiceShortcut,
    VoiceSpeaking, VoiceState,
};
use pl_voice::wire::Ticket;
use pl_voice::{CrewTicket, Platform, Service, Voice};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};

/// Every change of the state, whole (`VoiceState`).
pub const STATE_EVENT: &str = "voice-state";
/// Moments worth a toast (`VoiceNotice`).
pub const NOTICE_EVENT: &str = "voice-notice";
/// Who speaks now (`VoiceSpeaking`), only while the main window is on screen.
pub const SPEAKING_EVENT: &str = "voice-speaking";
/// The microphone's level for the settings page (`VoiceLevel`).
pub const LEVEL_EVENT: &str = "voice-level";

/// How long the hub may take to answer a call.
const CALL_WAIT: Duration = Duration::from_secs(10);

/// VOICE-0's probe (debug builds asked for it with `PROJECTS_VOICE_PROBE`).
pub fn install(app: &tauri::App) {
    probe::install(app);
}

/// Voice chat for the app: its parts wired, the page and the in-game overlay told every change.
pub fn create(
    app: &AppHandle,
    root: &Path,
    social: SocialService,
    runtime: tokio::runtime::Handle,
    overlay: pl_overlay::Overlay,
) -> Voice {
    let platform = app_platform(app);
    let voice = Voice::new(
        Store::open(root),
        Arc::new(SocialLink(social.clone())),
        platform.clone(),
        runtime,
    );
    let _ = platform.voice.set(voice.clone());
    wire_overlay(&overlay, &voice);
    let _ = platform.overlay.set(overlay);
    let hears = voice.clone();
    social.on_voice(move |event| on_hub(&hears, event));
    social.set_voice_switch(voice.settings().enabled);
    if debug_voice() {
        social.set_voice_enabled(true);
        voice.set_flag(true);
    }
    voice
}

/// The signed config's `features.voice` (with `friends`): the presence connection asks for
/// `voice1` and the page may show the feature.
pub fn apply_flag(state: &crate::state::AppState, friends: bool, voice: bool) {
    let on = (friends && voice) || debug_voice();
    state.social.set_voice_enabled(on);
    state.voice.set_flag(on);
}

/// `PROJECTS_SOCIAL_VOICE=1` (debug builds only): voice chat on whatever the signed config says,
/// for a local `wrangler dev`.
fn debug_voice() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var("PROJECTS_SOCIAL_VOICE").is_ok_and(|value| value == "1")
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

/// VOICE-0's probe runs (debug builds): it owns the window label then.
fn probing() -> bool {
    cfg!(debug_assertions)
        && std::env::var_os("PROJECTS_VOICE_PROBE").is_some_and(|v| !v.is_empty())
}

/// The friends service's every update: who the player is, and the names notifications show.
pub fn on_social(app: &AppHandle, update: &pl_social::SocialUpdate) {
    let pl_social::SocialUpdate::View(view) = update else {
        return;
    };
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let me = view.me.as_ref().map(|me| me.id.clone()).unwrap_or_default();
    let mut names: HashMap<String, String> = view
        .friends
        .iter()
        .map(|friend| (friend.id.clone(), friend.name.clone()))
        .collect();
    for crew in &view.crews {
        for member in &crew.members {
            names
                .entry(member.id.clone())
                .or_insert_with(|| member.name.clone());
        }
    }
    let Some(platform) = PLATFORM.get() else {
        return;
    };
    let changed = {
        let mut known = lock(&platform.me);
        let changed = *known != me;
        known.clone_from(&me);
        changed
    };
    *lock(&platform.names) = names;
    if changed {
        state.voice.set_me(&me);
    }
}

/// The in-game overlay's presses (VOICE-O) go to the same `Voice` methods as the page's commands.
/// The ones that act at once are done before the call returns, so two quick presses (the
/// microphone off and on again) keep their order.
fn wire_overlay(overlay: &pl_overlay::Overlay, voice: &Voice) {
    use pl_overlay::{VoiceAct, VoiceFuture};
    let voice = voice.clone();
    overlay.on_voice(move |act| -> VoiceFuture {
        let done = || -> VoiceFuture { Box::pin(std::future::ready(VoiceAnswer::OK)) };
        let voice = voice.clone();
        match act {
            VoiceAct::Mute(on) => {
                voice.set_mute(on);
                done()
            }
            VoiceAct::Deaf(on) => {
                voice.set_deaf(on);
                done()
            }
            VoiceAct::Leave => {
                voice.leave();
                done()
            }
            VoiceAct::Decline { room } => {
                voice.decline(&room);
                done()
            }
            VoiceAct::Volume { person, percent } => {
                voice.set_volume(&person, percent);
                done()
            }
            VoiceAct::MuteForMe { person, on } => {
                voice.set_muted_for_me(&person, on);
                done()
            }
            VoiceAct::Accept { room } => Box::pin(async move { voice.accept(room).await }),
            VoiceAct::Moderate(action) => Box::pin(async move { voice.moderate(action).await }),
            VoiceAct::Call { friend } => Box::pin(async move { voice.call(friend).await }),
            VoiceAct::Invite { friend } => Box::pin(async move { voice.invite(friend).await }),
            // OVL-SOC: crew chat's "Sesli" in the game ("Katıl" on a crew room, "Yeni oda").
            VoiceAct::JoinCrew { crew, room, new } => {
                Box::pin(async move { voice.join_crew(crew, room, new).await })
            }
        }
    });
}

/// The main page never gets the microphone or the camera (call once, in setup).
pub fn guard_main(app: &tauri::App) {
    window::guard_main(app);
}

/// The voice window was destroyed (`WindowEvent::Destroyed`).
pub fn window_destroyed(app: &AppHandle) {
    window::destroyed(app);
}

/// The voice page attached its channel.
pub fn attach(voice: &Voice, channel: Channel<EngineCommand>) {
    if let Some(platform) = PLATFORM.get() {
        *lock(&platform.channel) = Some(channel);
    }
    voice.page_attached();
}

/// "Ayarları aç": Windows' microphone privacy page.
pub fn open_mic_settings() {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer.exe")
            .arg("ms-settings:privacy-microphone")
            .spawn();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The hub's voice messages, as the machine takes them.
fn on_hub(voice: &Voice, event: VoiceEvent) {
    match event {
        VoiceEvent::Hello {
            taken,
            status,
            rooms,
        } => voice.hub_hello(
            taken,
            status.map(service_voice),
            rooms.into_iter().map(crew_rooms).collect(),
        ),
        VoiceEvent::Status(status) => voice.hub_settings(service_voice(status)),
        VoiceEvent::Ring { ring, quiet } => voice.ring(
            IncomingRing {
                room: ring.room,
                from: ring.from,
                at: ring.at,
                until: ring.until,
                ticket: Ticket::new(ring.ticket.into_inner()),
                with: ring.with,
            },
            quiet,
        ),
        VoiceEvent::RingEnd { room, reason } => voice.ring_end(
            &room,
            match reason.as_str() {
                "joined" => RingEnd::Joined,
                "declined" => RingEnd::Declined,
                _ => RingEnd::Ended,
            },
        ),
        VoiceEvent::Rooms(rooms) => {
            let (crew, rooms) = crew_rooms(rooms);
            voice.crew_rooms(&crew, rooms);
        }
        VoiceEvent::Down => voice.hub_down(),
    }
}

fn service_voice(status: pl_social::VoiceStatus) -> ServiceVoice {
    ServiceVoice {
        on: status.on,
        friend_max: status.friend_max,
        crew_max: status.crew_max,
        crew_rooms: status.crew_rooms,
        ring_seconds: status.ring_seconds,
        idle_minutes: status.idle_minutes,
        kbps: status.kbps,
    }
}

fn crew_rooms(rooms: pl_social::VoiceCrewRooms) -> (String, Vec<CrewRoom>) {
    (
        rooms.crew,
        rooms
            .rooms
            .into_iter()
            .map(|room| CrewRoom {
                id: room.id,
                name: room.name,
                ids: room.ids,
            })
            .collect(),
    )
}

/// The friends service as voice chat uses it.
struct SocialLink(SocialService);

impl Service for SocialLink {
    fn call(&self, friend: String) -> BoxFuture<'_, Result<(String, Ticket), VoiceAnswer>> {
        Box::pin(async move {
            match self
                .0
                .voice_request(VoiceFrame::Call { to: friend }, CALL_WAIT)
                .await
            {
                Ok(ack) => Ok((ack.room, Ticket::new(ack.ticket.into_inner()))),
                Err(VoiceError::Refused {
                    code,
                    retry_after,
                    until,
                }) => Err(VoiceAnswer::from_code(&code, retry_after, until)),
                Err(VoiceError::Off) => Err(VoiceAnswer::refused(VoiceRefusal::Off)),
                Err(VoiceError::Unavailable) => Err(VoiceAnswer::refused(VoiceRefusal::Failed)),
            }
        })
    }

    fn decline(&self, room: String) {
        self.0.voice_send(VoiceFrame::Decline { room });
    }

    fn switch(&self, on: bool) {
        self.0.set_voice_switch(on);
    }

    fn crew_ticket(
        &self,
        crew: String,
        room: Option<String>,
        new: bool,
    ) -> BoxFuture<'_, Result<CrewTicket, VoiceAnswer>> {
        Box::pin(async move {
            match self.0.voice_crew_ticket(&crew, room.as_deref(), new).await {
                Ok(ticket) => Ok(CrewTicket {
                    room: ticket.room,
                    name: ticket.name,
                    ticket: Ticket::new(ticket.ticket.into_inner()),
                }),
                Err(error) => Err(refusal_of(&error)),
            }
        })
    }

    fn socket_target(&self) -> Option<SocketTarget> {
        let (base, bearer, user_agent) = self.0.voice_socket()?;
        let url = SocketTarget::url_from_base(&base)?;
        let tls = if url.scheme() == "wss" {
            match pl_social::socket_tls() {
                Ok(tls) => Some(tls),
                Err(error) => {
                    tracing::warn!(%error, "voice: no TLS for the room socket");
                    return None;
                }
            }
        } else {
            None
        };
        Some(SocketTarget {
            url,
            bearer,
            user_agent,
            tls,
        })
    }
}

/// A friends service error as a voice refusal.
fn refusal_of(error: &pl_social::SocialError) -> VoiceAnswer {
    use pl_social::SocialError;
    match error {
        SocialError::Refused { code, .. } => VoiceAnswer::from_code(code.as_str(), None, None),
        SocialError::RateLimited { retry_after, .. } => VoiceAnswer {
            refused: Some(VoiceRefusal::RateLimited),
            retry_after: retry_after.map(|wait| wait.as_secs()),
            until: None,
        },
        SocialError::Unauthorized | SocialError::Restricted { .. } => {
            VoiceAnswer::refused(VoiceRefusal::Off)
        }
        _ => VoiceAnswer::refused(VoiceRefusal::Failed),
    }
}

/// The platform of the one app (the voice page attaches to it).
static PLATFORM: OnceLock<Arc<AppPlatform>> = OnceLock::new();

/// The launcher's own parts.
pub struct AppPlatform {
    app: AppHandle,
    voice: OnceLock<Voice>,
    /// The in-game overlay (VOICE-O): it hears every state, speaking change and notice.
    overlay: OnceLock<pl_overlay::Overlay>,
    /// The attached voice page's channel.
    channel: Mutex<Option<Channel<EngineCommand>>>,
    /// The signed-in player's id, as last given to the machine.
    me: Mutex<String>,
    /// Names for notifications: friends and crewmates.
    names: Mutex<HashMap<String, String>>,
    /// Whether the main window was on screen, and when that was looked at.
    in_sight: Mutex<Option<(bool, Instant)>>,
    input: Mutex<Option<input::Watch>>,
    /// What the tray shows of the room (its label, the microphone off).
    tray: Mutex<Option<(String, bool)>>,
}

impl AppPlatform {
    fn new(app: AppHandle) -> Self {
        Self {
            app,
            voice: OnceLock::new(),
            overlay: OnceLock::new(),
            channel: Mutex::new(None),
            me: Mutex::new(String::new()),
            names: Mutex::new(HashMap::new()),
            in_sight: Mutex::new(None),
            input: Mutex::new(None),
            tray: Mutex::new(None),
        }
    }

    /// The main window is on screen (shown, not minimized); looked at once a second at most.
    fn window_in_sight(&self) -> bool {
        let mut cached = lock(&self.in_sight);
        if let Some((seen, at)) = *cached
            && at.elapsed() < Duration::from_secs(1)
        {
            return seen;
        }
        let seen = self.app.get_webview_window("main").is_some_and(|window| {
            window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false)
        });
        *cached = Some((seen, Instant::now()));
        seen
    }

    /// A game's mod shows voice chat now: its cards stand in for this launcher's notifications.
    fn shown_in_game(&self) -> bool {
        self.overlay
            .get()
            .is_some_and(pl_overlay::Overlay::shows_voice)
    }

    fn name_of(&self, id: &str) -> String {
        lock(&self.names)
            .get(id)
            .cloned()
            .unwrap_or_else(|| match pl_lang::Lang::current() {
                pl_lang::Lang::Tr => "Bir arkadaşın".to_owned(),
                pl_lang::Lang::En => "A friend".to_owned(),
            })
    }

    /// Someone in the room, for the tray: a friend's or crewmate's name, else the one their card
    /// gave (a friend's friend, VOICE-S2), else the page's "Bir oyuncu".
    #[cfg(desktop)]
    fn person_name(&self, id: &str, profiles: &[pl_voice::view::VoiceProfile]) -> String {
        if let Some(name) = lock(&self.names).get(id) {
            return name.clone();
        }
        if let Some(profile) = profiles.iter().find(|profile| profile.id == id) {
            return profile.name.clone();
        }
        match pl_lang::Lang::current() {
            pl_lang::Lang::Tr => "Bir oyuncu".to_owned(),
            pl_lang::Lang::En => "A player".to_owned(),
        }
    }

    fn system_notice(&self, notice: SystemNotice) {
        let tr = pl_lang::Lang::current() == pl_lang::Lang::Tr;
        match notice {
            SystemNotice::Ring { room, from, .. } => {
                // In front of the player the page shows its card and rings itself; in a game whose
                // overlay shows voice chat, its card does (and rings while the window is away).
                if self.window_in_sight() || self.shown_in_game() {
                    return;
                }
                let name = self.name_of(&from);
                let title = if tr {
                    format!("{name} arıyor")
                } else {
                    format!("{name} is calling")
                };
                let actions = if tr {
                    [("Katıl", "join"), ("Reddet", "decline")]
                } else {
                    [("Join", "join"), ("Decline", "decline")]
                };
                let app = self.app.clone();
                let ring_room = room.clone();
                crate::notify::voice_show(
                    &room,
                    &title,
                    Some(if tr { "Sesli sohbet" } else { "Voice chat" }),
                    Some(actions),
                    Box::new(move |action| ring_action(&app, &ring_room, &action)),
                );
            }
            SystemNotice::RingGone { room } => crate::notify::voice_clear(&room),
            SystemNotice::Missed { room, from } => {
                if self.window_in_sight() {
                    return;
                }
                let name = self.name_of(&from);
                let app = self.app.clone();
                crate::notify::voice_show(
                    &format!("missed-{room}"),
                    if tr { "Cevapsız arama" } else { "Missed call" },
                    Some(&name),
                    None,
                    Box::new(move |_| focus(&app)),
                );
            }
            SystemNotice::Left { reason } => {
                if self.window_in_sight() || self.shown_in_game() {
                    return;
                }
                let text = match (reason, tr) {
                    (VoiceLeftReason::Kicked, true) => "Odadan çıkarıldın.",
                    (VoiceLeftReason::Kicked, false) => "You were removed from the room.",
                    (VoiceLeftReason::Closed, true) => "Oda kapatıldı.",
                    (VoiceLeftReason::Closed, false) => "The room was closed.",
                    (_, true) => "Sesli sohbet şu an kullanılamıyor.",
                    (_, false) => "Voice chat isn't available right now.",
                };
                let app = self.app.clone();
                crate::notify::voice_show(
                    "left",
                    if tr { "Sesli sohbet" } else { "Voice chat" },
                    Some(text),
                    None,
                    Box::new(move |_| focus(&app)),
                );
            }
            SystemNotice::MutedByModerator => {
                if self.window_in_sight() || self.shown_in_game() {
                    return;
                }
                let app = self.app.clone();
                crate::notify::voice_show(
                    "muted",
                    if tr { "Sesli sohbet" } else { "Voice chat" },
                    Some(if tr {
                        "Bir moderatör seni susturdu."
                    } else {
                        "A moderator muted you."
                    }),
                    None,
                    Box::new(move |_| focus(&app)),
                );
            }
        }
    }
}

/// A press on a ring's notification: "Katıl", "Reddet", or the notification itself.
fn ring_action(app: &AppHandle, room: &str, action: &str) {
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    match action {
        "join" => {
            let voice = state.voice.clone();
            let room = room.to_owned();
            tauri::async_runtime::spawn(async move {
                let answer = voice.accept(room).await;
                if !answer.is_ok() {
                    tracing::debug!(?answer.refused, "voice: the ring's join was refused");
                }
            });
        }
        "decline" => state.voice.decline(room),
        _ => focus(app),
    }
}

fn focus(app: &AppHandle) {
    crate::shell::focus_main_window(app);
    crate::webview_rest::wake(app, "voice notification");
}

impl Platform for AppPlatform {
    fn window(&self, action: lifecycle::Action) {
        if probing() {
            return;
        }
        match action {
            lifecycle::Action::Open | lifecycle::Action::Reopen => {
                *lock(&self.channel) = None;
                // Off the calling thread: a window built inside a synchronous command (the main
                // thread) deadlocks WebView2 on Windows.
                let app = self.app.clone();
                let voice = self.voice.get().cloned();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(error) = window::open(&app, action == lifecycle::Action::Reopen) {
                        tracing::warn!(%error, "voice: the voice window did not open");
                        // As a crash: a few more tries, then "Bağlanamadı.".
                        if let Some(voice) = voice {
                            voice.page_crashed();
                        }
                    }
                });
            }
            lifecycle::Action::Close | lifecycle::Action::GiveUp => {
                *lock(&self.channel) = None;
                if let Some(voice) = self.voice.get() {
                    voice.page_gone();
                }
                window::close(&self.app);
            }
            lifecycle::Action::None => {}
        }
    }

    fn engine(&self, command: EngineCommand) {
        let mut channel = lock(&self.channel);
        if let Some(sender) = channel.as_ref()
            && let Err(error) = sender.send(command)
        {
            tracing::debug!(%error, "voice: the page did not take a command");
            *channel = None;
        }
    }

    fn input(&self, keys: Option<Vec<(VoiceShortcut, VoiceKey)>>) {
        let mut watch = lock(&self.input);
        // The old watch puts tao's registration back before a new one takes it.
        if let Some(old) = watch.take() {
            old.stop();
        }
        if let (Some(keys), Some(voice)) = (keys, self.voice.get()) {
            *watch = input::Watch::start(&keys, voice.clone());
        }
    }

    fn ring(&self, _on: bool) {
        // The ring's notification carries Windows' call sound while the window is away; in
        // front of the player the page rings with its card.
    }

    fn system(&self, notice: SystemNotice) {
        self.system_notice(notice);
    }

    fn state(&self, state: VoiceState) {
        // The in-game overlay (VOICE-O): a ring rings in the game while the window is away (in
        // front, the page rings).
        if let Some(overlay) = self.overlay.get() {
            overlay.set_voice(&state, !self.window_in_sight());
        }
        // The tray's mute and leave items and the room in its tooltip (design §5.4).
        #[cfg(desktop)]
        {
            // A crew's "Genel" and the service's "Oda 2" in the interface's language, as the page
            // says them; a name its leader typed as typed.
            let english = pl_lang::Lang::current() == pl_lang::Lang::En;
            let tray = state.room.as_ref().map(|room| {
                let label = room.shown_name(english).unwrap_or_else(|| {
                    room.people
                        .iter()
                        .map(|person| self.person_name(&person.id, &state.profiles))
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                // Deafened, the microphone is off too ("Mikrofonu aç" opens it and ends deafness).
                (label, room.me.mute || room.me.deaf)
            });
            let changed = {
                let mut last = lock(&self.tray);
                if *last == tray {
                    false
                } else {
                    last.clone_from(&tray);
                    true
                }
            };
            if changed {
                let app = self.app.clone();
                // Waits for the main thread: never on it.
                tauri::async_runtime::spawn_blocking(move || {
                    crate::shell::set_tray_voice(&app, tray)
                });
            }
        }
        let _ = self.app.emit(STATE_EVENT, state);
    }

    fn notice(&self, notice: VoiceNotice) {
        if let Some(overlay) = self.overlay.get() {
            overlay.voice_notice(&notice);
        }
        let _ = self.app.emit(NOTICE_EVENT, notice);
    }

    fn speaking(&self, speaking: VoiceSpeaking) {
        // A game's overlay draws who speaks (only while a game runs; changes only).
        if let Some(overlay) = self.overlay.get() {
            overlay.voice_speaking(&speaking);
        }
        // A resting page would only queue them; it reads the state when it comes back.
        if self.window_in_sight() {
            let _ = self.app.emit(SPEAKING_EVENT, speaking);
        }
    }

    fn level(&self, level: VoiceLevel) {
        let _ = self.app.emit(LEVEL_EVENT, level);
    }

    fn idle_ms(&self) -> u64 {
        input::idle_ms()
    }
}

/// The app's one platform (the voice page's channel attaches to it).
fn app_platform(app: &AppHandle) -> Arc<AppPlatform> {
    PLATFORM
        .get_or_init(|| Arc::new(AppPlatform::new(app.clone())))
        .clone()
}
