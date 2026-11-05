//! Voice chat's state machine on the launcher side (design §3, §5, §6; the S1 contract in
//! [`crate::wire`]). It does no I/O: every input returns [`Effect`]s that the caller carries out
//! (the presence hub, the room socket, the voice window and its page, Raw Input, the ring, the
//! page's state), and [`Machine::next_deadline`] names the one moment it wants a [`Machine::tick`].
//! Without a call there is no deadline at all: nothing wakes.
//!
//! Times: `now` is milliseconds on the caller's monotonic clock, `unix` is Unix seconds.

use std::collections::{BTreeMap, BTreeSet};

use pl_social::{Backoff, BackoffPolicy};

use crate::engine::{
    EngineCommand, EngineIce, EnginePerson, EngineSettings, EngineSound, PageReport,
};
use crate::gain::{self, Playout};
use crate::idle::Idle;
use crate::lifecycle::{self, Lifecycle};
use crate::room::Room;
use crate::settings::{Store, VoiceKey, VoicePersonPrefs, VoiceSettings};
use crate::view::{
    VoiceAnswer, VoiceCalling, VoiceCrewRoom, VoiceCrewRooms, VoiceDevices, VoiceLeftReason,
    VoiceLevel, VoiceLevels, VoiceMe, VoiceMic, VoiceMissed, VoiceModAction, VoiceNotice,
    VoiceOutRing, VoicePerson, VoicePersonLevel, VoicePhase, VoicePowers, VoiceProfile,
    VoiceQuality, VoiceRefusal, VoiceRing, VoiceRole, VoiceRoomKind, VoiceRoomView, VoiceShortcut,
    VoiceSpeaking, VoiceState,
};
use crate::wire::{self, ClosedWhy, FromRoom, ModAct, Role, RoomKind, Sdp, Ticket, ToRoom};

/// SHARE: screen sharing's part of the machine (its own file; a child, so it shares the stay).
#[path = "share_machine.rs"]
mod share_machine;

/// An invite's ticket holds this long after its ring began (`TICKET_TTL.invite`).
pub const INVITE_TICKET_SECONDS: i64 = 600;
/// The connection may be away this long before the stay ends ("Bağlanamadı.").
pub const GIVE_UP_MS: u64 = 60_000;
/// "Yeniden bağlanıyor…" shows after the connection was away this long.
pub const RECONNECTING_SHOWN_MS: u64 = 2_000;
/// After a 4008 close the socket waits this long (the seat holds 120 s).
pub const RATE_LIMITED_WAIT_MS: u64 = 60_000;
/// After an audio connection was lost the page joins anew at once; when that one is lost too
/// (no connection again, TURN refused), the next waits 1, 2, 4, then at most this long: a page
/// that cannot make a connection never spins (VOICE-INT).
pub const REJOIN_MAX_MS: u64 = 8_000;
/// Missed calls kept at most.
pub const MISSED_MAX: usize = 10;
/// SOC-NEXT §3.7 (VOICE-CALL): at most this many ignored people (the page keeps 200), ids this long.
pub const IGNORED_MAX: usize = 200;
const IGNORED_ID_MAX: usize = 64;
/// Studio's defaults until `hello.voice` says otherwise.
const DEFAULT_KBPS: u16 = 24;
const DEFAULT_RING_SECONDS: u32 = 30;
const DEFAULT_CREW_ROOMS: u16 = 3;

/// `hello.voice` / `voice_state`: Studio's switch and what launchers apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceVoice {
    pub on: bool,
    pub friend_max: u16,
    pub crew_max: u16,
    pub crew_rooms: u16,
    pub ring_seconds: u32,
    pub idle_minutes: u32,
    pub kbps: u16,
    /// VOICE-ROOMS: crews arrange their rooms (Studio's "Oda yönetimi").
    pub manage: bool,
}

impl Default for ServiceVoice {
    fn default() -> Self {
        Self {
            on: false,
            friend_max: 10,
            crew_max: 15,
            crew_rooms: DEFAULT_CREW_ROOMS,
            ring_seconds: DEFAULT_RING_SECONDS,
            idle_minutes: crate::idle::DEFAULT_MINUTES,
            kbps: DEFAULT_KBPS,
            manage: false,
        }
    }
}

/// A ring as the hub sends it (`voice_ring.ring`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingRing {
    pub room: String,
    pub from: String,
    /// Unix seconds.
    pub at: i64,
    pub until: i64,
    pub ticket: Ticket,
    pub with: Vec<String>,
}

/// `voice_ring_end.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingEnd {
    /// On another device of the player's.
    Joined,
    Declined,
    /// The room closed, or the caller took it back.
    Ended,
}

/// One crew room as the hub lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrewRoom {
    pub id: String,
    pub name: String,
    pub ids: Vec<String>,
    /// VOICE-ROOMS: people at most, when the crew set fewer than the crew room maximum.
    pub max: Option<u16>,
    /// VOICE-ROOMS: the player opened it.
    pub own: bool,
}

/// How a stay begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StayStart {
    pub room: String,
    pub ticket: Ticket,
    /// The friend the player called (the room rings them; "Aranıyor…").
    pub calling: Option<String>,
    /// A crew room the player opened ("Yeni oda": they may rename it).
    pub opened_by_me: bool,
}

/// A Windows notification the launcher may show (it decides by whether its window is in sight).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemNotice {
    /// A ring with "Katıl" and "Reddet".
    Ring {
        room: String,
        from: String,
        with: Vec<String>,
    },
    /// The ring is over: take its notification away.
    RingGone {
        room: String,
    },
    Missed {
        room: String,
        from: String,
    },
    Left {
        reason: VoiceLeftReason,
    },
    MutedByModerator,
}

/// What the caller does.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// `voice_decline` on the presence socket.
    HubDecline {
        room: String,
    },
    /// `status.voice`: the player's switch.
    HubSwitch {
        on: bool,
    },
    /// Open the room socket (`GET /v1/voice?ticket=…`); its events come back with `generation`.
    Connect {
        generation: u64,
        ticket: Ticket,
    },
    Send {
        generation: u64,
        frame: ToRoom,
    },
    /// Close that socket (after `leave`, or a stay that ended).
    Disconnect {
        generation: u64,
    },
    /// The voice window: open, close, or open again after a crash.
    Window(lifecycle::Action),
    /// To the attached voice page.
    Engine(EngineCommand),
    /// Raw Input: watch these shortcuts, or nothing (None).
    Input(Option<Vec<(VoiceShortcut, VoiceKey)>>),
    /// The ring sound (the launcher's own, also while its page sleeps).
    Ring(bool),
    System(SystemNotice),
    /// A toast for the page.
    Notice(VoiceNotice),
    Speaking(VoiceSpeaking),
    Level(VoiceLevel),
    /// The answer for a request the caller waits on (`rid`), or the first outcome of a join
    /// (`rid` 0).
    Answer {
        rid: u32,
        answer: VoiceAnswer,
    },
    /// The page's state changed: build [`Machine::view`] and send it.
    Changed,
    // SHARE ------------------------------------------------------------------------------------
    /// To the publisher (`pl-share`).
    Share(crate::share::ShareCmd),
    /// To the page that watches (`surface`), through the channel it attached.
    Watch {
        surface: crate::share::ShareSurface,
        cmd: crate::share::WatchCmd,
    },
    /// A toast for the page (`share-notice`).
    ShareNotice(crate::share::ShareNotice),
    /// While the player shares: the launcher's windows out of the picture (`hide`), its Windows
    /// notifications quiet (`quiet`); both off when nothing is shared.
    SharePrivacy {
        hide: bool,
        quiet: bool,
    },
    /// Open (or bring to the front) or close the "Yayın" window.
    SharePopout(bool),
}

/// The first answer a join's caller waits for.
pub const JOIN_RID: u32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    /// Connecting (the first time, or again).
    Connecting,
    Open,
    /// Waiting to connect again at `at`.
    Waiting {
        at: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    Turn,
    Invite,
    Cancel,
    Mod,
    // SHARE: `share_open` of share `n`, a `watch` (person, share), TURN for the watch session,
    // a moderator's `unshare`.
    ShareOpen(u8),
    Watch(String, u8),
    WatchTurn,
    Unshare,
}

#[derive(Debug)]
struct Stay {
    room_id: String,
    kind: RoomKind,
    /// The ticket to connect with: the join ticket, then the resume ticket from `hello`.
    ticket: Ticket,
    generation: u64,
    link: Link,
    /// Since when the connection is away (socket or audio); None while all is well.
    away_since: Option<u64>,
    /// "Yeniden bağlanıyor…" shows (the connection is away for more than 2 s).
    away_shown: bool,
    /// The stay ends at this moment unless the connection is back.
    give_up_at: Option<u64>,
    backoff: Backoff,
    /// A first `hello` came: the stay is real (before it, refusals answer the join).
    welcomed: bool,
    /// The join's caller still waits for its first outcome.
    join_waiting: bool,
    room: Option<Room>,
    /// The page's offer, waiting for the socket.
    offer: Option<(Option<Sdp>, Option<String>)>,
    /// A `join` went out on this socket (the page owes nothing more until a new peer connection).
    join_sent: bool,
    /// The page asked for a new peer connection and its offer has not come yet.
    offer_asked: bool,
    /// The audio connection works (the page said `connected`, or a `sync` kept it).
    audio_ok: bool,
    /// Audio connections lost in a row since the last `connected` (the next rejoin waits longer).
    losses: u32,
    /// The page is asked for a new peer connection at this moment (a rejoin that waits).
    rejoin_at: Option<u64>,
    calling: Option<(String, i64)>,
    opened_by_me: bool,
    speaking: BTreeSet<String>,
    talking: bool,
    quality: u8,
    ptt_down: bool,
    idle: Idle,
    /// VOICE-CALL: since when someone else shares the room with the player (Unix seconds, this
    /// computer's clock; the call timer's start). Set once per stay.
    together_since: Option<i64>,
}

#[derive(Debug, Clone)]
struct Missed {
    room: String,
    from: String,
    at: i64,
    ticket: Ticket,
    ended: bool,
}

/// What the machine knows of the voice page.
#[derive(Debug, Default)]
struct Page {
    attached: bool,
    mic: Option<VoiceMic>,
    devices: VoiceDevices,
    setup: bool,
    testing: bool,
}

