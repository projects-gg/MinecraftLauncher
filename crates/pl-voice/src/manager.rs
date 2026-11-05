//! Voice chat for the launcher ([`Voice`]): the [`Machine`] with its I/O. The friends service
//! (calls, rings, crew room tickets, the player's switch) is a [`Service`]; the app's own parts
//! (the voice window and its page, Raw Input, the ring, notifications, the page's events) are a
//! [`Platform`]; the room socket is [`crate::socket`]'s. One timer task sleeps until the
//! machine's next deadline and nothing else wakes: without a call there is none.
//!
//! Every step's effects are queued under the machine's lock and carried out by one thread
//! (`voice-effects`) in that order, so two steps on different threads (a command and a socket
//! message) can never apply their effects the other way round (a stale "watch keys" after a
//! leave, an older state over a newer one). The thread waits on its queue: no wake-ups.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::{Notify, mpsc, oneshot};
use tokio::time::Instant;

use crate::discord::VoiceDiscordOffer;
use crate::engine::{EngineCommand, PageReport};
use crate::input::KeyChange;
use crate::lifecycle;
use crate::machine::{
    CrewRoom, Effect, IncomingRing, JOIN_RID, Machine, RingEnd, ServiceVoice, StayStart,
    SystemNotice,
};
use crate::settings::{Store, VoiceBackup, VoiceKey, VoiceSettings};
use crate::socket::{self, SocketEvent, SocketTarget};
use crate::view::{
    VoiceAnswer, VoiceCrewOp, VoiceKeyAsk, VoiceKeyCheck, VoiceLevel, VoiceLevels, VoiceModAction,
    VoiceNotice, VoiceRefusal, VoiceShortcut, VoiceSpeaking, VoiceState,
};
use crate::wire::{self, Ticket, ToRoom};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// SHARE: screen sharing's commands (its own file; a child, so it shares the machine's steps).
#[path = "share_manager.rs"]
mod share_manager;

/// A room the friends service gave a ticket to (`POST /v1/voice/crew`, `…/crew/new`).
#[derive(Debug, Clone)]
pub struct CrewTicket {
    pub room: String,
    pub name: String,
    pub ticket: Ticket,
}

/// The friends service as voice chat uses it.
pub trait Service: Send + Sync + 'static {
    /// `voice_call`: the new friend room and the caller's ticket, or the refusal.
    fn call(&self, friend: String) -> BoxFuture<'_, Result<(String, Ticket), VoiceAnswer>>;
    /// `voice_decline`.
    fn decline(&self, room: String);
    /// `status.voice`: the player's "Sesli sohbet" switch.
    fn switch(&self, on: bool);
    /// A ticket to one of the crew's live rooms (`room`), to its "Genel" (none), or to a new room
    /// (`new`).
    fn crew_ticket(
        &self,
        crew: String,
        room: Option<String>,
        new: bool,
    ) -> BoxFuture<'_, Result<CrewTicket, VoiceAnswer>>;
    /// Where room sockets go and with which session (none while signed out).
    fn socket_target(&self) -> Option<SocketTarget>;
    /// VOICE-ROOMS: a change of a crew's rooms from the crew page (the service checks the role,
    /// the switch and the crew's day).
    fn crew_change(&self, op: VoiceCrewOp) -> BoxFuture<'_, Result<(), VoiceAnswer>>;
}

/// The launcher's own parts.
pub trait Platform: Send + Sync + 'static {
    /// Open, close, or open again (after a crash) the hidden voice window.
    fn window(&self, action: lifecycle::Action);
    /// To the attached voice page.
    fn engine(&self, command: EngineCommand);
    /// Raw Input: watch these shortcuts, or stop.
    fn input(&self, keys: Option<Vec<(VoiceShortcut, VoiceKey)>>);
    /// The ring sound.
    fn ring(&self, on: bool);
    /// A Windows notification (the platform decides by whether its window is in sight).
    fn system(&self, notice: SystemNotice);
    /// The page's events.
    fn state(&self, state: VoiceState);
    fn notice(&self, notice: VoiceNotice);
    fn speaking(&self, speaking: VoiceSpeaking);
    fn level(&self, level: VoiceLevel);
    /// Milliseconds since the computer was last touched (keyboard, mouse, controller).
    fn idle_ms(&self) -> u64;
    /// The live meters' levels (VOICE-CALL), while the page asks for them; at most sixteen a
    /// second. Nothing by default.
    fn levels(&self, _levels: VoiceLevels) {}
    // SHARE (nothing by default) ---------------------------------------------------------------
    /// To the publisher (`pl-share`).
    fn share(&self, _cmd: crate::share::ShareCmd) {}
    /// To the page that watches on `surface`.
    fn watch(&self, _surface: crate::share::ShareSurface, _cmd: crate::share::WatchCmd) {}
    /// A toast for the page (`share-notice`).
    fn share_notice(&self, _notice: crate::share::ShareNotice) {}
    /// While the player shares: the launcher's windows out of the picture, its notifications quiet.
    fn share_privacy(&self, _hide: bool, _quiet: bool) {}
    /// Open (or bring to the front) or close the "Yayın" window.
    fn share_popout(&self, _open: bool) {}
}