pub struct Machine {
    me: String,
    flag: bool,
    service: Option<ServiceVoice>,
    hub_up: bool,
    store: Store,
    stay: Option<Stay>,
    /// Kept between stays of this run (Discord keeps them too).
    mute: bool,
    deaf: bool,
    rings: Vec<(IncomingRing, u64)>,
    missed: Vec<Missed>,
    crews: BTreeMap<String, Vec<CrewRoom>>,
    page: Page,
    /// How the voice page plays people, as it last said (none before the first sound opened: the
    /// WebAudio path, the one VOICE-0 measured).
    playout: Option<Playout>,
    window: Lifecycle,
    /// The window is wanted (a stay, the settings page, the self test).
    window_wanted: bool,
    input_on: bool,
    ring_sound: bool,
    next_generation: u64,
    next_rid: u32,
    pending: BTreeMap<u32, Pending>,
    /// The room the last failed stay was in ("Tekrar dene").
    retry: Option<(String, Option<String>, Ticket)>,
    /// BAN-TYPES: the end of the voice chat ban that ends the stay now (`closed` `muted`), for its
    /// notice and a waiting join's answer; the next `end_stay` takes it.
    restricted_until: Option<i64>,
    backoff: BackoffPolicy,
    /// VOICE-CALL: the page shows live meters now (its window on screen, a room's pill or panel).
    meters: bool,
    /// What the attached page measures for the meters (`me`, `people`); a fresh page: neither.
    meters_sent: (bool, bool),
    /// SOC-NEXT §3.7 (VOICE-CALL): whom the player ignores on this computer (the page's list,
    /// sorted; empty while its switch is off). Their rings stay quiet.
    ignored: Vec<String>,
    /// SHARE: screen sharing in the room.
    share: share_machine::Shares,
}

impl std::fmt::Debug for Machine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Machine")
            .field("in_room", &self.stay.is_some())
            .field("rings", &self.rings.len())
            .finish_non_exhaustive()
    }
}

impl Machine {
    pub fn new(store: Store) -> Self {
        Self {
            me: String::new(),
            flag: false,
            service: None,
            hub_up: false,
            store,
            stay: None,
            mute: false,
            deaf: false,
            rings: Vec::new(),
            missed: Vec::new(),
            crews: BTreeMap::new(),
            page: Page::default(),
            playout: None,
            window: Lifecycle::default(),
            window_wanted: false,
            input_on: false,
            ring_sound: false,
            next_generation: 1,
            next_rid: 1,
            pending: BTreeMap::new(),
            retry: None,
            restricted_until: None,
            backoff: BackoffPolicy {
                initial: std::time::Duration::from_millis(1_000),
                max: std::time::Duration::from_secs(15),
                spread: std::time::Duration::from_millis(1_500),
            },
            meters: false,
            meters_sent: (false, false),
            ignored: Vec::new(),
            share: share_machine::Shares::default(),
        }
    }

    /// The backoff of reconnects (tests make it quick and fixed).
    pub fn set_backoff(&mut self, policy: BackoffPolicy) {
        self.backoff = policy;
    }