/// How long an action waits for the room's answer.
const ANSWER_WAIT: Duration = Duration::from_secs(10);
/// How long a join waits for its socket before it answers "on the way".
const JOIN_WAIT: Duration = Duration::from_secs(15);

struct Inner {
    machine: Mutex<Machine>,
    service: Arc<dyn Service>,
    platform: Arc<dyn Platform>,
    runtime: tokio::runtime::Handle,
    epoch: Instant,
    timer: Notify,
    sockets: Mutex<HashMap<u64, mpsc::UnboundedSender<ToRoom>>>,
    waiters: Mutex<HashMap<u32, oneshot::Sender<VoiceAnswer>>>,
    /// Effects in the machine's order, for the `voice-effects` thread.
    queue: Mutex<std::sync::mpsc::Sender<Batch>>,
    /// Discord's keys as offered, until the player answers (VOICE-KEYS).
    discord: Mutex<Option<VoiceDiscordOffer>>,
}

/// One step's effects, and the state to show after them.
struct Batch {
    effects: Vec<Effect>,
    view: Option<VoiceState>,
}

/// Voice chat (cheap to clone).
#[derive(Clone)]
pub struct Voice {
    inner: Arc<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default()
}

impl Voice {
    pub fn new(
        store: Store,
        service: Arc<dyn Service>,
        platform: Arc<dyn Platform>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        let (queue, batches) = std::sync::mpsc::channel::<Batch>();
        let voice = Self {
            inner: Arc::new(Inner {
                machine: Mutex::new(Machine::new(store)),
                service,
                platform,
                runtime,
                epoch: Instant::now(),
                timer: Notify::new(),
                sockets: Mutex::new(HashMap::new()),
                waiters: Mutex::new(HashMap::new()),
                queue: Mutex::new(queue),
                discord: Mutex::new(None),
            }),
        };
        let executor = voice.clone();
        let spawned = std::thread::Builder::new()
            .name("voice-effects".to_owned())
            .spawn(move || {
                for batch in batches {
                    executor.run(batch);
                }
            });
        if let Err(error) = spawned {
            tracing::error!(%error, "voice: no thread for its effects");
        }
        voice.start_timer();
        voice
    }

    fn now(&self) -> u64 {
        self.inner.epoch.elapsed().as_millis() as u64
    }

    fn last_input(&self, now: u64) -> u64 {
        now.saturating_sub(self.inner.platform.idle_ms())
    }

    /// Runs `step` on the machine and queues its effects in the machine's order (they are carried
    /// out on the `voice-effects` thread); the timer looks at the deadline again.
    fn step<R>(&self, step: impl FnOnce(&mut Machine, u64, i64) -> (R, Vec<Effect>)) -> R {
        let now = self.now();
        let unix = unix_now();
        let result = {
            let mut machine = lock(&self.inner.machine);
            let (result, effects) = step(&mut machine, now, unix);
            let view = effects
                .iter()
                .any(|effect| matches!(effect, Effect::Changed))
                .then(|| machine.view(now, unix));
            if !effects.is_empty() || view.is_some() {
                let _ = lock(&self.inner.queue).send(Batch { effects, view });
            }
            result
        };
        self.inner.timer.notify_one();
        result
    }

    fn effects(&self, step: impl FnOnce(&mut Machine, u64, i64) -> Vec<Effect>) {
        self.step(|machine, now, unix| ((), step(machine, now, unix)));
    }