    pub fn settings(&self) -> &VoiceSettings {
        self.store.settings()
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The store, for what changes no state the page or the room sees (Discord's one question).
    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    pub fn in_room(&self) -> bool {
        self.stay.is_some()
    }

    /// The room the player is in or joining.
    pub fn room_id(&self) -> Option<&str> {
        self.stay.as_ref().map(|stay| stay.room_id.as_str())
    }

    /// Voice chat may be offered: the signed config and Studio's switch agree (a presence
    /// reconnect does not hide it).
    pub fn available(&self) -> bool {
        self.flag && self.service.is_some_and(|service| service.on)
    }

    /// New calls and joins may start now: available, the player's switch on, the hub connected.
    fn usable(&self) -> bool {
        self.available() && self.store.settings().enabled && self.hub_up && !self.me.is_empty()
    }

    fn service(&self) -> ServiceVoice {
        self.service.unwrap_or_default()
    }

    // ---------------------------------------------------------------------------------------
    // Switches

    /// The signed config's `features.voice` (with `friends`).
    pub fn set_flag(&mut self, on: bool, now: u64) -> Vec<Effect> {
        if self.flag == on {
            return Vec::new();
        }
        self.flag = on;
        let mut out = vec![Effect::Changed];
        if !on {
            self.end_everything(now, None, &mut out);
        }
        out
    }

    /// The signed-in player's id (the friends service's), or empty when signed out.
    pub fn set_me(&mut self, me: &str, now: u64) -> Vec<Effect> {
        if self.me == me {
            return Vec::new();
        }
        self.me = me.to_owned();
        let mut out = vec![Effect::Changed];
        if me.is_empty() {
            self.service = None;
            self.hub_up = false;
            self.crews.clear();
            self.end_everything(now, None, &mut out);
        }
        out
    }

    /// The presence connection connected: whether it took `voice1`, Studio's settings, and the
    /// crews' live rooms.
    pub fn hub_hello(
        &mut self,
        taken: bool,
        service: Option<ServiceVoice>,
        rooms: Vec<(String, Vec<CrewRoom>)>,
    ) -> Vec<Effect> {
        self.hub_up = taken;
        self.service = if taken { service } else { None };
        self.crews = rooms.into_iter().filter(|(_, r)| !r.is_empty()).collect();
        let mut out = vec![Effect::Changed];
        // A `voice1` socket counts as "on" until told otherwise.
        if taken && !self.store.settings().enabled {
            out.push(Effect::HubSwitch { on: false });
        }
        self.apply_service(&mut out);
        out
    }

    /// Studio's settings changed (`voice_state`).
    pub fn hub_settings(&mut self, service: ServiceVoice) -> Vec<Effect> {
        if self.service.is_none() {
            return Vec::new();
        }
        self.service = Some(service);
        let mut out = vec![Effect::Changed];
        self.apply_service(&mut out);
        out
    }

    fn apply_service(&mut self, out: &mut Vec<Effect>) {
        let service = self.service();
        if let Some(stay) = &mut self.stay {
            stay.idle.set_minutes(service.idle_minutes);
        }
        if self.page.attached {
            out.push(Effect::Engine(self.configure()));
        }
    }

    /// The presence connection dropped (it comes back by itself): rings and calls wait, a room
    /// goes on (its own socket and the SFU do not need the hub).
    pub fn hub_down(&mut self) -> Vec<Effect> {
        self.hub_up = false;
        vec![Effect::Changed]
    }

    // ---------------------------------------------------------------------------------------
    // Settings

    pub fn save_settings(
        &mut self,
        settings: VoiceSettings,
        now: u64,
    ) -> (VoiceSettings, Vec<Effect>) {
        let before = self.store.settings().clone();
        let kept = self.store.set_settings(settings);
        let mut out = vec![Effect::Changed];
        self.after_settings(&before, now, &mut out);
        (kept, out)
    }

    pub fn restore(&mut self, backup: crate::settings::VoiceBackup, now: u64) -> Vec<Effect> {
        let before = self.store.settings().clone();
        self.store.restore(backup);
        let mut out = vec![Effect::Changed];
        self.after_settings(&before, now, &mut out);
        out.extend(self.playback());
        out
    }

    fn after_settings(&mut self, before: &VoiceSettings, now: u64, out: &mut Vec<Effect>) {
        let after = self.store.settings().clone();
        self.share_settings_changed(out);
        if before.enabled != after.enabled {
            if self.hub_up {
                out.push(Effect::HubSwitch { on: after.enabled });
            }
            if !after.enabled {
                // Off: leave, stop ringing, nothing more.
                self.end_everything(now, None, out);
            }
        }
        if self.page.attached {
            out.push(Effect::Engine(self.configure()));
        }
        if before.watched_keys() != after.watched_keys() || before.mode != after.mode {
            if let Some(stay) = &mut self.stay {
                stay.ptt_down = false;
            }
            // The watch takes the new keys (or stops).
            let keys = after.watched_keys();
            let wanted = crate::input::needs_input(self.stay.is_some(), keys.len());
            if wanted || self.input_on {
                self.input_on = wanted;
                out.push(Effect::Input(wanted.then_some(keys)));
            }
            self.talk(out);
        }
    }

    fn configure(&self) -> EngineCommand {
        let settings = self.store.settings();
        EngineCommand::Configure {
            settings: EngineSettings {
                input_device: settings.input_device.clone(),
                output_device: settings.output_device.clone(),
                noise_suppression: settings.noise_suppression,
                echo_cancellation: settings.echo_cancellation,
                auto_gain: settings.auto_gain,
                sensitivity: settings.sensitivity,
                mic_volume: settings.mic_volume,
                push_to_talk: settings.push_to_talk().is_some(),
                kbps: self.service().kbps,
                sounds: settings.sounds,
                // VOICE-CALL: everyone at once, held to the path's top as a person's volume is.
                output_volume: gain::clamp(settings.output_volume, self.playout()),
            },
        }
    }

    /// A person's volume (percent) for the player.
    pub fn set_volume(&mut self, person: &str, percent: u16) -> Vec<Effect> {
        if !self
            .store
            .set_person(person, |prefs| prefs.volume = percent)
        {
            return Vec::new();
        }
        let mut out = vec![Effect::Changed];
        out.extend(self.playback());
        out
    }

    /// "Sustur (yalnız sen)".
    pub fn set_muted_for_me(&mut self, person: &str, on: bool) -> Vec<Effect> {
        if !self.store.set_person(person, |prefs| prefs.muted = on) {
            return Vec::new();
        }
        let mut out = vec![Effect::Changed];
        out.extend(self.playback());
        out
    }

    // ---------------------------------------------------------------------------------------
    // Rings

    /// May the player call `friend` now (the caller asks the hub then)?
    pub fn may_call(&self) -> Result<(), VoiceAnswer> {
        if self.usable() {
            Ok(())
        } else {
            Err(VoiceAnswer::refused(VoiceRefusal::Off))
        }
    }

    /// VOICE-ROOMS: may the crew page change rooms now (voice chat usable, and the service lets
    /// crews arrange their rooms)? The role is the service's to check.
    pub fn may_manage(&self) -> Result<(), VoiceAnswer> {
        self.may_call()?;
        if self.service().manage {
            Ok(())
        } else {
            Err(VoiceAnswer::refused(VoiceRefusal::Off))
        }
    }

    /// A ring for the player (`voice_ring`); `quiet` on do-not-disturb. Someone the player ignores
    /// (SOC-NEXT §3.7) rings quieter still: their call waits in the missed list without a word (no
    /// card, no tone, no Windows notification, nothing in the game).
    pub fn ring(&mut self, ring: IncomingRing, quiet: bool, now: u64, unix: i64) -> Vec<Effect> {
        if !self.available() || !self.store.settings().enabled || !wire::is_room_id(&ring.room) {
            return Vec::new();
        }
        if self
            .stay
            .as_ref()
            .is_some_and(|stay| stay.room_id == ring.room)
        {
            return Vec::new();
        }
        self.rings.retain(|(old, _)| old.room != ring.room);
        let mut out = vec![Effect::Changed];
        if self.ignores(&ring.from) {
            self.keep_missed(&ring);
            self.sync_ring(&mut out);
            return out;
        }
        if quiet || ring.until <= unix {
            self.add_missed(&ring, &mut out);
            return out;
        }
        let deadline = now + secs_ms(ring.until - unix);
        out.push(Effect::System(SystemNotice::Ring {
            room: ring.room.clone(),
            from: ring.from.clone(),
            with: ring.with.clone(),
        }));
        self.rings.push((ring, deadline));
        self.sync_ring(&mut out);
        out
    }

    /// `voice_ring_end`: joined or declined on another device, or the room ended.
    pub fn ring_end(&mut self, room: &str, reason: RingEnd) -> Vec<Effect> {
        let mut out = Vec::new();
        let before = self.rings.len();
        self.rings.retain(|(ring, _)| ring.room != room);
        if self.rings.len() != before {
            out.push(Effect::System(SystemNotice::RingGone {
                room: room.to_owned(),
            }));
            out.push(Effect::Changed);
        }
        match reason {
            RingEnd::Ended => {
                for missed in &mut self.missed {
                    if missed.room == room && !missed.ended {
                        missed.ended = true;
                        out.push(Effect::Changed);
                    }
                }
            }
            // Handled elsewhere: no "Cevapsız arama" for it here.
            RingEnd::Joined | RingEnd::Declined => {
                let before = self.missed.len();
                self.missed.retain(|missed| missed.room != room);
                if self.missed.len() != before {
                    out.push(Effect::Changed);
                }
            }
        }
        self.sync_ring(&mut out);
        dedup_changed(&mut out);
        out
    }

    fn add_missed(&mut self, ring: &IncomingRing, out: &mut Vec<Effect>) {
        self.keep_missed(ring);
        out.push(Effect::Notice(VoiceNotice::Missed {
            from: ring.from.clone(),
            room: ring.room.clone(),
        }));
        out.push(Effect::System(SystemNotice::Missed {
            room: ring.room.clone(),
            from: ring.from.clone(),
        }));
    }

    /// The missed list keeps the call (the friend's row says "Cevapsız arama"), without a word.
    fn keep_missed(&mut self, ring: &IncomingRing) {
        self.missed.retain(|missed| missed.room != ring.room);
        self.missed.insert(
            0,
            Missed {
                room: ring.room.clone(),
                from: ring.from.clone(),
                at: ring.at,
                ticket: ring.ticket.clone(),
                ended: false,
            },
        );
        self.missed.truncate(MISSED_MAX);
    }

    fn ignores(&self, user: &str) -> bool {
        self.ignored
            .binary_search_by(|id| id.as_str().cmp(user))
            .is_ok()
    }

    /// Whom the player ignores here (`voice_ignored`: the page's SOC-NEXT list, empty while its
    /// switch is off). A ring of theirs that rings now goes quiet at once, into the missed list.
    pub fn set_ignored(&mut self, mut ids: Vec<String>) -> Vec<Effect> {
        ids.retain(|id| !id.is_empty() && id.len() <= IGNORED_ID_MAX);
        ids.sort_unstable();
        ids.dedup();
        ids.truncate(IGNORED_MAX);
        if ids == self.ignored {
            return Vec::new();
        }
        self.ignored = ids;
        let (quiet, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.rings)
            .into_iter()
            .partition(|(ring, _)| self.ignores(&ring.from));
        self.rings = kept;
        let mut out = Vec::new();
        for (ring, _) in quiet {
            out.push(Effect::System(SystemNotice::RingGone {
                room: ring.room.clone(),
            }));
            self.keep_missed(&ring);
            out.push(Effect::Changed);
        }
        self.sync_ring(&mut out);
        dedup_changed(&mut out);
        out
    }

    /// "Reddet": the ring stops on every device and the caller hears nothing more than a time-out.
    pub fn decline(&mut self, room: &str) -> Vec<Effect> {
        let before = self.rings.len();
        self.rings.retain(|(ring, _)| ring.room != room);
        if self.rings.len() == before {
            return Vec::new();
        }
        let mut out = vec![
            Effect::HubDecline {
                room: room.to_owned(),
            },
            Effect::System(SystemNotice::RingGone {
                room: room.to_owned(),
            }),
            Effect::Changed,
        ];
        self.sync_ring(&mut out);
        out
    }

    /// Takes a missed call off the list.
    pub fn dismiss_missed(&mut self, room: &str) -> Vec<Effect> {
        let before = self.missed.len();
        self.missed.retain(|missed| missed.room != room);
        if self.missed.len() == before {
            Vec::new()
        } else {
            vec![Effect::Changed]
        }
    }

    /// "Katıl" on a ring or a missed call that is still joinable: its ticket.
    pub fn ticket_for(&self, room: &str, unix: i64) -> Option<Ticket> {
        if let Some((ring, _)) = self.rings.iter().find(|(ring, _)| ring.room == room) {
            return Some(ring.ticket.clone());
        }
        self.missed
            .iter()
            .find(|missed| missed.room == room && joinable(missed, unix))
            .map(|missed| missed.ticket.clone())
    }

    fn sync_ring(&mut self, out: &mut Vec<Effect>) {
        let wanted = !self.rings.is_empty();
        if wanted != self.ring_sound {
            self.ring_sound = wanted;
            out.push(Effect::Ring(wanted));
        }
    }

    /// One crew's live rooms changed (`voice_rooms`).
    pub fn crew_rooms(&mut self, crew: &str, rooms: Vec<CrewRoom>) -> Vec<Effect> {
        if rooms.is_empty() {
            self.crews.remove(crew);
        } else {
            self.crews.insert(crew.to_owned(), rooms);
        }
        vec![Effect::Changed]
    }

    // ---------------------------------------------------------------------------------------
    // Stays

    /// Joins a room with a ticket (a call's, a ring's, a crew room's, a move's). A stay in
    /// another room ends first, quietly.
    pub fn join(&mut self, start: StayStart, now: u64, unix: i64) -> Vec<Effect> {
        let mut out = Vec::new();
        if !self.available() || !self.store.settings().enabled {
            out.push(Effect::Answer {
                rid: JOIN_RID,
                answer: VoiceAnswer::refused(VoiceRefusal::Off),
            });
            return out;
        }
        if self.stay.is_some() {
            self.end_stay(now, None, &mut out);
            // A move to another room is no ending (VOICE-CALL): its "Connected" says it.
            out.retain(|effect| !is_sound(effect, EngineSound::Ended));
        }
        // Joining ends its ring and its missed call; calling a friend back ("Geri ara") ends their
        // missed calls too.
        let before = (self.rings.len(), self.missed.len());
        self.rings.retain(|(ring, _)| ring.room != start.room);
        let called = start.calling.as_deref();
        self.missed
            .retain(|missed| missed.room != start.room && Some(missed.from.as_str()) != called);
        if (self.rings.len(), self.missed.len()) != before {
            out.push(Effect::System(SystemNotice::RingGone {
                room: start.room.clone(),
            }));
        }
        self.sync_ring(&mut out);
        self.retry = None;
        let generation = self.generation();
        let service = self.service();
        let calling = start
            .calling
            .map(|friend| (friend, unix + i64::from(service.ring_seconds)));
        self.stay = Some(Stay {
            kind: RoomKind::of(&start.room),
            room_id: start.room,
            ticket: start.ticket.clone(),
            generation,
            link: Link::Connecting,
            away_since: None,
            away_shown: false,
            give_up_at: None,
            backoff: Backoff::new(self.backoff),
            welcomed: false,
            join_waiting: true,
            room: None,
            offer: None,
            join_sent: false,
            offer_asked: false,
            audio_ok: false,
            losses: 0,
            rejoin_at: None,
            calling,
            opened_by_me: start.opened_by_me,
            speaking: BTreeSet::new(),
            talking: false,
            quality: 0,
            ptt_down: false,
            idle: Idle::new(now, service.idle_minutes),
            together_since: None,
        });
        out.push(Effect::Connect {
            generation,
            ticket: start.ticket,
        });
        self.want_window(now, &mut out);
        if self.page.attached {
            self.ask_offer(&mut out);
        }
        self.sync_meters(&mut out);
        self.sync_input(&mut out);
        out.push(Effect::Changed);
        out
    }

    /// "Ayrıl" (also "İptal" while calling).
    pub fn leave(&mut self, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.stay.is_some() {
            self.end_stay(now, None, &mut out);
        }
        out
    }

    /// "Tekrar dene" after "Bağlanamadı.": the same room again while its seat may still hold
    /// (a crew room asks the service for a new ticket instead: the caller's part).
    pub fn retry_ticket(&self) -> Option<(String, Option<String>, Ticket)> {
        self.retry.clone()
    }

    /// The player blocked `user`: a two-person call with them ends at once.
    pub fn blocked(&mut self, user: &str, now: u64) -> Vec<Effect> {
        let alone_with = self
            .stay
            .as_ref()
            .and_then(|stay| stay.room.as_ref())
            .is_some_and(|room| {
                room.kind == RoomKind::Friend
                    && room.people.len() == 1
                    && room.people.contains_key(user)
            });
        let mut out = Vec::new();
        if alone_with {
            self.end_stay(now, None, &mut out);
        }
        self.rings.retain(|(ring, _)| ring.from != user);
        self.missed.retain(|missed| missed.from != user);
        self.sync_ring(&mut out);
        out.push(Effect::Changed);
        out
    }

    fn generation(&mut self) -> u64 {
        let generation = self.next_generation;
        self.next_generation += 1;
        generation
    }

    fn rid(&mut self) -> u32 {
        let rid = self.next_rid;
        self.next_rid = self.next_rid.wrapping_add(1).max(1);
        rid
    }

    /// Ends the stay: `leave` to the room, the page's call down, the window lingers a minute.
    /// `reason`: why, when the player did not choose it.
    fn end_stay(
        &mut self,
        now: u64,
        reason: Option<(VoiceLeftReason, Option<String>)>,
        out: &mut Vec<Effect>,
    ) {
        let restricted_until = self.restricted_until.take();
        let Some(stay) = self.stay.take() else {
            return;
        };
        // SHARE: every share stops, nothing is watched (the room's `leave` ends them there).
        self.share_end(out);
        if stay.link == Link::Open {
            out.push(Effect::Send {
                generation: stay.generation,
                frame: ToRoom::Leave,
            });
        }
        out.push(Effect::Disconnect {
            generation: stay.generation,
        });
        if stay.join_waiting {
            let answer = match reason {
                Some((VoiceLeftReason::Unavailable, _)) => VoiceAnswer::refused(VoiceRefusal::Off),
                // BAN-TYPES: a voice chat ban, until its end.
                Some((VoiceLeftReason::Restricted, _)) => VoiceAnswer {
                    until: restricted_until,
                    ..VoiceAnswer::refused(VoiceRefusal::StaffMuted)
                },
                Some(_) => VoiceAnswer::refused(VoiceRefusal::Failed),
                None => VoiceAnswer::refused(VoiceRefusal::Nothing),
            };
            out.push(Effect::Answer {
                rid: JOIN_RID,
                answer,
            });
        }
        for (rid, _) in std::mem::take(&mut self.pending) {
            out.push(Effect::Answer {
                rid,
                answer: VoiceAnswer::refused(VoiceRefusal::Nothing),
            });
        }
        if self.page.attached {
            // VOICE-CALL: the stay's end is heard before the page closes it (its sound outlasts
            // the call: the page's sound waits 1.5 s after a stay). Never for a join refused
            // before it began.
            if stay.welcomed {
                self.cue(EngineSound::Ended, out);
            }
            out.push(Effect::Engine(EngineCommand::Leave));
            // The page's leave turns its meters off.
            self.meters_sent = (false, false);
        }
        if let Some((reason, friend)) = reason {
            if reason == VoiceLeftReason::Failed {
                self.retry = Some((
                    stay.room_id.clone(),
                    stay.room.as_ref().and_then(|room| room.crew.clone()),
                    stay.ticket.clone(),
                ));
            }
            out.push(Effect::Notice(VoiceNotice::Left {
                reason,
                friend: friend.clone(),
                until: restricted_until.filter(|_| reason == VoiceLeftReason::Restricted),
            }));
            if matches!(
                reason,
                VoiceLeftReason::Kicked
                    | VoiceLeftReason::Closed
                    | VoiceLeftReason::Unavailable
                    | VoiceLeftReason::Restricted
            ) {
                out.push(Effect::System(SystemNotice::Left { reason }));
            }
        }
        self.want_window(now, out);
        self.sync_input(out);
        out.push(Effect::Changed);
    }

    fn end_everything(
        &mut self,
        now: u64,
        reason: Option<(VoiceLeftReason, Option<String>)>,
        out: &mut Vec<Effect>,
    ) {
        self.end_stay(now, reason, out);
        for (ring, _) in std::mem::take(&mut self.rings) {
            out.push(Effect::System(SystemNotice::RingGone { room: ring.room }));
        }
        self.missed.clear();
        self.page.setup = false;
        self.page.testing = false;
        self.want_window(now, out);
        self.sync_ring(out);
    }

    // ---------------------------------------------------------------------------------------
    // The room socket

    fn current(&mut self, generation: u64) -> Option<&mut Stay> {
        self.stay
            .as_mut()
            .filter(|stay| stay.generation == generation)
    }

    /// The socket of `generation` opened.
    pub fn socket_open(&mut self, generation: u64, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        let Some(stay) = self.current(generation) else {
            return out;
        };
        stay.link = Link::Open;
        if stay.join_waiting {
            stay.join_waiting = false;
            out.push(Effect::Answer {
                rid: JOIN_RID,
                answer: VoiceAnswer::OK,
            });
        }
        if stay.welcomed && stay.audio_ok {
            // Back with the resume ticket and the audio still flowing.
            stay.join_sent = true;
            out.push(Effect::Send {
                generation,
                frame: ToRoom::Sync { caps: Self::caps() },
            });
        } else {
            stay.join_sent = false;
            if let Some((sdp, mid)) = stay.offer.take() {
                stay.join_sent = true;
                out.push(Effect::Send {
                    generation,
                    frame: ToRoom::Join {
                        sdp,
                        mid,
                        caps: Self::caps(),
                    },
                });
            } else if self.page.attached {
                self.ask_offer(&mut out);
            }
        }
        let _ = now;
        out
    }

    /// The socket of `generation` could not open: an HTTP refusal (`status`, `code`) or none
    /// (the network).
    pub fn socket_refused(
        &mut self,
        generation: u64,
        status: Option<u16>,
        code: Option<&str>,
        now: u64,
    ) -> Vec<Effect> {
        let mut out = Vec::new();
        let Some(stay) = self.current(generation) else {
            return out;
        };
        let welcomed = stay.welcomed;
        let transient = matches!(status, None | Some(500..=599) | Some(429));
        if transient && (welcomed || stay.away_since.is_some() || !stay.join_waiting) {
            self.connection_away(now, None, &mut out);
            return out;
        }
        if transient {
            // The first connect: once more, then "Bağlanamadı.".
            stay.join_waiting = false;
            out.push(Effect::Answer {
                rid: JOIN_RID,
                answer: VoiceAnswer::OK,
            });
            self.connection_away(now, None, &mut out);
            return out;
        }
        let answer = VoiceAnswer::from_code(code.unwrap_or("bad_request"), None, None);
        if stay.join_waiting {
            stay.join_waiting = false;
            out.push(Effect::Answer {
                rid: JOIN_RID,
                answer,
            });
            // Refused before it began: no toast beyond the answer.
            self.end_stay(now, None, &mut out);
        } else {
            let reason = match answer.refused {
                Some(VoiceRefusal::Off) => VoiceLeftReason::Unavailable,
                Some(VoiceRefusal::Kicked) => VoiceLeftReason::Kicked,
                // BAN-TYPES: a voice chat ban met coming back (its end is not in the refusal).
                Some(VoiceRefusal::StaffMuted) => VoiceLeftReason::Restricted,
                _ => VoiceLeftReason::Failed,
            };
            self.end_stay(now, Some((reason, None)), &mut out);
        }
        out
    }

    /// The socket of `generation` closed (`code`: the close frame's, none when it just dropped).
    pub fn socket_closed(&mut self, generation: u64, code: Option<u16>, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.current(generation).is_none() {
            return out;
        }
        match code {
            Some(wire::close::SESSION_ENDED) => {
                self.end_stay(now, Some((VoiceLeftReason::Unavailable, None)), &mut out);
            }
            Some(wire::close::PROTOCOL) => {
                tracing::warn!("voice: the room closed the socket for a protocol error");
                self.end_stay(now, Some((VoiceLeftReason::Failed, None)), &mut out);
            }
            Some(wire::close::REPLACED) => {
                // A newer socket of this device took the seat: nothing to come back to here.
                self.end_stay(now, None, &mut out);
            }
            Some(wire::close::RATE_LIMITED) => {
                tracing::warn!("voice: the room socket sent too much");
                self.connection_away(now, Some(RATE_LIMITED_WAIT_MS), &mut out);
            }
            _ => self.connection_away(now, None, &mut out),
        }
        out
    }

    /// The socket is gone for now: come back with backoff (`wait`: at least this long), give up
    /// a minute after it went.
    fn connection_away(&mut self, now: u64, wait: Option<u64>, out: &mut Vec<Effect>) {
        let Some(stay) = self.stay.as_mut() else {
            return;
        };
        let old = stay.generation;
        out.push(Effect::Disconnect { generation: old });
        stay.generation = self.next_generation;
        self.next_generation += 1;
        let since = *stay.away_since.get_or_insert(now);
        let delay = stay.backoff.next_delay().as_millis() as u64;
        let at = now + wait.unwrap_or(0).max(delay);
        stay.link = Link::Waiting { at };
        let limit = match wait {
            Some(wait) => {
                (now + wait + 30_000).min(since + wire::limits::SEAT_SECONDS * 1_000 - 5_000)
            }
            None => since + GIVE_UP_MS,
        };
        stay.give_up_at = Some(stay.give_up_at.map_or(limit, |old| old.max(limit)));
        stay.join_sent = false;
        // A TURN answer can only come on the socket that asked (the page's own wait reports the
        // connection lost, and the socket's return asks for a new one).
        self.pending.retain(|_, pending| *pending != Pending::Turn);
        out.push(Effect::Changed);
    }

    /// A message from the room on `generation`'s socket.
    pub fn socket_message(
        &mut self,
        generation: u64,
        message: FromRoom,
        now: u64,
        unix: i64,
    ) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.current(generation).is_none() {
            return out;
        }
        // SHARE: share frames are the share part's; what voice messages say of shares, after them.
        let Some(message) = self.share_message(message, now, &mut out) else {
            dedup_changed(&mut out);
            return out;
        };
        let seen = share_machine::Seen::of(&message);
        let again = self.stay.as_ref().is_some_and(|stay| stay.welcomed);
        self.voice_message(generation, message, now, unix, &mut out);
        if self.current(generation).is_some() {
            self.share_seen(seen, again, now, &mut out);
        }
        out
    }

    /// A voice message from the room on `generation`'s socket.
    fn voice_message(
        &mut self,
        generation: u64,
        message: FromRoom,
        now: u64,
        unix: i64,
        out: &mut Vec<Effect>,
    ) {
        match message {
            FromRoom::Hello {
                room,
                me,
                people,
                rings,
                sdp,
                resume,
                ..
            } => {
                let my_id = self.me.clone();
                let Some(stay) = self.stay.as_mut() else {
                    return;
                };
                let had_room = stay.room.take();
                let model = Room::from_hello(&my_id, room, me.role, people, rings);
                if let Some(resume) = resume {
                    stay.ticket = resume;
                }
                let first = !stay.welcomed;
                stay.welcomed = true;
                // The room is back. With an answer the audio is still on its way: a connection
                // lost before stays away (and gives up after its minute) until the page says
                // `connected`, so rejoins that never connect cannot hold the stay forever.
                let back = sdp.is_none() && stay.away_shown;
                if sdp.is_none() {
                    stay.away_since = None;
                    stay.away_shown = false;
                    stay.give_up_at = None;
                }
                stay.backoff.reset();
                let mut joined = Vec::new();
                if let Some(before) = had_room {
                    for id in model.people.keys() {
                        if !before.people.contains_key(id) {
                            joined.push(id.clone());
                        }
                    }
                }
                let alone = model.people.is_empty();
                // VOICE-CALL: the friend the player calls is here already (they came before this
                // `hello`): the call is answered as if they had just come.
                let answered = stay
                    .calling
                    .as_ref()
                    .is_some_and(|(friend, _)| model.people.contains_key(friend));
                if answered {
                    stay.calling = None;
                }
                if !alone && stay.together_since.is_none() {
                    stay.together_since = Some(unix);
                }
                stay.room = Some(model);
                if let Some(sdp) = sdp {
                    stay.audio_ok = false;
                    if self.page.attached {
                        out.push(Effect::Engine(EngineCommand::Answer {
                            sdp: sdp.into_inner(),
                        }));
                    }
                }
                let (mute, deaf) = (self.mute, self.deaf);
                out.push(Effect::Send {
                    generation,
                    frame: ToRoom::Set { mute, deaf },
                });
                let calling = self
                    .stay
                    .as_ref()
                    .is_some_and(|stay| stay.calling.is_some());
                if calling && alone && self.page.attached {
                    out.push(Effect::Engine(EngineCommand::Sound {
                        sound: EngineSound::Waiting,
                    }));
                }
                if answered && self.page.attached {
                    out.push(Effect::Engine(EngineCommand::Sound {
                        sound: EngineSound::Stop,
                    }));
                }
                // VOICE-CALL: in (a caller's own wait says it with its tone), or back after
                // "Yeniden bağlanıyor…"; a room that changed meanwhile says who came.
                if (first && !calling) || back {
                    self.cue(EngineSound::Connected, out);
                } else if !joined.is_empty() {
                    self.cue(EngineSound::Join, out);
                }
                out.extend(self.playback());
                self.talk(out);
                self.sync_meters(out);
                out.push(Effect::Changed);
            }
            FromRoom::Offer { sdp, mids } => {
                if self.page.attached {
                    out.push(Effect::Engine(EngineCommand::Offer {
                        sdp: sdp.into_inner(),
                        mids,
                    }));
                }
            }
            FromRoom::Kicked => {
                self.end_stay(now, Some((VoiceLeftReason::Kicked, None)), out);
            }
            FromRoom::Closed { why, until } => {
                let friend = self
                    .stay
                    .as_ref()
                    .and_then(|stay| stay.calling.as_ref().map(|(friend, _)| friend.clone()));
                let reason = match why {
                    ClosedWhy::Leader => VoiceLeftReason::Closed,
                    ClosedWhy::Alone => VoiceLeftReason::Alone,
                    ClosedWhy::NoAnswer => VoiceLeftReason::NoAnswer,
                    ClosedWhy::Elsewhere => VoiceLeftReason::Elsewhere,
                    ClosedWhy::Crew => VoiceLeftReason::Crew,
                    ClosedWhy::Muted => VoiceLeftReason::Restricted,
                    ClosedWhy::Off | ClosedWhy::Budget | ClosedWhy::Staff | ClosedWhy::Account => {
                        VoiceLeftReason::Unavailable
                    }
                };
                let friend = (reason == VoiceLeftReason::NoAnswer)
                    .then_some(friend)
                    .flatten();
                // BAN-TYPES: a voice chat ban's end goes with the notice.
                if reason == VoiceLeftReason::Restricted {
                    self.restricted_until = until;
                }
                self.end_stay(now, Some((reason, friend)), out);
            }
            FromRoom::Moved { room, .. } if !wire::is_room_id(&room) => {
                tracing::warn!("voice: a move to a room id that is not one");
                self.end_stay(now, Some((VoiceLeftReason::Failed, None)), out);
            }
            FromRoom::Moved { room, ticket } => {
                let name = self
                    .crews
                    .values()
                    .flatten()
                    .find(|crew_room| crew_room.id == room)
                    .map(|crew_room| crew_room.name.clone())
                    .unwrap_or_default();
                let mut moved = self.join(
                    StayStart {
                        room: room.clone(),
                        ticket,
                        calling: None,
                        opened_by_me: false,
                    },
                    now,
                    unix,
                );
                // Nobody waits for this join's answer.
                moved.retain(|effect| !matches!(effect, Effect::Answer { rid: JOIN_RID, .. }));
                if let Some(stay) = &mut self.stay {
                    stay.join_waiting = false;
                }
                out.extend(moved);
                out.push(Effect::Notice(VoiceNotice::Moved { room, name }));
            }
            FromRoom::Ack { rid, ice, .. } => {
                let Some(rid) = rid else {
                    return;
                };
                match self.pending.remove(&rid) {
                    Some(Pending::Turn) => match ice {
                        Some(ice) if self.page.attached && !ice.urls.is_empty() => {
                            out.push(Effect::Engine(EngineCommand::Ice {
                                ice: EngineIce {
                                    urls: ice.urls,
                                    username: ice.username,
                                    credential: ice.credential,
                                },
                            }));
                        }
                        // No credentials after all: as a lost connection (L2's P1).
                        _ => self.audio_lost(now, out),
                    },
                    Some(_) => out.push(Effect::Answer {
                        rid,
                        answer: VoiceAnswer::OK,
                    }),
                    None => {}
                }
            }
            FromRoom::Error {
                rid,
                code,
                retry_after,
                until,
                ..
            } => {
                if let Some(rid) = rid
                    && let Some(pending) = self.pending.remove(&rid)
                {
                    if pending == Pending::Turn {
                        // `turn_unavailable` (the budget guard, no TURN key) or `rate_limited`:
                        // no stay without a connection, so as lost (L2's P1).
                        tracing::debug!(code, "voice: no TURN for this connection");
                        self.audio_lost(now, out);
                    } else {
                        out.push(Effect::Answer {
                            rid,
                            answer: VoiceAnswer::from_code(&code, retry_after, until),
                        });
                    }
                }
            }
            FromRoom::Unknown => {}
            other => {
                let attached = self.page.attached;
                let mic_chosen = !self.mute && !self.deaf;
                let Some(stay) = self.stay.as_mut() else {
                    return;
                };
                let Some(room) = stay.room.as_mut() else {
                    return;
                };
                let cut_before = room.my_sm;
                let applied = room.apply(&other);
                let cut = room.my_sm;
                if let Some(id) = &applied.left {
                    stay.speaking.remove(id);
                }
                // VOICE-CALL: the call's timer starts with the first one to share the room.
                if applied.joined.is_some() && stay.together_since.is_none() {
                    stay.together_since = Some(unix);
                }
                // Someone came: the call is answered.
                let answered = applied.joined.is_some() && stay.calling.take().is_some();
                if answered && attached {
                    out.push(Effect::Engine(EngineCommand::Sound {
                        sound: EngineSound::Stop,
                    }));
                }
                if answered {
                    self.cue(EngineSound::Connected, out);
                } else if applied.joined.is_some() {
                    self.cue(EngineSound::Join, out);
                } else if applied.left.is_some() {
                    self.cue(EngineSound::Leave, out);
                }
                if applied.muted_me {
                    out.push(Effect::Notice(VoiceNotice::MutedByModerator));
                    out.push(Effect::System(SystemNotice::MutedByModerator));
                }
                // A moderator's mute (or its end) turns a microphone the player keeps open.
                if cut != cut_before && mic_chosen {
                    let sound = if cut {
                        EngineSound::MicOff
                    } else {
                        EngineSound::MicOn
                    };
                    self.cue(sound, out);
                }
                if applied.playback {
                    out.extend(self.playback());
                }
                // Alone, nothing goes out (VOICE-CALL): the gate follows who is in the room too.
                if (matches!(other, FromRoom::State { .. }) && applied.changed)
                    || applied.joined.is_some()
                    || applied.left.is_some()
                {
                    self.talk(out);
                }
                if cut != cut_before {
                    self.sync_meters(out);
                }
                if applied.changed {
                    out.push(Effect::Changed);
                }
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Requests on the room socket

    fn open_generation(&self) -> Option<u64> {
        self.stay
            .as_ref()
            .filter(|stay| stay.link == Link::Open && stay.welcomed)
            .map(|stay| stay.generation)
    }

    /// "Davet et": `rid` to wait on (its answer comes as [`Effect::Answer`]).
    pub fn invite(&mut self, to: &str) -> Result<(u32, Vec<Effect>), VoiceAnswer> {
        self.request(Pending::Invite, |rid| ToRoom::Invite {
            to: to.to_owned(),
            rid,
        })
    }

    /// Takes the player's ringing invite back.
    pub fn cancel_invite(&mut self, to: &str) -> Result<(u32, Vec<Effect>), VoiceAnswer> {
        self.request(Pending::Cancel, |rid| ToRoom::Cancel {
            to: to.to_owned(),
            rid,
        })
    }

    /// A leader's or moderator's action.
    pub fn moderate(&mut self, action: &VoiceModAction) -> Result<(u32, Vec<Effect>), VoiceAnswer> {
        let (act, to, room, name) = match action {
            VoiceModAction::Mute { person } => (ModAct::Mute, Some(person.clone()), None, None),
            VoiceModAction::Unmute { person } => (ModAct::Unmute, Some(person.clone()), None, None),
            VoiceModAction::Kick { person } => (ModAct::Kick, Some(person.clone()), None, None),
            VoiceModAction::Unkick { person } => (ModAct::Unkick, Some(person.clone()), None, None),
            VoiceModAction::Move { person, room } => {
                if !wire::is_room_id(room) {
                    return Err(VoiceAnswer::refused(VoiceRefusal::RoomGone));
                }
                (ModAct::Move, Some(person.clone()), Some(room.clone()), None)
            }
            VoiceModAction::Mod { person } => (ModAct::Mod, Some(person.clone()), None, None),
            VoiceModAction::Unmod { person } => (ModAct::Unmod, Some(person.clone()), None, None),
            VoiceModAction::Rename { name } => {
                let name = name.trim();
                let chars = name.chars().count();
                if !(3..=24).contains(&chars) {
                    return Err(VoiceAnswer::refused(VoiceRefusal::InvalidName));
                }
                (ModAct::Name, None, None, Some(name.to_owned()))
            }
            VoiceModAction::Close => (ModAct::Close, None, None, None),
        };
        self.request(Pending::Mod, |rid| ToRoom::Mod {
            act,
            to,
            room,
            name,
            n: None,
            who: None,
            rid,
        })
    }

    fn request(
        &mut self,
        pending: Pending,
        frame: impl FnOnce(u32) -> ToRoom,
    ) -> Result<(u32, Vec<Effect>), VoiceAnswer> {
        let Some(generation) = self.open_generation() else {
            return Err(VoiceAnswer::refused(if self.stay.is_some() {
                VoiceRefusal::Failed
            } else {
                VoiceRefusal::Nothing
            }));
        };
        let rid = self.rid();
        self.pending.insert(rid, pending);
        Ok((
            rid,
            vec![Effect::Send {
                generation,
                frame: frame(rid),
            }],
        ))
    }

    /// A request's caller gave up waiting: forget its `rid`.
    pub fn forget(&mut self, rid: u32) {
        self.pending.remove(&rid);
    }

    // ---------------------------------------------------------------------------------------
    // The player's own state

    pub fn set_mute(&mut self, on: bool) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.mute == on && !(self.deaf && !on) {
            return out;
        }
        let was_deaf = self.deaf;
        self.mute = on;
        // Opening the microphone ends deafness too (Discord's way).
        if !on {
            self.deaf = false;
            if self.page.attached {
                out.push(Effect::Engine(EngineCommand::Deaf { on: false }));
            }
        }
        self.send_set(&mut out);
        self.talk(&mut out);
        // VOICE-CALL: heard whoever turned it (the page, a shortcut, the tray, the game's overlay);
        // opening it while deafened brings the sound back, and says that.
        if self.stay.is_some() {
            match (on, was_deaf) {
                (true, false) => self.cue(EngineSound::MicOff, &mut out),
                (false, true) => self.cue(EngineSound::DeafOff, &mut out),
                (false, false) => self.cue(EngineSound::MicOn, &mut out),
                // Deafened, the microphone was off already.
                (true, true) => {}
            }
        }
        self.sync_meters(&mut out);
        out.push(Effect::Changed);
        out
    }

    pub fn set_deaf(&mut self, on: bool) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.deaf == on {
            return out;
        }
        self.deaf = on;
        if self.page.attached {
            out.push(Effect::Engine(EngineCommand::Deaf { on }));
        }
        self.send_set(&mut out);
        self.talk(&mut out);
        if self.stay.is_some() {
            let sound = if on {
                EngineSound::DeafOn
            } else {
                EngineSound::DeafOff
            };
            self.cue(sound, &mut out);
        }
        self.sync_meters(&mut out);
        out.push(Effect::Changed);
        out
    }

    fn send_set(&self, out: &mut Vec<Effect>) {
        if let Some(generation) = self.open_generation() {
            out.push(Effect::Send {
                generation,
                frame: ToRoom::Set {
                    mute: self.mute,
                    deaf: self.deaf,
                },
            });
        }
    }

    /// Whether the microphone may send now.
    fn talk_open(&self) -> bool {
        let Some(stay) = &self.stay else {
            return false;
        };
        let muted_for_all = stay.room.as_ref().is_some_and(|room| room.my_sm);
        // VOICE-CALL: nobody else in the room hears it, so nothing goes out until someone comes
        // (no packets for the SFU, no work for the noise suppressor; a caller waiting alone).
        let alone = stay.room.as_ref().is_none_or(|room| room.people.is_empty());
        let push_to_talk = self.store.settings().push_to_talk().is_some();
        !self.mute && !self.deaf && !muted_for_all && !alone && (!push_to_talk || stay.ptt_down)
    }

    /// The gate as it should be now; a close cuts at once (a mute, a deafen, a moderator's mute).
    fn talk(&self, out: &mut Vec<Effect>) {
        self.talk_with(false, out);
    }

    /// `tail`: push-to-talk was let go, the page keeps the end of the word (200 ms).
    fn talk_with(&self, tail: bool, out: &mut Vec<Effect>) {
        if self.page.attached && self.stay.is_some() {
            out.push(Effect::Engine(EngineCommand::Talk {
                open: self.talk_open(),
                tail,
            }));
        }
    }

    /// A shortcut's key went down or up (Raw Input).
    pub fn key(&mut self, change: crate::input::KeyChange) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.stay.is_none() {
            return out;
        }
        match change.shortcut {
            VoiceShortcut::PushToTalk => {
                if let Some(stay) = &mut self.stay {
                    stay.ptt_down = change.down;
                }
                self.talk_with(!change.down, &mut out);
            }
            // Deafened, the microphone shows as off: the mute key opens it and ends deafness
            // (Discord's way; flipping `mute` alone would change nothing the player can see).
            // Their own sounds say it (VOICE-CALL; the shortcut's tick went).
            VoiceShortcut::Mute if change.down => {
                out.extend(self.set_mute(!(self.mute || self.deaf)));
            }
            VoiceShortcut::Deafen if change.down => {
                out.extend(self.set_deaf(!self.deaf));
            }
            _ => {}
        }
        out
    }

    /// One of voice chat's own sounds (VOICE-CALL), while the page plays them and the player
    /// keeps "Sesli sohbet sesleri" on.
    fn cue(&self, sound: EngineSound, out: &mut Vec<Effect>) {
        if self.page.attached && self.store.settings().sounds {
            out.push(Effect::Engine(EngineCommand::Sound { sound }));
        }
    }

    /// The page's live meters as they should be now (VOICE-CALL): the people's while the page
    /// shows a room, the player's microphone while it is theirs to open (not muted, deafened or
    /// muted for everyone). A fresh page starts with both off.
    fn sync_meters(&mut self, out: &mut Vec<Effect>) {
        if !self.page.attached {
            return;
        }
        let wanted = match &self.stay {
            Some(stay) if self.meters => {
                let muted_for_all = stay.room.as_ref().is_some_and(|room| room.my_sm);
                (!self.mute && !self.deaf && !muted_for_all, true)
            }
            _ => (false, false),
        };
        if wanted == self.meters_sent {
            return;
        }
        self.meters_sent = wanted;
        out.push(Effect::Engine(EngineCommand::Meters {
            me: wanted.0,
            people: wanted.1,
        }));
    }

    /// The page shows live meters now (its window on screen with a room's pill or panel), or
    /// not (VOICE-CALL).
    pub fn set_meters(&mut self, on: bool) -> Vec<Effect> {
        if self.meters == on {
            return Vec::new();
        }
        self.meters = on;
        let mut out = Vec::new();
        self.sync_meters(&mut out);
        out
    }

    /// The live meters' levels as the page reported them, for the launcher's page (VOICE-CALL):
    /// only while it asks for them, only people in the room, the player's microphone only while
    /// it is measured.
    pub fn levels_of(
        &self,
        me: f32,
        open: bool,
        ids: Vec<String>,
        levels: Vec<f32>,
    ) -> Option<VoiceLevels> {
        if !self.meters {
            return None;
        }
        let stay = self.stay.as_ref()?;
        let people = match &stay.room {
            Some(room) => ids
                .into_iter()
                .zip(levels)
                .filter(|(id, _)| room.people.contains_key(id))
                .map(|(id, level)| VoicePersonLevel {
                    id,
                    level: level.clamp(0.0, 1.0),
                })
                .collect(),
            None => Vec::new(),
        };
        let measured = self.meters_sent.0;
        Some(VoiceLevels {
            me: if measured { me.clamp(0.0, 1.0) } else { 0.0 },
            open: measured && open,
            people,
        })
    }

    fn sync_input(&mut self, out: &mut Vec<Effect>) {
        let keys = self.store.settings().watched_keys();
        let wanted = crate::input::needs_input(self.stay.is_some(), keys.len());
        if wanted != self.input_on {
            self.input_on = wanted;
            out.push(Effect::Input(wanted.then_some(keys)));
        }
    }

    /// How each person plays now.
    fn playback(&self) -> Vec<Effect> {
        let Some(room) = self.stay.as_ref().and_then(|stay| stay.room.as_ref()) else {
            return Vec::new();
        };
        if !self.page.attached {
            return Vec::new();
        }
        let playout = self.playout();
        let people = room
            .people_in_order()
            .into_iter()
            .map(|person| {
                let prefs = self.store.person(&person.u);
                EnginePerson {
                    id: person.u.clone(),
                    volume: gain::clamp(prefs.volume, playout),
                    silent: prefs.muted || person.sm,
                }
            })
            .collect();
        vec![Effect::Engine(EngineCommand::People { people })]
    }

    /// How the page plays people (the WebAudio path until it said otherwise).
    fn playout(&self) -> Playout {
        self.playout.unwrap_or(Playout::WebAudio)
    }

    // ---------------------------------------------------------------------------------------
    // The voice window and its page

    fn want_window(&mut self, now: u64, out: &mut Vec<Effect>) {
        let wanted = self.stay.is_some() || self.page.setup || self.page.testing;
        if wanted == self.window_wanted {
            return;
        }
        self.window_wanted = wanted;
        let action = if wanted {
            self.window.join(now)
        } else {
            self.window.leave(now)
        };
        if action != lifecycle::Action::None {
            out.push(Effect::Window(action));
        }
    }

    /// The settings page opened (`on`) or closed: the engine runs for its devices and level.
    pub fn set_setup(&mut self, on: bool, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        if on && (!self.available() || !self.store.settings().enabled) {
            return out;
        }
        if self.page.setup == on && (on || !self.page.testing) {
            return out;
        }
        self.page.setup = on;
        if !on && self.page.testing {
            self.page.testing = false;
            if self.page.attached {
                out.push(Effect::Engine(EngineCommand::SelfTest { on: false }));
            }
            out.push(Effect::Changed);
        }
        if self.page.attached {
            out.push(Effect::Engine(EngineCommand::Setup { on }));
        }
        self.want_window(now, &mut out);
        out
    }

    /// "Mikrofonu dene".
    pub fn set_test(&mut self, on: bool, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        if (on && (!self.available() || !self.store.settings().enabled)) || self.page.testing == on
        {
            return out;
        }
        self.page.testing = on;
        if self.page.attached {
            out.push(Effect::Engine(EngineCommand::SelfTest { on }));
        }
        self.want_window(now, &mut out);
        out.push(Effect::Changed);
        out
    }

    /// The page attached (after the window opened, or again after a crash).
    pub fn page_attached(&mut self) -> Vec<Effect> {
        self.page.attached = true;
        // A fresh page measures nothing for the meters until told.
        self.meters_sent = (false, false);
        let mut out = vec![Effect::Engine(self.configure())];
        if self.page.setup {
            out.push(Effect::Engine(EngineCommand::Setup { on: true }));
        }
        if self.page.testing {
            out.push(Effect::Engine(EngineCommand::SelfTest { on: true }));
        }
        if self.stay.is_some() {
            out.push(Effect::Engine(EngineCommand::Deaf { on: self.deaf }));
            if let Some(stay) = &mut self.stay {
                // A new page means a new peer connection.
                stay.audio_ok = false;
                stay.join_sent = false;
                stay.offer = None;
            }
            self.ask_offer(&mut out);
            out.extend(self.playback());
            self.talk(&mut out);
            self.sync_meters(&mut out);
        }
        out
    }

    fn ask_offer(&mut self, out: &mut Vec<Effect>) {
        if let Some(stay) = &mut self.stay
            && !stay.offer_asked
        {
            stay.offer_asked = true;
            stay.audio_ok = false;
            stay.rejoin_at = None;
            out.push(Effect::Engine(EngineCommand::Join));
        }
    }

    /// The audio connection is gone (the page said `lost`, or TURN could not be had): no stay
    /// without a connection (L2's P1). "Yeniden bağlanıyor…" after 2 s, "Bağlanamadı." after a
    /// minute unless the page says `connected` before; the page joins anew, at once the first
    /// time, then after a growing wait ([`REJOIN_MAX_MS`]). A detached page asks for its own
    /// connection when it attaches.
    fn audio_lost(&mut self, now: u64, out: &mut Vec<Effect>) {
        let attached = self.page.attached;
        let Some(stay) = self.stay.as_mut() else {
            return;
        };
        stay.audio_ok = false;
        stay.away_since.get_or_insert(now);
        stay.give_up_at.get_or_insert(now + GIVE_UP_MS);
        // The page has no connection left to make an offer from, nor one it made before.
        stay.offer_asked = false;
        stay.offer = None;
        let wait = rejoin_wait(stay.losses);
        stay.losses = stay.losses.saturating_add(1);
        out.push(Effect::Changed);
        if !attached {
            return;
        }
        if wait == 0 {
            self.ask_offer(out);
        } else {
            stay.rejoin_at = Some(now + wait);
        }
    }

    /// The window's page went away (closed by the launcher).
    pub fn page_gone(&mut self) {
        self.page.attached = false;
        self.meters_sent = (false, false);
        if let Some(stay) = &mut self.stay {
            stay.offer_asked = false;
        }
    }

    /// The window's WebView crashed: open it again (the stay goes on) or, after a few, give up.
    pub fn page_crashed(&mut self, now: u64) -> Vec<Effect> {
        self.page_gone();
        let mut out = Vec::new();
        match self.window.crashed(now) {
            lifecycle::Action::Reopen => {
                out.push(Effect::Window(lifecycle::Action::Reopen));
                if self.stay.is_some() {
                    if let Some(stay) = &mut self.stay {
                        stay.away_since.get_or_insert(now);
                    }
                    out.push(Effect::Changed);
                }
            }
            lifecycle::Action::GiveUp => {
                self.window_wanted = false;
                self.page.setup = false;
                self.page.testing = false;
                self.end_stay(now, Some((VoiceLeftReason::Failed, None)), &mut out);
                // The window is closed already; nothing lingers.
                out.retain(|effect| !matches!(effect, Effect::Window(_)));
            }
            _ => {}
        }
        out
    }

    /// A report from the voice page.
    pub fn page_report(&mut self, report: PageReport, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        if report.is_oversized() {
            tracing::warn!(?report, "voice: the page sent an oversized report");
            return out;
        }
        match report {
            PageReport::Ready => {}
            PageReport::Offer { sdp, mid } => {
                let Some(stay) = self.stay.as_mut() else {
                    return out;
                };
                stay.offer_asked = false;
                let sdp = sdp.map(Sdp::new);
                // A first join, or a new peer connection during the stay (it replaces the SFU
                // session on the same socket).
                let send_now = stay.link == Link::Open && (!stay.join_sent || stay.welcomed);
                if send_now {
                    stay.join_sent = true;
                    out.push(Effect::Send {
                        generation: stay.generation,
                        frame: ToRoom::Join {
                            sdp,
                            mid,
                            caps: Self::caps(),
                        },
                    });
                } else {
                    stay.offer = Some((sdp, mid));
                }
            }
            PageReport::Answer { sdp } => {
                if let Some(generation) = self.open_generation() {
                    out.push(Effect::Send {
                        generation,
                        frame: ToRoom::Answer { sdp: Sdp::new(sdp) },
                    });
                }
            }
            PageReport::Speaking { ids, me } => {
                let Some(stay) = self.stay.as_mut() else {
                    return out;
                };
                let known: BTreeSet<String> = ids
                    .into_iter()
                    .filter(|id| {
                        stay.room
                            .as_ref()
                            .is_some_and(|room| room.people.contains_key(id))
                    })
                    .collect();
                if me {
                    stay.idle.spoke(now);
                }
                if known != stay.speaking || me != stay.talking {
                    stay.speaking = known;
                    stay.talking = me;
                    out.push(Effect::Speaking(VoiceSpeaking {
                        ids: stay.speaking.iter().cloned().collect(),
                        me,
                    }));
                }
            }
            PageReport::Quality { q } => {
                let q = wire::quality(q);
                let generation = self.open_generation();
                if let Some(stay) = self.stay.as_mut()
                    && stay.quality != q
                {
                    stay.quality = q;
                    if let Some(generation) = generation {
                        out.push(Effect::Send {
                            generation,
                            frame: ToRoom::Q { v: q },
                        });
                    }
                    out.push(Effect::Changed);
                }
            }
            PageReport::NeedTurn => {
                // No audio meanwhile: "Yeniden bağlanıyor…" after 2 s (§7.10), and the minute
                // runs, until the page connects through TURN.
                if let Some(stay) = self.stay.as_mut() {
                    stay.away_since.get_or_insert(now);
                    stay.give_up_at.get_or_insert(now + GIVE_UP_MS);
                    out.push(Effect::Changed);
                }
                match self.request(Pending::Turn, |rid| ToRoom::Turn { rid }) {
                    Ok((_, effects)) => out.extend(effects),
                    // No socket to ask on: as lost (the offer waits for the socket's return).
                    Err(_) => self.audio_lost(now, &mut out),
                }
            }
            PageReport::Lost => self.audio_lost(now, &mut out),
            PageReport::Connected => {
                let mut back = false;
                if let Some(stay) = self.stay.as_mut() {
                    stay.audio_ok = true;
                    stay.losses = 0;
                    stay.rejoin_at = None;
                    if stay.link == Link::Open {
                        back = stay.away_shown;
                        stay.away_since = None;
                        stay.away_shown = false;
                        stay.give_up_at = None;
                    }
                    out.push(Effect::Changed);
                }
                // VOICE-CALL: back after "Yeniden bağlanıyor…".
                if back {
                    self.cue(EngineSound::Connected, &mut out);
                }
            }
            PageReport::Playout { playout } => {
                if self.playout != Some(playout) {
                    self.playout = Some(playout);
                    // Volumes held to the path's top, and the slider's top with them; the
                    // master volume too (VOICE-CALL).
                    if self.page.attached {
                        out.push(Effect::Engine(self.configure()));
                    }
                    out.extend(self.playback());
                    out.push(Effect::Changed);
                }
            }
            // The manager hands these straight to the page's event (`Voice::page_report`).
            PageReport::Levels { .. } => {}
            PageReport::Mic { mic, changed } => {
                let before = self.page.mic;
                self.page.mic = Some(mic);
                if changed {
                    out.push(Effect::Notice(VoiceNotice::MicChanged));
                }
                if before != Some(mic) {
                    if matches!(mic, VoiceMic::Denied | VoiceMic::Missing) && self.stay.is_some() {
                        out.push(Effect::Notice(VoiceNotice::Mic { mic }));
                    }
                    out.push(Effect::Changed);
                }
            }
            PageReport::Devices { devices } => {
                if self.page.devices != devices {
                    self.page.devices = devices;
                    out.push(Effect::Changed);
                }
            }
            PageReport::Level { level, open } => {
                if self.page.setup || self.page.testing {
                    out.push(Effect::Level(VoiceLevel {
                        level: level.clamp(0.0, 1.0),
                        open,
                    }));
                }
            }
            PageReport::Stats { .. } => {}
            PageReport::Failed { code } => {
                tracing::warn!(code, "voice: the page reported a failure");
            }
        }
        out
    }

    // ---------------------------------------------------------------------------------------
    // Time

    /// The moment the machine wants [`Machine::tick`], if any (`last_input`: the system's last
    /// touch, for the inactivity rule).
    pub fn next_deadline(&self, last_input: u64) -> Option<u64> {
        let mut next: Option<u64> = None;
        let mut at = |moment: u64| next = Some(next.map_or(moment, |n| n.min(moment)));
        for (_, deadline) in &self.rings {
            at(*deadline);
        }
        if let Some(close) = self.window.deadline() {
            at(close);
        }
        if let Some(stay) = &self.stay {
            if let Link::Waiting { at: when } = stay.link {
                at(when);
            }
            if let Some(give_up) = stay.give_up_at {
                at(give_up);
            }
            if let Some(rejoin) = stay.rejoin_at {
                at(rejoin);
            }
            if let Some(since) = stay.away_since
                && !stay.away_shown
            {
                at(since + RECONNECTING_SHOWN_MS);
            }
            // SHARE: someone who shares or watches is no idle player.
            if stay.welcomed && !self.share_active() {
                at(stay.idle.deadline(last_input));
            }
            self.share_deadline(last_input, &mut at);
        }
        next
    }

    pub fn tick(&mut self, now: u64, unix: i64, last_input: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        // Rings that ran out: missed calls.
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.rings)
            .into_iter()
            .partition(|(_, deadline)| *deadline <= now);
        self.rings = kept;
        for (ring, _) in gone {
            out.push(Effect::System(SystemNotice::RingGone {
                room: ring.room.clone(),
            }));
            self.add_missed(&ring, &mut out);
            out.push(Effect::Changed);
        }
        self.sync_ring(&mut out);
        if self.window.tick(now) == lifecycle::Action::Close {
            self.page.attached = false;
            out.push(Effect::Window(lifecycle::Action::Close));
        }
        let mut end: Option<VoiceLeftReason> = None;
        let mut reconnect: Option<(u64, Ticket)> = None;
        let mut rejoin = false;
        let mut lost = false;
        // SHARE: sharing's own moments (rest, end, out of sight, "Hâlâ izliyor musun?"); while it
        // runs the player is no idle player.
        if self.stay.as_ref().is_some_and(|stay| stay.welcomed) {
            self.share_tick(now, last_input, &mut out);
        }
        if self.share_active()
            && let Some(stay) = &mut self.stay
        {
            stay.idle.spoke(now);
        }
        if let Some(stay) = &mut self.stay {
            if stay.give_up_at.is_some_and(|at| now >= at) {
                end = Some(VoiceLeftReason::Failed);
            } else if stay.welcomed && stay.idle.due(now, last_input) {
                end = Some(VoiceLeftReason::Idle);
            } else {
                if let Link::Waiting { at } = stay.link
                    && now >= at
                {
                    stay.link = Link::Connecting;
                    reconnect = Some((stay.generation, stay.ticket.clone()));
                }
                if stay.rejoin_at.is_some_and(|at| now >= at) {
                    stay.rejoin_at = None;
                    rejoin = true;
                }
                if !stay.away_shown
                    && stay
                        .away_since
                        .is_some_and(|since| now >= since + RECONNECTING_SHOWN_MS)
                {
                    stay.away_shown = true;
                    lost = true;
                    out.push(Effect::Changed);
                }
                if let Some(room) = &mut stay.room
                    && room.expire_rings(unix)
                {
                    out.push(Effect::Changed);
                }
            }
        }
        // VOICE-CALL: "Yeniden bağlanıyor…" shows now; heard too (in the game, in the tray).
        if lost {
            self.cue(EngineSound::Lost, &mut out);
        }
        if let Some(reason) = end {
            self.end_stay(now, Some((reason, None)), &mut out);
        }
        if let Some((generation, ticket)) = reconnect {
            out.push(Effect::Connect { generation, ticket });
        }
        if rejoin && self.page.attached {
            self.ask_offer(&mut out);
        }
        dedup_changed(&mut out);
        out
    }

    // ---------------------------------------------------------------------------------------
    // The page's view

    pub fn view(&self, now: u64, unix: i64) -> VoiceState {
        let settings = self.store.settings();
        let service = self.service();
        let room = self
            .stay
            .as_ref()
            .and_then(|stay| self.room_view(stay, now));
        let calling = self.stay.as_ref().and_then(|stay| {
            stay.calling.as_ref().map(|(friend, until)| VoiceCalling {
                friend: friend.clone(),
                until: *until,
            })
        });
        VoiceState {
            available: self.available() && !self.me.is_empty(),
            enabled: settings.enabled,
            room,
            calling,
            rings: self
                .rings
                .iter()
                .map(|(ring, _)| VoiceRing {
                    room: ring.room.clone(),
                    from: ring.from.clone(),
                    with: ring.with.clone(),
                    until: ring.until,
                })
                .collect(),
            missed: self
                .missed
                .iter()
                .map(|missed| VoiceMissed {
                    room: missed.room.clone(),
                    from: missed.from.clone(),
                    at: missed.at,
                    joinable: joinable(missed, unix),
                })
                .collect(),
            crews: self
                .crews
                .iter()
                .map(|(crew, rooms)| crew_rooms_view(crew, rooms, service))
                .collect(),
            crew_rooms_max: service.crew_rooms,
            mic: self.page.mic.unwrap_or(VoiceMic::Unknown),
            devices: self.page.devices.clone(),
            testing: self.page.testing,
            profiles: self
                .stay
                .as_ref()
                .and_then(|stay| stay.room.as_ref())
                .map(|room| {
                    room.people_in_order()
                        .into_iter()
                        .filter_map(|person| {
                            person
                                .card
                                .as_deref()
                                .map(|card| VoiceProfile::of(&person.u, card))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            presence: self
                .crews
                .values()
                .flatten()
                .flat_map(|room| room.ids.iter())
                .filter(|id| **id != self.me)
                .map(|id| (id.clone(), pl_social::PresenceVoice::In))
                .collect(),
            volume_max: self.playout().max_percent(),
            backup_rev: self.store.revision(),
            ask_before_call: settings.ask_before_call,
            together_since: self.stay.as_ref().and_then(|stay| stay.together_since),
            share: self.share_view(),
        }
    }

    fn room_view(&self, stay: &Stay, now: u64) -> Option<VoiceRoomView> {
        let phase = if stay.away_shown
            || stay
                .away_since
                .is_some_and(|since| now >= since + RECONNECTING_SHOWN_MS)
        {
            VoicePhase::Reconnecting
        } else if stay.welcomed {
            VoicePhase::Live
        } else {
            VoicePhase::Joining
        };
        let kind = match stay.kind {
            RoomKind::Friend => VoiceRoomKind::Friend,
            RoomKind::Crew => VoiceRoomKind::Crew,
        };
        let me_view = |role: VoiceRole, sm: bool| VoiceMe {
            id: self.me.clone(),
            role,
            mute: self.mute,
            deaf: self.deaf,
            muted_by_moderator: sm,
            talking: stay.talking,
            quality: VoiceQuality::from_wire(stay.quality),
        };
        let Some(room) = &stay.room else {
            return Some(VoiceRoomView {
                id: stay.room_id.clone(),
                kind,
                crew: None,
                name: None,
                general: stay.room_id.starts_with('g'),
                max: 0,
                phase,
                me: me_view(VoiceRole::Member, false),
                people: Vec::new(),
                ringing: Vec::new(),
                can_rename: false,
                can_close: false,
            });
        };
        let my_role = role_view(room.my_role);
        // VOICE-ROOMS: "Genel" is always one of the crew's rooms, listed or not.
        let other_rooms = room.crew.as_ref().is_some_and(|crew| {
            !room.is_general()
                || self
                    .crews
                    .get(crew)
                    .is_some_and(|rooms| rooms.iter().any(|r| r.id != room.id))
        });
        let playout = self.playout();
        let people = room
            .people_in_order()
            .into_iter()
            .map(|person| {
                let prefs: VoicePersonPrefs = self.store.person(&person.u);
                let their_role = room.role_of(&person.u);
                let acts = match room.my_role {
                    Role::Leader => true,
                    Role::Moderator => their_role == Role::Member,
                    Role::Member => false,
                };
                VoicePerson {
                    id: person.u.clone(),
                    role: role_view(their_role),
                    mute: person.mute,
                    deaf: person.deaf,
                    muted_by_moderator: person.sm,
                    quality: VoiceQuality::from_wire(person.q),
                    speaking: stay.speaking.contains(&person.u),
                    // On the slider's scale (`volume_max`); the kept value stays for the other path.
                    volume: gain::clamp(prefs.volume, playout),
                    muted_for_me: prefs.muted,
                    joined_at: person.j,
                    can: VoicePowers {
                        mute: acts,
                        kick: acts,
                        move_to: acts && room.kind == RoomKind::Crew && other_rooms,
                        moderator: room.kind == RoomKind::Friend && room.my_role == Role::Leader,
                    },
                }
            })
            .collect();
        let general = room.is_general();
        Some(VoiceRoomView {
            id: room.id.clone(),
            kind,
            crew: room.crew.clone(),
            name: room.name.clone(),
            general,
            max: room.max,
            phase,
            me: me_view(my_role, room.my_sm),
            people,
            ringing: room
                .ringing
                .iter()
                .map(|(to, until)| VoiceOutRing {
                    to: to.clone(),
                    until: *until,
                })
                .collect(),
            can_rename: !general
                && match room.kind {
                    RoomKind::Friend => room.my_role == Role::Leader,
                    // VOICE-ROOMS: a kept room's opener, also in a later run (the crew's list says).
                    RoomKind::Crew => {
                        room.my_role != Role::Member
                            || stay.opened_by_me
                            || room
                                .crew
                                .as_ref()
                                .and_then(|crew| self.crews.get(crew))
                                .is_some_and(|rooms| rooms.iter().any(|r| r.id == room.id && r.own))
                    }
                },
            can_close: !general && room.my_role == Role::Leader,
        })
    }
}

fn role_view(role: Role) -> VoiceRole {
    match role {
        Role::Leader => VoiceRole::Leader,
        Role::Moderator => VoiceRole::Moderator,
        Role::Member => VoiceRole::Member,
    }
}

/// VOICE-ROOMS: one crew's rooms as the surfaces show them, "Genel" always first: a service from
/// before VOICE-ROOMS left an empty "Genel" out while another room was live (it disappeared from
/// the crew page), and lists it wherever it came. The rest keep the service's order.
fn crew_rooms_view(crew: &str, rooms: &[CrewRoom], service: ServiceVoice) -> VoiceCrewRooms {
    let general_id = format!("g{crew}");
    let general = rooms
        .iter()
        .find(|room| room.id == general_id)
        .cloned()
        .unwrap_or_else(|| CrewRoom {
            id: general_id.clone(),
            name: "Genel".to_owned(),
            ..CrewRoom::default()
        });
    let view = |room: &CrewRoom| VoiceCrewRoom {
        id: room.id.clone(),
        name: room.name.clone(),
        general: room.id.starts_with('g'),
        people: room.ids.clone(),
        max: room.max,
        own: room.own,
    };
    let rooms: Vec<VoiceCrewRoom> = std::iter::once(view(&general))
        .chain(rooms.iter().filter(|room| room.id != general_id).map(view))
        .collect();
    VoiceCrewRooms {
        crew: crew.to_owned(),
        can_add: rooms.len() < usize::from(service.crew_rooms),
        rooms,
        manage: service.manage,
        crew_max: service.crew_max,
    }
}

fn joinable(missed: &Missed, unix: i64) -> bool {
    !missed.ended && unix < missed.at + INVITE_TICKET_SECONDS
}

/// How long the rejoin after `losses` lost connections in a row waits: none, then 1, 2, 4, 8 s.
fn rejoin_wait(losses: u32) -> u64 {
    match losses {
        0 => 0,
        n => (1_000_u64 << (n - 1).min(3)).min(REJOIN_MAX_MS),
    }
}

fn secs_ms(seconds: i64) -> u64 {
    u64::try_from(seconds).unwrap_or(0).saturating_mul(1_000)
}

/// Whether an effect plays that sound on the page.
fn is_sound(effect: &Effect, sound: EngineSound) -> bool {
    matches!(effect, Effect::Engine(EngineCommand::Sound { sound: s }) if *s == sound)
}

/// One [`Effect::Changed`] is enough.
fn dedup_changed(out: &mut Vec<Effect>) {
    let mut seen = false;
    out.retain(|effect| {
        if matches!(effect, Effect::Changed) {
            if seen {
                return false;
            }
            seen = true;
        }
        true
    });
}