    /// Carries one step's effects out (the `voice-effects` thread only).
    fn run(&self, batch: Batch) {
        let Batch { effects, view } = batch;
        let platform = &self.inner.platform;
        for effect in effects {
            match effect {
                Effect::HubDecline { room } => self.inner.service.decline(room),
                Effect::HubSwitch { on } => self.inner.service.switch(on),
                Effect::Connect { generation, ticket } => self.connect(generation, ticket),
                Effect::Send { generation, frame } => {
                    if let Some(socket) = lock(&self.inner.sockets).get(&generation) {
                        let _ = socket.send(frame);
                    }
                }
                Effect::Disconnect { generation } => {
                    lock(&self.inner.sockets).remove(&generation);
                }
                Effect::Window(action) => platform.window(action),
                Effect::Engine(command) => platform.engine(command),
                Effect::Input(keys) => platform.input(keys),
                Effect::Ring(on) => platform.ring(on),
                Effect::System(notice) => platform.system(notice),
                Effect::Notice(notice) => platform.notice(notice),
                Effect::Speaking(speaking) => platform.speaking(speaking),
                Effect::Level(level) => platform.level(level),
                Effect::Answer { rid, answer } => {
                    if let Some(waiter) = lock(&self.inner.waiters).remove(&rid) {
                        let _ = waiter.send(answer);
                    }
                }
                Effect::Changed => {}
                Effect::Share(cmd) => platform.share(cmd),
                Effect::Watch { surface, cmd } => platform.watch(surface, cmd),
                Effect::ShareNotice(notice) => platform.share_notice(notice),
                Effect::SharePrivacy { hide, quiet } => platform.share_privacy(hide, quiet),
                Effect::SharePopout(open) => platform.share_popout(open),
            }
        }
        if let Some(view) = view {
            platform.state(view);
        }
    }

    fn connect(&self, generation: u64, ticket: Ticket) {
        let Some(target) = self.inner.service.socket_target() else {
            self.effects(|machine, now, _| {
                machine.socket_refused(generation, Some(401), Some("unauthorized"), now)
            });
            return;
        };
        let voice = self.clone();
        let events: socket::EventSink = Arc::new(move |event| voice.on_socket(generation, event));
        let sender = socket::spawn(&self.inner.runtime, target, ticket, events);
        lock(&self.inner.sockets).insert(generation, sender);
    }

    fn on_socket(&self, generation: u64, event: SocketEvent) {
        match event {
            SocketEvent::Open => {
                self.effects(|machine, now, _| machine.socket_open(generation, now))
            }
            SocketEvent::Message(message) => self.effects(|machine, now, unix| {
                machine.socket_message(generation, *message, now, unix)
            }),
            SocketEvent::Closed(code) => {
                lock(&self.inner.sockets).remove(&generation);
                self.effects(|machine, now, _| machine.socket_closed(generation, code, now));
            }
            SocketEvent::Refused { status, code } => {
                lock(&self.inner.sockets).remove(&generation);
                self.effects(|machine, now, _| {
                    machine.socket_refused(generation, status, code.as_deref(), now)
                });
            }
        }
    }

    /// The one timer: it sleeps until the machine's next deadline (or a change), and without a
    /// deadline it only waits for a change.
    fn start_timer(&self) {
        let voice = self.clone();
        self.inner.runtime.spawn(async move {
            loop {
                let notified = voice.inner.timer.notified();
                let now = voice.now();
                let deadline = lock(&voice.inner.machine).next_deadline(voice.last_input(now));
                match deadline {
                    Some(at) => {
                        let wake = voice.inner.epoch + Duration::from_millis(at.max(now));
                        tokio::select! {
                            () = notified => {}
                            () = tokio::time::sleep_until(wake) => {
                                let platform = voice.inner.platform.clone();
                                voice.effects(|machine, now, unix| {
                                    let last_input = now.saturating_sub(platform.idle_ms());
                                    machine.tick(now, unix, last_input)
                                });
                            }
                        }
                    }
                    None => notified.await,
                }
            }
        });
    }

    async fn wait(
        &self,
        rid: u32,
        receiver: oneshot::Receiver<VoiceAnswer>,
        wait: Duration,
    ) -> VoiceAnswer {
        match tokio::time::timeout(wait, receiver).await {
            Ok(Ok(answer)) => answer,
            _ => {
                lock(&self.inner.waiters).remove(&rid);
                lock(&self.inner.machine).forget(rid);
                VoiceAnswer::refused(VoiceRefusal::Failed)
            }
        }
    }

    // -------------------------------------------------------------------------------------------
    // The friends service's side

    /// The signed config's `features.voice` (with `friends`).
    pub fn set_flag(&self, on: bool) {
        self.effects(|machine, now, _| machine.set_flag(on, now));
    }

    /// The signed-in player's id, or empty when signed out.
    pub fn set_me(&self, me: &str) {
        self.effects(|machine, now, _| machine.set_me(me, now));
    }

    pub fn hub_hello(
        &self,
        taken: bool,
        service: Option<ServiceVoice>,
        rooms: Vec<(String, Vec<CrewRoom>)>,
    ) {
        self.effects(|machine, _, _| machine.hub_hello(taken, service, rooms));
    }

    pub fn hub_settings(&self, service: ServiceVoice) {
        self.effects(|machine, _, _| machine.hub_settings(service));
    }

    pub fn hub_down(&self) {
        self.effects(|machine, _, _| machine.hub_down());
    }

    pub fn ring(&self, ring: IncomingRing, quiet: bool) {
        self.effects(|machine, now, unix| machine.ring(ring, quiet, now, unix));
    }

    pub fn ring_end(&self, room: &str, reason: RingEnd) {
        self.effects(|machine, _, _| machine.ring_end(room, reason));
    }

    pub fn crew_rooms(&self, crew: &str, rooms: Vec<CrewRoom>) {
        self.effects(|machine, _, _| machine.crew_rooms(crew, rooms));
    }

    /// The player blocked someone (a two-person call with them ends).
    pub fn blocked(&self, user: &str) {
        self.effects(|machine, now, _| machine.blocked(user, now));
    }

    // -------------------------------------------------------------------------------------------
    // The page's commands

    pub fn state(&self) -> VoiceState {
        lock(&self.inner.machine).view(self.now(), unix_now())
    }

    pub fn in_room(&self) -> bool {
        lock(&self.inner.machine).in_room()
    }

    /// The room's name for the tray's tooltip and the room id, while in one.
    pub fn room_id(&self) -> Option<String> {
        lock(&self.inner.machine).room_id().map(str::to_owned)
    }

    async fn join(&self, start: StayStart) -> VoiceAnswer {
        let (sender, receiver) = oneshot::channel();
        if let Some(old) = lock(&self.inner.waiters).insert(JOIN_RID, sender) {
            let _ = old.send(VoiceAnswer::refused(VoiceRefusal::Nothing));
        }
        self.effects(|machine, now, unix| machine.join(start, now, unix));
        match tokio::time::timeout(JOIN_WAIT, receiver).await {
            Ok(Ok(answer)) => answer,
            // Still connecting: the room bar says so.
            _ => VoiceAnswer::OK,
        }
    }

    /// "Ara".
    pub async fn call(&self, friend: String) -> VoiceAnswer {
        if let Err(answer) = lock(&self.inner.machine).may_call() {
            return answer;
        }
        match self.inner.service.call(friend.clone()).await {
            Ok((room, ticket)) => {
                if !wire::is_room_id(&room) {
                    return VoiceAnswer::refused(VoiceRefusal::Failed);
                }
                self.join(StayStart {
                    room,
                    ticket,
                    calling: Some(friend),
                    opened_by_me: false,
                })
                .await
            }
            Err(answer) => answer,
        }
    }

    /// "Katıl" on a ring or a missed call.
    pub async fn accept(&self, room: String) -> VoiceAnswer {
        let ticket = lock(&self.inner.machine).ticket_for(&room, unix_now());
        let Some(ticket) = ticket else {
            return VoiceAnswer::refused(VoiceRefusal::RoomGone);
        };
        self.join(StayStart {
            room,
            ticket,
            calling: None,
            opened_by_me: false,
        })
        .await
    }

    /// "Reddet".
    pub fn decline(&self, room: &str) {
        self.effects(|machine, _, _| machine.decline(room));
    }

    pub fn dismiss_missed(&self, room: &str) {
        self.effects(|machine, _, _| machine.dismiss_missed(room));
    }

    /// "Katıl" on a crew room ("Genel" without `room`), or "Yeni oda" (`new`).
    pub async fn join_crew(&self, crew: String, room: Option<String>, new: bool) -> VoiceAnswer {
        if let Err(answer) = lock(&self.inner.machine).may_call() {
            return answer;
        }
        match self.inner.service.crew_ticket(crew, room, new).await {
            Ok(ticket) => {
                if !wire::is_room_id(&ticket.room) {
                    return VoiceAnswer::refused(VoiceRefusal::Failed);
                }
                self.join(StayStart {
                    room: ticket.room,
                    ticket: ticket.ticket,
                    calling: None,
                    opened_by_me: new,
                })
                .await
            }
            Err(answer) => answer,
        }
    }

    /// VOICE-ROOMS: the crew page's room change ("Odayı düzenle", the rooms' order, "Odayı sil", a
    /// drag that moves a crewmate). What cannot be right is refused here; the role, the switch and
    /// the crew's day are the service's. The crew's new list comes as its `voice_rooms`.
    pub async fn crew_change(&self, op: VoiceCrewOp) -> VoiceAnswer {
        if let Err(answer) = lock(&self.inner.machine).may_manage() {
            return answer;
        }
        if let Some(refusal) = crew_op_refusal(&op) {
            return VoiceAnswer::refused(refusal);
        }
        match self.inner.service.crew_change(op).await {
            Ok(()) => VoiceAnswer::OK,
            Err(answer) => answer,
        }
    }

    /// "Tekrar dene" after "Bağlanamadı.".
    pub async fn retry(&self) -> VoiceAnswer {
        let retry = lock(&self.inner.machine).retry_ticket();
        let Some((room, crew, ticket)) = retry else {
            return VoiceAnswer::refused(VoiceRefusal::Nothing);
        };
        match crew {
            Some(crew) => self.join_crew(crew, Some(room), false).await,
            None => {
                self.join(StayStart {
                    room,
                    ticket,
                    calling: None,
                    opened_by_me: false,
                })
                .await
            }
        }
    }

    /// "Ayrıl" (and "İptal" while calling).
    pub fn leave(&self) {
        self.effects(|machine, now, _| machine.leave(now));
    }

    pub fn set_mute(&self, on: bool) {
        self.effects(|machine, _, _| machine.set_mute(on));
    }

    pub fn set_deaf(&self, on: bool) {
        self.effects(|machine, _, _| machine.set_deaf(on));
    }

    pub fn set_volume(&self, person: &str, percent: u16) {
        self.effects(|machine, _, _| machine.set_volume(person, percent));
    }

    pub fn set_muted_for_me(&self, person: &str, on: bool) {
        self.effects(|machine, _, _| machine.set_muted_for_me(person, on));
    }

    async fn request(
        &self,
        make: impl FnOnce(&mut Machine) -> Result<(u32, Vec<Effect>), VoiceAnswer>,
    ) -> VoiceAnswer {
        let (sender, receiver) = oneshot::channel();
        let rid = self.step(|machine, _, _| match make(machine) {
            Ok((rid, effects)) => {
                lock(&self.inner.waiters).insert(rid, sender);
                (Ok(rid), effects)
            }
            Err(answer) => (Err(answer), Vec::new()),
        });
        match rid {
            Ok(rid) => self.wait(rid, receiver, ANSWER_WAIT).await,
            Err(answer) => answer,
        }
    }

    /// "Davet et".
    pub async fn invite(&self, friend: String) -> VoiceAnswer {
        self.request(|machine| machine.invite(&friend)).await
    }

    /// "İptal" on the player's ringing invite.
    pub async fn cancel_invite(&self, friend: String) -> VoiceAnswer {
        self.request(|machine| machine.cancel_invite(&friend)).await
    }

    /// A leader's or moderator's action.
    pub async fn moderate(&self, action: VoiceModAction) -> VoiceAnswer {
        self.request(|machine| machine.moderate(&action)).await
    }

    pub fn settings(&self) -> VoiceSettings {
        lock(&self.inner.machine).settings().clone()
    }

    pub fn save_settings(&self, settings: VoiceSettings) -> VoiceSettings {
        self.step(|machine, now, _| machine.save_settings(settings, now))
    }

    pub fn backup(&self) -> VoiceBackup {
        lock(&self.inner.machine).store().backup()
    }

    pub fn restore(&self, backup: VoiceBackup) {
        self.effects(|machine, now, _| machine.restore(backup, now));
    }

    pub fn check_key(&self, ask: &VoiceKeyAsk) -> VoiceKeyCheck {
        let machine = lock(&self.inner.machine);
        crate::keys::check(&ask.key, ask.shortcut, machine.settings())
    }

    /// Discord's keys to offer, once per computer (VOICE-KEYS): None when the player answered
    /// before, or when Discord had nothing to copy (then its files are never read again). `read`
    /// reads Discord's files: the caller runs this off the main thread.
    pub fn discord_offer(
        &self,
        read: impl FnOnce() -> Option<VoiceDiscordOffer>,
    ) -> Option<VoiceDiscordOffer> {
        if lock(&self.inner.machine).store().discord_asked() {
            return None;
        }
        if let Some(offer) = lock(&self.inner.discord).clone() {
            return Some(offer);
        }
        let offer = read();
        match &offer {
            Some(offer) => *lock(&self.inner.discord) = Some(offer.clone()),
            None => lock(&self.inner.machine).store_mut().set_discord_asked(),
        }
        offer
    }

    /// The player's answer to Discord's keys: "Kopyala" fills the shortcuts Discord has (and
    /// "Bas-konuş" when Discord talks by push-to-talk), "Hayır" leaves everything; either way it is
    /// never asked again. Returns the settings as kept.
    pub fn discord_answer(&self, copy: bool) -> VoiceSettings {
        let offer = lock(&self.inner.discord).take();
        lock(&self.inner.machine).store_mut().set_discord_asked();
        let Some(offer) = offer.filter(|_| copy) else {
            return self.settings();
        };
        let mut settings = self.settings();
        if let Some(key) = offer.push_to_talk {
            settings.push_to_talk_key = Some(key);
            if offer.push_to_talk_mode {
                settings.mode = crate::settings::VoiceTalkMode::PushToTalk;
            }
        }
        if let Some(key) = offer.mute {
            settings.mute_key = Some(key);
        }
        if let Some(key) = offer.deafen {
            settings.deafen_key = Some(key);
        }
        self.save_settings(settings)
    }

    /// The settings page is open (`on`) or closed.
    pub fn set_setup(&self, on: bool) {
        self.effects(|machine, now, _| machine.set_setup(on, now));
    }

    /// "Mikrofonu dene".
    pub fn set_test(&self, on: bool) {
        self.effects(|machine, now, _| machine.set_test(on, now));
    }

    /// The page shows live meters (VOICE-CALL: its window on screen with a room's pill or panel),
    /// or not.
    pub fn set_meters(&self, on: bool) {
        self.effects(|machine, _, _| machine.set_meters(on));
    }

    /// SOC-NEXT §3.7 (VOICE-CALL): whom the player ignores on this computer; their rings stay
    /// quiet (the page's list, sent whenever it changes).
    pub fn set_ignored(&self, ids: Vec<String>) {
        self.effects(|machine, _, _| machine.set_ignored(ids));
    }

    // -------------------------------------------------------------------------------------------
    // The voice window's side

    pub fn page_attached(&self) {
        self.effects(|machine, _, _| machine.page_attached());
    }

    pub fn page_report(&self, report: PageReport) {
        if !matches!(report, PageReport::Levels { .. }) {
            self.effects(|machine, now, _| machine.page_report(report, now));
            return;
        }
        // The live meters' levels (VOICE-CALL, up to sixteen a second) change nothing the machine
        // keeps: straight to the page's event, past the effects queue and the timer.
        if report.is_oversized() {
            return;
        }
        let PageReport::Levels {
            me,
            open,
            ids,
            levels,
        } = report
        else {
            return;
        };
        let shown = lock(&self.inner.machine).levels_of(me, open, ids, levels);
        if let Some(levels) = shown {
            self.inner.platform.levels(levels);
        }
    }

    pub fn page_gone(&self) {
        lock(&self.inner.machine).page_gone();
    }

    pub fn page_crashed(&self) {
        self.effects(|machine, now, _| machine.page_crashed(now));
    }

    /// A shortcut's key changed (Raw Input's thread).
    pub fn key(&self, change: KeyChange) {
        self.effects(|machine, _, _| machine.key(change));
    }
}

/// VOICE-ROOMS: a crew page change that cannot be right as asked (ids that are not ids, "Genel"
/// where it never changes, a name outside 3–24 characters, a limit outside 2–25), or none.
fn crew_op_refusal(op: &VoiceCrewOp) -> Option<VoiceRefusal> {
    let crew_ok = |crew: &str| crew.len() == 16 && crew.bytes().all(|b| b.is_ascii_hexdigit());
    let kept = |room: &str| wire::is_room_id(room) && room.starts_with('c');
    let crew_room = |room: &str| wire::is_room_id(room) && !room.starts_with('f');
    match op {
        VoiceCrewOp::Edit {
            crew,
            room,
            name,
            max,
            ..
        } => {
            if !crew_ok(crew) || !kept(room) {
                return Some(VoiceRefusal::NotAllowed);
            }
            if !(3..=24).contains(&name.trim().chars().count()) {
                return Some(VoiceRefusal::InvalidName);
            }
            max.is_some_and(|max| !(2..=25).contains(&max))
                .then_some(VoiceRefusal::NotAllowed)
        }
        VoiceCrewOp::Order { crew, rooms } => {
            (!crew_ok(crew) || rooms.len() > 10 || !rooms.iter().all(|room| kept(room)))
                .then_some(VoiceRefusal::NotAllowed)
        }
        VoiceCrewOp::Remove { crew, room, .. } => {
            (!crew_ok(crew) || !kept(room)).then_some(VoiceRefusal::NotAllowed)
        }
        VoiceCrewOp::Move { crew, person, room } => (!crew_ok(crew)
            || person.is_empty()
            || !person.bytes().all(|b| b.is_ascii_digit())
            || !crew_room(room))
        .then_some(VoiceRefusal::NotAllowed),
    }
}

#[cfg(test)]
mod crew_op_tests {
    use super::*;

    const CREW: &str = "0123456789abcdef";
    const ROOM: &str = "c0123456789abcdef";

    #[test]
    fn refuses_crew_page_changes_that_cannot_be_right() {
        let edit = |room: &str, name: &str, max: Option<u16>| VoiceCrewOp::Edit {
            crew: CREW.into(),
            room: room.into(),
            name: name.into(),
            max,
            share_by: None,
        };
        assert_eq!(crew_op_refusal(&edit(ROOM, "Antrenman", Some(4))), None);
        assert_eq!(crew_op_refusal(&edit(ROOM, "  Taş Ocağı ", None)), None);
        assert_eq!(
            crew_op_refusal(&edit(ROOM, "ab", None)),
            Some(VoiceRefusal::InvalidName)
        );
        assert_eq!(
            crew_op_refusal(&edit(ROOM, "Bir oda adı yirmi dört harften uzun", None)),
            Some(VoiceRefusal::InvalidName)
        );
        assert_eq!(
            crew_op_refusal(&edit(ROOM, "Antrenman", Some(1))),
            Some(VoiceRefusal::NotAllowed)
        );
        // "Genel" never changes; friend rooms are no crew's.
        assert_eq!(
            crew_op_refusal(&edit("g0123456789abcdef", "Lobi", None)),
            Some(VoiceRefusal::NotAllowed)
        );
        assert_eq!(
            crew_op_refusal(&VoiceCrewOp::Remove {
                crew: CREW.into(),
                room: "f0123456789abcdef".into(),
                empty: false
            }),
            Some(VoiceRefusal::NotAllowed)
        );
        assert_eq!(
            crew_op_refusal(&VoiceCrewOp::Order {
                crew: CREW.into(),
                rooms: vec![ROOM.into()]
            }),
            None
        );
        // A crewmate can be moved to "Genel" too.
        let move_to = |person: &str, room: &str| VoiceCrewOp::Move {
            crew: CREW.into(),
            person: person.into(),
            room: room.into(),
        };
        assert_eq!(
            crew_op_refusal(&move_to("123456789012345678", "g0123456789abcdef")),
            None
        );
        assert_eq!(
            crew_op_refusal(&move_to("someone", ROOM)),
            Some(VoiceRefusal::NotAllowed)
        );
    }
}
