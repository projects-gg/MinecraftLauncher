//! The machine's screen sharing part (SHARE; [`crate::share`] holds the types). A child of
//! [`crate::machine`], so it shares the machine's stay, socket and requests.
//!
//! - **Sharing:** `share_start` asks the publisher for an offer; the offer goes to the room as
//!   `share_open`; the room's `ack.sdp` goes back to the publisher. Nobody watching for a minute,
//!   encoding rests (`share_set paused`); a watcher brings it back with a key frame; 15 minutes
//!   alone, the share ends. The player's own windows leave the picture and their notifications
//!   stay quiet while anything is shared (`Effect::SharePrivacy`, as chosen).
//! - **Watching:** one watch session per player (the room's, never seen here); `watch` asks for a
//!   share, the room's `woffer` goes to the page that watches (the main window or the "Yayın"
//!   window), its answer back as `wanswer`. A page out of sight for a few seconds lets go of
//!   everything it watches and asks again when it is back; a page that moves (the "Yayın" window
//!   opens or closes) asks anew from there. Half an hour without a touch asks "Hâlâ izliyor musun?";
//!   a minute without an answer stops watching.
//! - Someone who shares or watches is no idle player: the inactivity rule waits while they do.
//! - **SQOL (`share2`, the lead's contract `docs/tasks/sqol.md` §1.2, §2.1):** while the service
//!   allows live changes (`share.edit`), a live share changes its source ("Ekranı Değiştir", the
//!   watchers staying), its quality, frame rate and sound (the room asked first, its applied values
//!   used), and holds a blank picture ("Duraklat"). A closed window no longer ends the share: it
//!   holds a minute for another source ("Başka Ekran Seç"), and a game the launcher starts meanwhile
//!   takes its place. Watchers hear what they see (`st`: `hold`, `min`).

use std::collections::BTreeMap;

use base64::Engine as _;

use super::{Effect, Machine, Pending};
use crate::engine::EngineSound;
use crate::share::{
    ALONE_STOP_MS, HIDDEN_GRACE_MS, HOLD_STOP_MS, PAUSE_AFTER_MS, SOUND_GAP_MS, STILL_ANSWER_MS,
    STILL_ASK_MS, ServiceShare, ShareAudioSource, ShareBy, ShareChange, ShareClosedWhy, ShareCmd,
    ShareEvent, ShareHoldWhy, ShareKind, ShareLayer, ShareLostWhy, ShareMinePhase, ShareMotion,
    ShareNotice, ShareOpts, SharePreset, ShareRefusal, ShareSource, ShareSt, ShareSurface,
    ShareWatchPhase, ShareWire, THUMB_MAX_BYTES, VoiceLiveShare, VoiceMyShare, VoiceShareView,
    VoiceStreamVolume, VoiceWatch, WATCH_LOSSES_MAX, WATCH_MAX, WATCH_OFFER_WAIT_MS, WatchCmd,
    WatchMid, WatchReport, WatchWire, normal_fps,
};
use crate::view::{VoiceAnswer, VoiceRefusal};
use crate::wire::{FromRoom, ModAct, ToRoom};

/// SQOL: a live change the room has not answered by then is given up (nothing changes).
const EDIT_WAIT_MS: u64 = 10_000;
/// MOB-WATCH: TURN the watching page has serves a new connection only while it has this long
/// left (the room's credentials last `ttl`; a connection set up on them must still come up).
const TURN_SPARE_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MinePhase {
    /// The publisher makes its offer.
    Preparing,
    /// `share_open` went out (its `rid`).
    Opening(u32),
    Live,
}

#[derive(Debug, Clone)]
struct Mine {
    n: u8,
    source: ShareSource,
    kind: ShareKind,
    label: String,
    opts: ShareOpts,
    phase: MinePhase,
    paused: bool,
    watchers: Vec<String>,
    /// Since when nobody watches (live shares only).
    alone_since: Option<u64>,
    since: i64,
    /// SHARE-LIVE: when the room took its last still picture (the room's `th`).
    th: Option<i64>,
    /// SQOL: "Görüntü" as chosen (`opts.motion` is what it means for the source).
    motion: ShareMotion,
    /// SQOL: "Önizleme Resmini Paylaş" for this share.
    thumbs: bool,
    /// SQOL: why the picture is blank (none: the source shows).
    hold: Option<ShareHoldWhy>,
    /// SQOL: a closed source's share ends at this moment unless another source comes.
    hold_until: Option<u64>,
    /// SQOL: the shared window is minimized.
    minimized: bool,
    /// SQOL: what the room was last told watchers see.
    told: ShareSt,
    /// SQOL: a switch on its way.
    switching: Option<Switching>,
    /// SQOL: a live change waits for the room: its `rid` and when it went.
    editing: Option<(u32, u64)>,
    /// SQOL-R3: what that newest change asked for (a quick change back is measured against it,
    /// not against what the room applied before).
    asked: Asked,
    /// SQOL-R3: the player's own "Duraklat", kept while a closed source's wait shows instead: a
    /// new source comes paused when it was.
    player_hold: bool,
    /// SQOL-R3: the publisher found no sound to share for this source: the room's answers do not
    /// turn it back on (the player's own "Sesi de Paylaş" or another source does).
    audio_failed: bool,
}

/// SQOL-R3: the values a live change asks the room for (none: unchanged).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Asked {
    preset: Option<SharePreset>,
    fps: Option<u8>,
    audio: Option<bool>,
}

/// SQOL: a source the publisher is switching to.
#[derive(Debug, Clone)]
struct Switching {
    source: ShareSource,
    label: String,
    /// The launcher's own (the game it started for a share whose game closed): no word when it
    /// does not happen.
    auto: bool,
}

impl Mine {
    /// What watchers see now.
    fn st(&self) -> ShareSt {
        if self.hold.is_some() {
            ShareSt::Hold
        } else if self.minimized {
            ShareSt::Min
        } else {
            ShareSt::Live
        }
    }
}

/// "Görüntü" for a source of `kind`: `auto` moves for a game, reads for the rest.
fn motion_of(choice: ShareMotion, kind: ShareKind) -> bool {
    match choice {
        ShareMotion::Auto => kind == ShareKind::Game,
        ShareMotion::Clarity => false,
        ShareMotion::Motion => true,
    }
}

#[derive(Debug, Clone)]
struct Watching {
    u: String,
    n: u8,
    layer: ShareLayer,
    /// `watch` went out on this session (the room knows it).
    sent: bool,
    /// Its `watch` still waits for the room's offer.
    rid: Option<u32>,
    /// When that `watch` went out (it is lost without an offer by `WATCH_OFFER_WAIT_MS`).
    asked_at: Option<u64>,
}

/// What the room said about shares in a message the machine handles for voice too.
pub(super) enum Seen {
    Nothing,
    Hello {
        /// `hello.room.shareBy`: present, the room shares (a `share1` room).
        share_by: Option<ShareBy>,
        /// `hello.me.watch`: what the room's watch session carries for the player.
        watch: Option<Vec<WatchWire>>,
        people: Vec<(String, Vec<ShareWire>)>,
    },
    Person(String, Vec<ShareWire>),
    Left(String),
    /// The `room` message's "Kim paylaşabilir".
    ShareBy(ShareBy),
}

impl Seen {
    pub(super) fn of(message: &FromRoom) -> Self {
        match message {
            FromRoom::Hello {
                room, me, people, ..
            } => Self::Hello {
                share_by: room.share_by,
                watch: me.watch.clone(),
                people: people
                    .iter()
                    .map(|person| (person.u.clone(), person.sh.clone()))
                    .collect(),
            },
            FromRoom::Joined { person } => Self::Person(person.u.clone(), person.sh.clone()),
            FromRoom::State {
                u, sh: Some(sh), ..
            } => Self::Person(u.clone(), sh.clone()),
            FromRoom::Left { u, .. } => Self::Left(u.clone()),
            FromRoom::Room {
                share_by: Some(by), ..
            } => Self::ShareBy(*by),
            _ => Self::Nothing,
        }
    }
}

/// Screen sharing's part of the machine.
#[derive(Debug)]
pub(super) struct Shares {
    /// The service's `hello.voice.share` (none: sharing off, or an older service).
    service: Option<ServiceShare>,
    /// The room said "Kim paylaşabilir": a `share1` room (an older one shows nothing of sharing).
    room_seen: bool,
    share_by: ShareBy,
    /// The publisher works on this computer, can share an app's sound, and encodes "Yüksek" in
    /// hardware.
    publisher: bool,
    audio: bool,
    high: bool,
    mine: Vec<Mine>,
    /// Others' shares, by person.
    remote: BTreeMap<String, Vec<ShareWire>>,
    watching: Vec<Watching>,
    surface: ShareSurface,
    /// Which pages attached their channel: [main, popout].
    attached: [bool; 2],
    /// Each watching page is on screen: [main, popout].
    visible: [bool; 2],
    /// The last moment the share part heard of (a `watch`'s time for its wait).
    clock: u64,
    hidden_since: Option<u64>,
    /// Watching let go while the page is out of sight.
    held: bool,
    /// The room's last offer, while its page has not attached yet.
    pending_offer: Option<WatchCmd>,
    /// The watch connection works.
    connected: bool,
    /// MOB-WATCH: until when the TURN the watching page was given serves a new connection (none:
    /// no end known, or none given).
    turn_until: Option<u64>,
    losses: u32,
    /// The last touch watching knows of (a watch, an answer, the window coming back).
    touched: u64,
    asking_since: Option<u64>,
    /// What `SharePrivacy` said last: (hide the launcher's windows, quiet notifications).
    privacy: (bool, bool),
    /// The "Yayın" window is open (or opening).
    popout: bool,
    /// SHARE-LIVE: when each of sharing's sounds last played ([`Machine::share_sound`]).
    sounded: [Option<u64>; 4],
}

impl Default for Shares {
    fn default() -> Self {
        Self {
            service: None,
            room_seen: false,
            share_by: ShareBy::All,
            publisher: false,
            audio: false,
            high: false,
            mine: Vec::new(),
            remote: BTreeMap::new(),
            watching: Vec::new(),
            surface: ShareSurface::Main,
            attached: [false; 2],
            visible: [true; 2],
            clock: 0,
            hidden_since: None,
            held: false,
            pending_offer: None,
            connected: false,
            turn_until: None,
            losses: 0,
            touched: 0,
            asking_since: None,
            privacy: (false, false),
            popout: false,
            sounded: [None; 4],
        }
    }
}

fn slot(surface: ShareSurface) -> usize {
    match surface {
        ShareSurface::Main => 0,
        ShareSurface::Popout => 1,
    }
}

impl Pending {
    /// Requests the share part answers itself (`unshare` waits like any moderator's action).
    fn is_share(&self) -> bool {
        matches!(
            self,
            Self::ShareOpen(_) | Self::Watch(..) | Self::WatchTurn | Self::ShareSet(_)
        )
    }
}

impl Machine {
    // ---------------------------------------------------------------------------------------
    // State

    /// The capabilities the room socket asks for.
    pub(super) fn caps() -> Vec<String> {
        vec![
            crate::share::CAP.to_owned(),
            crate::share::LIVE_CAP.to_owned(),
            crate::share::SHARE2_CAP.to_owned(),
            crate::share::RCHAT_CAP.to_owned(),
        ]
    }

    /// SQOL: the player's shares change live (the service's `share.edit` in a sharing room).
    fn share_edit(&self) -> bool {
        self.share_on() && self.share.service.is_some_and(|service| service.edit)
    }

    /// SQOL: tells the room what watchers of share `n` see, when that changed.
    fn sync_st(&mut self, n: u8, out: &mut Vec<Effect>) {
        let edit = self.share_edit();
        let generation = self.open_generation();
        let Some(mine) = self.share.mine.iter_mut().find(|mine| mine.n == n) else {
            return;
        };
        let st = mine.st();
        if !edit || mine.phase != MinePhase::Live || st == mine.told {
            return;
        }
        if let Some(generation) = generation {
            mine.told = st;
            out.push(Effect::Send {
                generation,
                frame: ToRoom::share_st(n, st),
            });
        }
    }

    /// SHARE-LIVE: one of sharing's sounds (the voice page plays it, as "Sesli sohbet sesleri"
    /// allows), at most once a [`SOUND_GAP_MS`]: two shares that start or end together sound once.
    fn share_sound(&mut self, sound: EngineSound, now: u64, out: &mut Vec<Effect>) {
        let slot = match sound {
            EngineSound::ShareStart => 0,
            EngineSound::ShareStop => 1,
            EngineSound::StreamStart => 2,
            EngineSound::StreamEnd => 3,
            _ => return,
        };
        if self.share.sounded[slot].is_some_and(|at| now.saturating_sub(at) < SOUND_GAP_MS) {
            return;
        }
        self.share.sounded[slot] = Some(now);
        self.cue(sound, out);
    }

    /// Sharing works in the room now.
    fn share_on(&self) -> bool {
        self.stay.as_ref().is_some_and(|stay| stay.welcomed)
            && self.share.room_seen
            && self.share.service.is_some()
    }

    fn mine_max(&self) -> u8 {
        self.share
            .service
            .map_or(0, |service| service.per_user.min(3))
    }

    fn max_preset(&self) -> SharePreset {
        let service = self
            .share
            .service
            .map_or(SharePreset::Low, |service| service.max_preset);
        // Without a hardware encoder "Yüksek" would cost the game (SHARE-N).
        if self.share.high {
            service
        } else {
            service.min(SharePreset::Normal)
        }
    }

    /// The player's role in the room.
    fn my_role(&self) -> crate::wire::Role {
        self.stay
            .as_ref()
            .and_then(|stay| stay.room.as_ref())
            .map_or(crate::wire::Role::Member, |room| room.my_role)
    }

    /// "Kim paylaşabilir" lets the player share.
    fn share_allowed(&self) -> bool {
        self.share.share_by == ShareBy::All || self.my_role() != crate::wire::Role::Member
    }

    /// What the service allows of screen sharing (`hello.voice.share`; none: off). Turned off,
    /// every share and watch ends here too (the room ends them as well).
    pub fn set_share_service(&mut self, service: Option<ServiceShare>, now: u64) -> Vec<Effect> {
        if self.share.service == service {
            return Vec::new();
        }
        self.share.service = service;
        self.share.clock = now;
        let mut out = vec![Effect::Changed];
        if service.is_none() {
            // The room closes them too; telling it first frees them at once.
            let ns: Vec<u8> = self.share.mine.iter().map(|mine| mine.n).collect();
            for n in ns {
                self.close_mine(n, Some(ShareClosedWhy::Limit), true, now, &mut out);
            }
            // SHARE-LIVE: what the player watched ended (Studio's switch, the month's budget).
            let watched = self.share.watching.iter().any(|watch| watch.u != self.me);
            self.stop_watching(None, &mut out);
            if watched {
                self.share_sound(EngineSound::StreamEnd, now, &mut out);
            }
        }
        out
    }

    /// The player shares or watches: no idle player.
    pub(super) fn share_active(&self) -> bool {
        !self.share.mine.is_empty() || (!self.share.watching.is_empty() && !self.share.held)
    }

    /// Sharing or watching changed: the inactivity rule counts from now.
    fn share_touch_idle(&mut self, now: u64) {
        if let Some(stay) = &mut self.stay {
            stay.idle.spoke(now);
        }
    }

    /// Whether the publisher works on this computer, can share an app's sound, and encodes
    /// "Yüksek" in hardware.
    pub fn set_share_publisher(&mut self, available: bool, audio: bool, high: bool) -> Vec<Effect> {
        if (self.share.publisher, self.share.audio, self.share.high) == (available, audio, high) {
            return Vec::new();
        }
        self.share.publisher = available;
        self.share.audio = audio;
        self.share.high = high;
        vec![Effect::Changed]
    }

    fn sync_privacy(&mut self, out: &mut Vec<Effect>) {
        let active = !self.share.mine.is_empty();
        let settings = &self.store.settings().share;
        let wanted = (active && settings.hide_launcher, active && settings.quiet);
        if wanted != self.share.privacy {
            self.share.privacy = wanted;
            out.push(Effect::SharePrivacy {
                hide: wanted.0,
                quiet: wanted.1,
            });
        }
    }

    /// The settings changed: privacy follows while sharing.
    pub(super) fn share_settings_changed(&mut self, out: &mut Vec<Effect>) {
        self.sync_privacy(out);
    }

    // ---------------------------------------------------------------------------------------
    // Sharing

    /// "Paylaş" in the picker: `label` is the window's title or the screen's name (this computer
    /// only).
    pub fn share_start(
        &mut self,
        source: ShareSource,
        label: &str,
        now: u64,
        unix: i64,
    ) -> Result<Vec<Effect>, ShareRefusal> {
        if !self.share_on() {
            return Err(ShareRefusal::Off);
        }
        if !self.share.publisher {
            return Err(ShareRefusal::Unsupported);
        }
        if !source.is_plausible() {
            return Err(ShareRefusal::Failed);
        }
        if !self.share_allowed() || self.mine_max() == 0 {
            return Err(ShareRefusal::NotAllowed);
        }
        if !self.share.service.is_some_and(|service| service.open) {
            return Err(ShareRefusal::Budget);
        }
        if self.share.mine.iter().any(|mine| mine.source == source) {
            return Ok(Vec::new());
        }
        if self.share.mine.len() >= usize::from(self.mine_max()) {
            return Err(ShareRefusal::Full);
        }
        let Some(n) = (1..=9u8).find(|n| self.share.mine.iter().all(|mine| mine.n != *n)) else {
            return Err(ShareRefusal::Full);
        };
        let kind = source.kind();
        let settings = &self.store.settings().share;
        let opts = ShareOpts::of(settings, kind, self.max_preset(), self.share.audio);
        let (motion, thumbs) = (settings.motion, settings.thumbs);
        let label = match kind {
            ShareKind::Game => String::new(),
            _ => crate::share::clip_label(label),
        };
        self.share.mine.push(Mine {
            n,
            source: source.clone(),
            kind,
            label,
            opts,
            phase: MinePhase::Preparing,
            paused: false,
            watchers: Vec::new(),
            alone_since: None,
            since: unix,
            th: None,
            motion,
            thumbs,
            hold: None,
            hold_until: None,
            minimized: false,
            told: ShareSt::Live,
            switching: None,
            editing: None,
            asked: Asked::default(),
            player_hold: false,
            audio_failed: false,
        });
        let mut out = vec![Effect::Share(ShareCmd::Prepare { n, source, opts })];
        self.sync_privacy(&mut out);
        self.share_touch_idle(now);
        out.push(Effect::Changed);
        Ok(out)
    }

    /// "Durdur" on one of the player's shares (the panel, the tray, the game's overlay).
    pub fn share_stop(&mut self, n: u8, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        self.close_mine(n, None, true, now, &mut out);
        out
    }

    /// Ends one of the player's shares: the publisher stops, the room hears it (`tell`), the page
    /// hears why (`why`, when the player did not choose it).
    fn close_mine(
        &mut self,
        n: u8,
        why: Option<ShareClosedWhy>,
        tell: bool,
        now: u64,
        out: &mut Vec<Effect>,
    ) {
        let Some(at) = self.share.mine.iter().position(|mine| mine.n == n) else {
            return;
        };
        let mine = self.share.mine.remove(at);
        out.push(Effect::Share(ShareCmd::Stop { n }));
        if tell
            && mine.phase != MinePhase::Preparing
            && let Some(generation) = self.open_generation()
        {
            out.push(Effect::Send {
                generation,
                frame: ToRoom::ShareClose { n },
            });
        }
        self.pending.retain(|_, pending| {
            *pending != Pending::ShareOpen(n) && *pending != Pending::ShareSet(n)
        });
        // The player's own preview of it goes with it.
        let me = self.me.clone();
        if self
            .share
            .watching
            .iter()
            .any(|watch| watch.u == me && watch.n == n)
        {
            self.drop_watch(&me, n, true, out);
        }
        if let Some(why) = why {
            out.push(Effect::ShareNotice(ShareNotice::Closed {
                why,
                src: mine.kind,
            }));
        }
        // SHARE-LIVE: a share that was live ended, whatever the reason (one that never started
        // made no sound to answer).
        if mine.phase == MinePhase::Live {
            self.share_sound(EngineSound::ShareStop, now, out);
        }
        self.sync_privacy(out);
        self.share_touch_idle(now);
        out.push(Effect::Changed);
    }

    /// The publisher's news.
    pub fn share_event(&mut self, event: ShareEvent, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        match event {
            ShareEvent::Offer { n, sdp } => {
                let Some(at) = self
                    .share
                    .mine
                    .iter()
                    .position(|mine| mine.n == n && mine.phase == MinePhase::Preparing)
                else {
                    // A share that ended meanwhile: the publisher lets it go.
                    out.push(Effect::Share(ShareCmd::Stop { n }));
                    return out;
                };
                let Some(generation) = self.open_generation() else {
                    self.close_mine(n, Some(ShareClosedWhy::Error), false, now, &mut out);
                    return out;
                };
                let rid = self.rid();
                self.pending.insert(rid, Pending::ShareOpen(n));
                let mine = &mut self.share.mine[at];
                mine.phase = MinePhase::Opening(rid);
                out.push(Effect::Send {
                    generation,
                    frame: ToRoom::ShareOpen {
                        n,
                        src: mine.kind,
                        audio: mine.opts.audio,
                        preset: mine.opts.preset,
                        fps: mine.opts.fps,
                        sdp,
                        rid,
                    },
                });
            }
            // The panel says "Yayında" from the room's answer; frames and numbers change nothing
            // the page shows (SQOL: the launcher hands the numbers to the page itself).
            ShareEvent::Started { .. } | ShareEvent::Stats { .. } => {}
            ShareEvent::Lost { n, why } => {
                let why = match why {
                    ShareLostWhy::Connect => ShareClosedWhy::Network,
                    ShareLostWhy::Other => ShareClosedWhy::Error,
                };
                self.close_mine(n, Some(why), true, now, &mut out);
            }
            ShareEvent::SourceGone { n } => self.source_gone(n, now, &mut out),
            ShareEvent::Switched { n, .. } => self.switched(n, &mut out),
            ShareEvent::SwitchFailed { n } => {
                if let Some(mine) = self.share.mine.iter_mut().find(|mine| mine.n == n)
                    && let Some(switching) = mine.switching.take()
                {
                    // The launcher's own try (a game it started) says nothing; the wait goes on.
                    if !switching.auto {
                        out.push(Effect::ShareNotice(ShareNotice::SwitchFailed { n }));
                    }
                    out.push(Effect::Changed);
                }
            }
            ShareEvent::Minimized { n, on } => {
                if let Some(mine) = self.share.mine.iter_mut().find(|mine| mine.n == n)
                    && mine.minimized != on
                {
                    mine.minimized = on;
                    self.sync_st(n, &mut out);
                    out.push(Effect::Changed);
                }
            }
            ShareEvent::AudioUnavailable { n } => {
                if let Some(mine) = self.share.mine.iter_mut().find(|mine| mine.n == n) {
                    // SQOL-R3: the room's answers keep it off from now on.
                    mine.audio_failed = true;
                    if mine.opts.audio {
                        mine.opts.audio = false;
                        out.push(Effect::ShareNotice(ShareNotice::AudioOff));
                        out.push(Effect::Changed);
                    }
                }
            }
            // SHARE-LIVE: a live share's still picture goes to the room (a `live1` room keeps it
            // for member lists; an older one answers `bad_request`, which nobody waits for).
            ShareEvent::Thumb { n, jpg } => {
                // SQOL: only while "Önizleme Resmini Paylaş" is on for it.
                let live = self
                    .share
                    .mine
                    .iter()
                    .any(|mine| mine.n == n && mine.phase == MinePhase::Live && mine.thumbs);
                let jpeg = jpg.0.len() <= THUMB_MAX_BYTES && jpg.0.starts_with(&[0xFF, 0xD8, 0xFF]);
                if live
                    && jpeg
                    && let Some(generation) = self.open_generation()
                {
                    out.push(Effect::Send {
                        generation,
                        frame: ToRoom::ShareThumb {
                            n,
                            jpg: base64::engine::general_purpose::STANDARD.encode(&jpg.0),
                        },
                    });
                }
            }
        }
        out
    }

    // ---------------------------------------------------------------------------------------
    // The room's share frames

    /// Takes the frames only sharing reads (and the answers to its own requests); hands back
    /// everything else.
    pub(super) fn share_message(
        &mut self,
        message: FromRoom,
        now: u64,
        out: &mut Vec<Effect>,
    ) -> Option<FromRoom> {
        self.share.clock = now;
        match message {
            FromRoom::Watchers { n, ids } => {
                let me = self.me.clone();
                let generation = self.open_generation();
                if let Some(mine) = self.share.mine.iter_mut().find(|mine| mine.n == n) {
                    let mut ids: Vec<String> = ids.into_iter().filter(|id| *id != me).collect();
                    ids.truncate(50);
                    let came = ids.iter().any(|id| !mine.watchers.contains(id));
                    mine.watchers = ids;
                    if mine.watchers.is_empty() {
                        mine.alone_since.get_or_insert(now);
                    } else {
                        mine.alone_since = None;
                        if mine.paused {
                            mine.paused = false;
                            out.push(Effect::Share(ShareCmd::Pause { n, paused: false }));
                            if let Some(generation) = generation {
                                out.push(Effect::Send {
                                    generation,
                                    frame: ToRoom::share_paused(n, false),
                                });
                            }
                        }
                        if came {
                            out.push(Effect::Share(ShareCmd::KeyFrame { n }));
                        }
                    }
                    out.push(Effect::Changed);
                }
                None
            }
            FromRoom::Woffer { sdp, mids, fresh } => {
                if self.share.watching.is_empty() || self.share.held {
                    // Nothing asked for on this page now (an offer that crossed an unwatch).
                    return None;
                }
                let mids: Vec<WatchMid> = mids
                    .into_iter()
                    .map(|(mid, wire)| WatchMid {
                        mid,
                        person: wire.u,
                        n: wire.n,
                        video: wire.k != "a",
                    })
                    .collect();
                for watch in &mut self.share.watching {
                    if mids
                        .iter()
                        .any(|mid| mid.person == watch.u && mid.n == watch.n)
                    {
                        watch.rid = None;
                        watch.asked_at = None;
                    }
                }
                self.deliver(
                    WatchCmd::Offer {
                        sdp: sdp.into_inner(),
                        mids,
                        fresh,
                    },
                    out,
                );
                out.push(Effect::Changed);
                None
            }
            FromRoom::ShareClosed { n, why } => {
                self.close_mine(n, Some(why), false, now, out);
                None
            }
            FromRoom::Ack {
                rid: Some(rid),
                sdp,
                ice,
                preset,
                fps,
                audio,
                ..
            } if self.pending.get(&rid).is_some_and(Pending::is_share) => {
                match self.pending.remove(&rid) {
                    Some(Pending::ShareOpen(n)) => match sdp {
                        Some(sdp) => {
                            if let Some(mine) = self.share.mine.iter_mut().find(|mine| mine.n == n)
                            {
                                mine.phase = MinePhase::Live;
                                mine.alone_since = Some(now);
                                out.push(Effect::Share(ShareCmd::Answer { n, sdp }));
                                // The room allowed less (the month's budget): the share goes on
                                // at that.
                                if let Some(allowed) = preset
                                    && allowed < mine.opts.preset
                                {
                                    mine.opts.preset = allowed;
                                    if allowed != SharePreset::High {
                                        mine.opts.fps = mine.opts.fps.min(30);
                                    }
                                    let opts = mine.opts;
                                    out.push(Effect::Share(ShareCmd::Update { n, opts }));
                                }
                                // SHARE-LIVE: the room took it: the player is live.
                                self.share_sound(EngineSound::ShareStart, now, out);
                                // SQOL: a share whose picture held before the room took it.
                                self.sync_st(n, out);
                                out.push(Effect::Changed);
                            }
                        }
                        None => self.close_mine(n, Some(ShareClosedWhy::Error), true, now, out),
                    },
                    // SQOL: the room's applied values of a live change.
                    Some(Pending::ShareSet(n)) => self.edited(n, rid, preset, fps, audio, out),
                    Some(Pending::WatchTurn) => match ice {
                        Some(ice) if !ice.urls.is_empty() => {
                            // MOB-WATCH: it serves the session's new connections until shortly
                            // before it runs out.
                            self.share.turn_until = (ice.ttl > 0).then(|| {
                                now + ice.ttl.saturating_mul(1_000).saturating_sub(TURN_SPARE_MS)
                            });
                            self.deliver(
                                WatchCmd::Ice {
                                    ice: crate::engine::EngineIce {
                                        urls: ice.urls,
                                        username: ice.username,
                                        credential: ice.credential,
                                    },
                                },
                                out,
                            );
                        }
                        _ => self.watch_lost(now, out),
                    },
                    // A watch's offer follows its `ack` (if the room sends one at all).
                    _ => {}
                }
                None
            }
            FromRoom::Error {
                rid: Some(rid),
                code,
                ..
            } if self.pending.get(&rid).is_some_and(Pending::is_share) => {
                let refusal = ShareRefusal::from_code(&code);
                match self.pending.remove(&rid) {
                    Some(Pending::ShareOpen(n)) => {
                        self.close_mine(n, None, false, now, out);
                        out.push(Effect::ShareNotice(ShareNotice::Refused { refusal }));
                    }
                    // SQOL: a live change the room refused: nothing changes. The budget and the
                    // room's rule are told; a room that does not take changes yet stays quiet.
                    Some(Pending::ShareSet(n)) => {
                        if let Some(mine) = self.share.mine.iter_mut().find(|mine| mine.n == n)
                            && mine.editing.is_some_and(|(asked, _)| asked == rid)
                        {
                            mine.editing = None;
                            mine.asked = Asked::default();
                        }
                        if matches!(refusal, ShareRefusal::Budget | ShareRefusal::NotAllowed) {
                            out.push(Effect::ShareNotice(ShareNotice::Refused { refusal }));
                        }
                        out.push(Effect::Changed);
                    }
                    Some(Pending::Watch(u, n)) => {
                        self.drop_watch(&u, n, false, out);
                        out.push(Effect::ShareNotice(ShareNotice::Refused { refusal }));
                    }
                    Some(Pending::WatchTurn) => self.watch_lost(now, out),
                    _ => {}
                }
                None
            }
            other => Some(other),
        }
    }

    /// What a voice message said about shares, after the machine handled it (`again`: a `hello`
    /// after the stay's first, a socket that came back).
    pub(super) fn share_seen(&mut self, seen: Seen, again: bool, now: u64, out: &mut Vec<Effect>) {
        match seen {
            Seen::Nothing => {}
            Seen::Hello {
                share_by,
                watch,
                people,
            } => {
                let on = share_by.is_some();
                self.share.room_seen = on;
                self.share.share_by = share_by.unwrap_or_default();
                self.share.clock = now;
                let me = self.me.clone();
                let mut mine_listed: Vec<ShareWire> = Vec::new();
                self.share.remote.clear();
                for (u, sh) in people {
                    if u == me {
                        mine_listed = sh;
                    } else if !sh.is_empty() {
                        self.share.remote.insert(u, sh);
                    }
                }
                if !on {
                    // Sharing went off (Studio, the budget, an older room after a deploy).
                    let ns: Vec<u8> = self.share.mine.iter().map(|mine| mine.n).collect();
                    for n in ns {
                        self.close_mine(n, Some(ShareClosedWhy::Limit), false, now, out);
                    }
                    self.stop_watching(None, out);
                } else if again {
                    self.resume_sharing(&mine_listed, now, out);
                    // SHARE-LIVE: catching up with the room says nothing (no sounds).
                    self.prune_watches(false, now, out);
                    self.resume_watching(watch.unwrap_or_default(), now, out);
                }
                self.mine_pictures(&mine_listed);
                out.push(Effect::Changed);
            }
            Seen::ShareBy(by) => {
                if self.share.share_by != by {
                    self.share.share_by = by;
                    out.push(Effect::Changed);
                }
            }
            Seen::Person(u, sh) => {
                if u == self.me {
                    // SHARE-LIVE: the room's time of the player's own pictures.
                    if self.mine_pictures(&sh) {
                        out.push(Effect::Changed);
                    }
                    return;
                }
                // SHARE-LIVE: a share the room did not list for them before (a new `n`, or the
                // same `n` begun anew) is someone starting to share.
                let started = {
                    let before = self.share.remote.get(&u);
                    sh.iter().any(|share| {
                        !before.is_some_and(|old| {
                            old.iter().any(|was| was.n == share.n && was.at == share.at)
                        })
                    })
                };
                if sh.is_empty() {
                    self.share.remote.remove(&u);
                } else {
                    self.share.remote.insert(u, sh);
                }
                if started {
                    self.share_sound(EngineSound::StreamStart, now, out);
                }
                self.prune_watches(true, now, out);
                out.push(Effect::Changed);
            }
            Seen::Left(u) => {
                self.share.remote.remove(&u);
                for mine in &mut self.share.mine {
                    mine.watchers.retain(|id| *id != u);
                }
                self.prune_watches(true, now, out);
                out.push(Effect::Changed);
            }
        }
    }

    /// SHARE-LIVE: the room's `th` of the player's own shares (`listed`); whether one changed.
    fn mine_pictures(&mut self, listed: &[ShareWire]) -> bool {
        let mut changed = false;
        for mine in &mut self.share.mine {
            if let Some(th) = listed
                .iter()
                .find(|share| share.n == mine.n)
                .and_then(|share| share.th)
                && mine.th != Some(th)
            {
                mine.th = Some(th);
                changed = true;
            }
        }
        changed
    }

    /// Back on a new socket, the player's shares against the room's list of them (`listed`, with
    /// how many watch each): a share the room no longer lists ended meanwhile; one whose
    /// `share_open` waited has no answer to come (it ends, the room's copy too); one the room
    /// lists that the player no longer has is closed there; frames lost meanwhile (`watchers`)
    /// are made up from the counts.
    fn resume_sharing(&mut self, listed: &[ShareWire], now: u64, out: &mut Vec<Effect>) {
        let ends: Vec<(u8, bool)> = self
            .share
            .mine
            .iter()
            .filter_map(|mine| {
                let there = listed.iter().any(|share| share.n == mine.n);
                match mine.phase {
                    MinePhase::Live if !there => Some((mine.n, false)),
                    MinePhase::Opening(_) => Some((mine.n, there)),
                    _ => None,
                }
            })
            .collect();
        let ended: Vec<u8> = ends.iter().map(|(n, _)| *n).collect();
        for (n, tell) in ends {
            self.close_mine(n, Some(ShareClosedWhy::Error), tell, now, out);
        }
        let generation = self.open_generation();
        // SQOL: shares whose source changed kind while the socket was away (the room's evidence).
        let mut kinds: Vec<(u8, ShareKind)> = Vec::new();
        for share in listed {
            if ended.contains(&share.n) {
                continue;
            }
            if !self.share.mine.iter().any(|mine| mine.n == share.n) {
                if let Some(generation) = generation {
                    out.push(Effect::Send {
                        generation,
                        frame: ToRoom::ShareClose { n: share.n },
                    });
                }
                continue;
            }
            let Some(mine) = self.share.mine.iter_mut().find(|mine| mine.n == share.n) else {
                continue;
            };
            if mine.kind != share.src {
                kinds.push((share.n, mine.kind));
            }
            // SQOL: what the room believes watchers see; told again below when it differs.
            mine.told = share.st;
            if share.w == 0 {
                mine.alone_since.get_or_insert(now);
            } else {
                mine.alone_since = None;
                if mine.paused {
                    mine.paused = false;
                    out.push(Effect::Share(ShareCmd::Pause {
                        n: share.n,
                        paused: false,
                    }));
                    if let Some(generation) = generation {
                        out.push(Effect::Send {
                            generation,
                            frame: ToRoom::share_paused(share.n, false),
                        });
                    }
                }
                out.push(Effect::Share(ShareCmd::KeyFrame { n: share.n }));
            }
        }
        let ns: Vec<u8> = self.share.mine.iter().map(|mine| mine.n).collect();
        for n in ns {
            self.sync_st(n, out);
        }
        if self.share_edit()
            && let Some(generation) = generation
        {
            for (n, src) in kinds {
                let rid = self.rid();
                self.pending.insert(rid, Pending::ShareSet(n));
                out.push(Effect::Send {
                    generation,
                    frame: ToRoom::ShareSet {
                        n,
                        rid: Some(rid),
                        paused: None,
                        src: Some(src),
                        audio: None,
                        preset: None,
                        fps: None,
                        st: None,
                    },
                });
            }
        }
    }

    /// Back on a new socket: the room's watch session still carries `kept` (its `hello.me.watch`).
    /// Those whose offer came go on as they are; the rest are asked for again; with none of them
    /// carried the session is new (the page's connection too). What the room carries that the
    /// player no longer watches (an `unwatch` lost meanwhile, or all of it while held), or not as
    /// the player left it (a `watch` whose offer went to the old socket, MOB-WATCH), is let go
    /// there. A connection that waited for TURN has no answer to come (only the socket that asked
    /// gets it): the session is new.
    fn resume_watching(&mut self, kept: Vec<WatchWire>, now: u64, out: &mut Vec<Effect>) {
        let held = self.share.held;
        let turn_lost = self
            .pending
            .values()
            .any(|pending| *pending == Pending::WatchTurn);
        let carried: Vec<bool> = self
            .share
            .watching
            .iter()
            .map(|watch| {
                !held
                    && !turn_lost
                    && watch.sent
                    && watch.rid.is_none()
                    && kept.iter().any(|k| k.u == watch.u && k.n == watch.n)
            })
            .collect();
        if let Some(generation) = self.open_generation() {
            for stray in &kept {
                let at = self
                    .share
                    .watching
                    .iter()
                    .position(|watch| watch.u == stray.u && watch.n == stray.n);
                if at.is_some_and(|at| carried[at]) {
                    continue;
                }
                out.push(Effect::Send {
                    generation,
                    frame: ToRoom::Unwatch {
                        u: stray.u.clone(),
                        n: stray.n,
                    },
                });
                if let Some(at) = at {
                    self.share.watching[at].sent = false;
                }
            }
        }
        if self.share.watching.is_empty() || held {
            return;
        }
        if !carried.contains(&true) {
            self.restart_watching(now, out);
            return;
        }
        self.pending
            .retain(|_, pending| !matches!(pending, Pending::Watch(..) | Pending::WatchTurn));
        for (at, carried) in carried.into_iter().enumerate() {
            if !carried {
                self.share.watching[at].sent = false;
                self.send_watch(at, out);
            }
        }
    }

    /// Watches of shares the room no longer lists go (the room closed them already). `sound`
    /// (SHARE-LIVE): the room's news, not a catch-up: someone else's watched share that ended
    /// plays its sound.
    fn prune_watches(&mut self, sound: bool, now: u64, out: &mut Vec<Effect>) {
        let me = self.me.clone();
        let gone: Vec<(String, u8)> = self
            .share
            .watching
            .iter()
            .filter(|watch| {
                if watch.u == me {
                    !self
                        .share
                        .mine
                        .iter()
                        .any(|mine| mine.n == watch.n && mine.phase == MinePhase::Live)
                } else {
                    !self
                        .share
                        .remote
                        .get(&watch.u)
                        .is_some_and(|sh| sh.iter().any(|share| share.n == watch.n))
                }
            })
            .map(|watch| (watch.u.clone(), watch.n))
            .collect();
        let others = gone.iter().any(|(u, _)| *u != me);
        for (u, n) in gone {
            self.drop_watch(&u, n, false, out);
        }
        if sound && others {
            self.share_sound(EngineSound::StreamEnd, now, out);
        }
    }

    /// The stay ended: every share stops, nothing is watched, the "Yayın" window closes.
    pub(super) fn share_end(&mut self, out: &mut Vec<Effect>) {
        for mine in std::mem::take(&mut self.share.mine) {
            out.push(Effect::Share(ShareCmd::Stop { n: mine.n }));
        }
        if !self.share.watching.is_empty()
            || self.share.connected
            || self.share.pending_offer.is_some()
        {
            out.push(Effect::Watch {
                surface: self.share.surface,
                cmd: WatchCmd::Reset { keep_turn: false },
            });
        }
        if self.share.popout {
            out.push(Effect::SharePopout(false));
        }
        self.pending.retain(|_, pending| !pending.is_share());
        let kept = Shares {
            service: self.share.service,
            publisher: self.share.publisher,
            audio: self.share.audio,
            high: self.share.high,
            attached: self.share.attached,
            visible: self.share.visible,
            privacy: self.share.privacy,
            ..Shares::default()
        };
        self.share = kept;
        self.sync_privacy(out);
    }

    // ---------------------------------------------------------------------------------------
    // Watching

    /// "İzle".
    pub fn share_watch(&mut self, u: &str, n: u8, now: u64) -> Result<Vec<Effect>, ShareRefusal> {
        if !self.share_on() {
            return Err(ShareRefusal::Off);
        }
        let own = u == self.me;
        if !own && !self.share.service.is_some_and(|service| service.watch) {
            return Err(ShareRefusal::Budget);
        }
        let exists = if own {
            self.share
                .mine
                .iter()
                .any(|mine| mine.n == n && mine.phase == MinePhase::Live)
        } else {
            self.share
                .remote
                .get(u)
                .is_some_and(|sh| sh.iter().any(|share| share.n == n))
        };
        if !exists {
            return Err(ShareRefusal::Gone);
        }
        if self
            .share
            .watching
            .iter()
            .any(|watch| watch.u == u && watch.n == n)
        {
            return Ok(Vec::new());
        }
        let others = self
            .share
            .watching
            .iter()
            .filter(|watch| watch.u != self.me)
            .count();
        if !own && others >= WATCH_MAX {
            return Err(ShareRefusal::WatchMax);
        }
        let mut out = Vec::new();
        self.share.clock = now;
        self.share.watching.push(Watching {
            u: u.to_owned(),
            n,
            // The player's own preview is small; others start big (the page asks for less).
            layer: if own { ShareLayer::Q } else { ShareLayer::F },
            sent: false,
            rid: None,
            asked_at: None,
        });
        if self.share.watching.len() == 1 {
            self.share.losses = 0;
        }
        self.share.touched = now;
        self.share.asking_since = None;
        if !self.share.visible[slot(self.share.surface)]
            && self.share.hidden_since.is_none()
            && !self.share.held
        {
            self.share.hidden_since = Some(now);
        }
        let last = self.share.watching.len() - 1;
        if self.watch_may_ask() {
            self.send_watch(last, &mut out);
        }
        self.share_touch_idle(now);
        out.push(Effect::Changed);
        Ok(out)
    }

    /// "İzlemeyi bırak" on one share.
    pub fn share_unwatch(&mut self, u: &str, n: u8, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        if self
            .share
            .watching
            .iter()
            .any(|watch| watch.u == u && watch.n == n)
        {
            self.drop_watch(u, n, true, &mut out);
            self.share_touch_idle(now);
        }
        out
    }

    /// The page wants another layer for a share (its tile grew or shrank).
    pub fn share_layer(&mut self, u: &str, n: u8, layer: ShareLayer) -> Vec<Effect> {
        let generation = self.open_generation();
        let held = self.share.held;
        let Some(watch) = self
            .share
            .watching
            .iter_mut()
            .find(|watch| watch.u == u && watch.n == n)
        else {
            return Vec::new();
        };
        if watch.layer == layer {
            return Vec::new();
        }
        watch.layer = layer;
        let mut out = Vec::new();
        if watch.sent
            && !held
            && let Some(generation) = generation
        {
            out.push(Effect::Send {
                generation,
                frame: ToRoom::Layer {
                    u: u.to_owned(),
                    n,
                    layer,
                },
            });
        }
        // SQOL-R3: the page reads the layer back from the state (its tiles chose by it: a stale one
        // kept the stream on the small layer after "Veri Tasarrufu" was left, the real-SFU run).
        out.push(Effect::Changed);
        out
    }

    /// A share leaves the player's watch list: `tell` sends `unwatch` (the room closed it
    /// already otherwise). The last one closes the page's connection.
    fn drop_watch(&mut self, u: &str, n: u8, tell: bool, out: &mut Vec<Effect>) {
        let Some(at) = self
            .share
            .watching
            .iter()
            .position(|watch| watch.u == u && watch.n == n)
        else {
            return;
        };
        let watch = self.share.watching.remove(at);
        if tell
            && watch.sent
            && let Some(generation) = self.open_generation()
        {
            out.push(Effect::Send {
                generation,
                frame: ToRoom::Unwatch {
                    u: watch.u.clone(),
                    n,
                },
            });
        }
        self.pending
            .retain(|_, pending| *pending != Pending::Watch(watch.u.clone(), n));
        if self.share.watching.is_empty() {
            self.reset_page(false, out);
            self.share.asking_since = None;
            self.share.hidden_since = None;
            self.share.held = false;
        }
        out.push(Effect::Changed);
    }

    /// Nothing more is watched: every share leaves the list (`notice`: why, for the page).
    fn stop_watching(&mut self, notice: Option<ShareNotice>, out: &mut Vec<Effect>) {
        if self.share.watching.is_empty() {
            return;
        }
        self.unwatch_room(out);
        self.share.watching.clear();
        self.reset_page(false, out);
        self.share.asking_since = None;
        self.share.hidden_since = None;
        self.share.held = false;
        if let Some(notice) = notice {
            out.push(Effect::ShareNotice(notice));
        }
        out.push(Effect::Changed);
    }

    /// Watching may ask the room now: the page that watches is there and on screen.
    fn watch_may_ask(&self) -> bool {
        !self.share.held && self.share.attached[slot(self.share.surface)]
    }

    fn send_watch(&mut self, at: usize, out: &mut Vec<Effect>) {
        let Some(generation) = self.open_generation() else {
            return;
        };
        let rid = self.rid();
        let watch = &mut self.share.watching[at];
        self.pending
            .insert(rid, Pending::Watch(watch.u.clone(), watch.n));
        watch.sent = true;
        watch.rid = Some(rid);
        watch.asked_at = Some(self.share.clock);
        out.push(Effect::Send {
            generation,
            frame: ToRoom::Watch {
                u: watch.u.clone(),
                n: watch.n,
                layer: watch.layer,
                rid,
            },
        });
    }

    /// Lets the room's watch session go (every share it carries); the list stays.
    fn unwatch_room(&mut self, out: &mut Vec<Effect>) {
        let generation = self.open_generation();
        for watch in &mut self.share.watching {
            if watch.sent
                && let Some(generation) = generation
            {
                out.push(Effect::Send {
                    generation,
                    frame: ToRoom::Unwatch {
                        u: watch.u.clone(),
                        n: watch.n,
                    },
                });
            }
            watch.sent = false;
            watch.rid = None;
            watch.asked_at = None;
        }
        self.pending
            .retain(|_, pending| !matches!(pending, Pending::Watch(..) | Pending::WatchTurn));
    }

    /// Asks again for every share on the list (a new session, from the page that watches now).
    fn rewatch(&mut self, out: &mut Vec<Effect>) {
        if !self.watch_may_ask() {
            return;
        }
        for at in 0..self.share.watching.len() {
            if !self.share.watching[at].sent {
                self.send_watch(at, out);
            }
        }
    }

    /// The session anew: the room lets the old go, the page closes its connection (keeping TURN
    /// it was given while that still serves, MOB-WATCH), the list is asked for again.
    fn restart_watching(&mut self, now: u64, out: &mut Vec<Effect>) {
        self.unwatch_room(out);
        let keep_turn = self.share.turn_until.is_none_or(|until| now < until);
        self.reset_page(keep_turn, out);
        self.rewatch(out);
    }

    /// The watching page closes its connection (`keep_turn`: TURN it has stays for the next one).
    fn reset_page(&mut self, keep_turn: bool, out: &mut Vec<Effect>) {
        self.share.pending_offer = None;
        self.share.connected = false;
        if !keep_turn {
            self.share.turn_until = None;
        }
        if self.share.attached[slot(self.share.surface)] {
            out.push(Effect::Watch {
                surface: self.share.surface,
                cmd: WatchCmd::Reset { keep_turn },
            });
        }
    }

    /// To the page that watches, or kept until it attaches.
    fn deliver(&mut self, cmd: WatchCmd, out: &mut Vec<Effect>) {
        if self.share.attached[slot(self.share.surface)] {
            out.push(Effect::Watch {
                surface: self.share.surface,
                cmd,
            });
        } else {
            self.share.pending_offer = Some(cmd);
        }
    }

    /// A page that watches attached its channel (`share_watch_attach`): a fresh page has no
    /// connection, so the session starts anew there.
    pub fn share_attach(&mut self, surface: ShareSurface, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        self.share.clock = now;
        self.share.attached[slot(surface)] = true;
        if surface != self.share.surface {
            return out;
        }
        if let Some(cmd) = self.share.pending_offer.take() {
            out.push(Effect::Watch { surface, cmd });
            return out;
        }
        if !self.share.watching.is_empty() && !self.share.held {
            self.restart_watching(now, &mut out);
            out.push(Effect::Changed);
        }
        out
    }

    /// A page that watched went away (the "Yayın" window closed, a page reloaded): watching moves
    /// back to the launcher's window.
    pub fn share_detach(&mut self, surface: ShareSurface, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        self.share.clock = now;
        self.share.attached[slot(surface)] = false;
        if surface == ShareSurface::Popout {
            self.share.popout = false;
            if self.share.surface == ShareSurface::Popout {
                self.move_watching(ShareSurface::Main, now, &mut out);
            }
        } else if surface == self.share.surface && !self.share.watching.is_empty() {
            self.unwatch_room(&mut out);
            self.share.connected = false;
            self.share.pending_offer = None;
            // SQOL-R3: what watching shows changed (it waits for the page again).
            out.push(Effect::Changed);
        }
        out
    }

    /// "Ayrı pencerede aç" (`open`) or back into the launcher's window.
    pub fn share_popout(&mut self, open: bool, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        self.share.clock = now;
        if !open && !self.share.popout {
            return out;
        }
        if open {
            self.share.popout = true;
            // Already there: the window comes to the front.
            out.push(Effect::SharePopout(true));
            if self.share.surface != ShareSurface::Popout {
                self.move_watching(ShareSurface::Popout, now, &mut out);
            }
        } else {
            self.share.popout = false;
            out.push(Effect::SharePopout(false));
            if self.share.surface == ShareSurface::Popout {
                self.move_watching(ShareSurface::Main, now, &mut out);
            }
        }
        out.push(Effect::Changed);
        out
    }

    fn move_watching(&mut self, to: ShareSurface, now: u64, out: &mut Vec<Effect>) {
        self.unwatch_room(out);
        self.reset_page(false, out);
        self.share.surface = to;
        self.share.hidden_since = None;
        self.share.touched = now;
        // A window out of sight (the launcher minimized when the "Yayın" window closed) holds
        // until it is back; a new window counts as on screen until it says otherwise.
        if to == ShareSurface::Popout {
            self.share.visible[slot(to)] = true;
        }
        self.share.held = !self.share.visible[slot(to)] && !self.share.watching.is_empty();
        self.rewatch(out);
        out.push(Effect::Changed);
    }

    /// The page that watches went out of sight (minimized, in the tray, the game in front) or
    /// came back.
    pub fn share_visible(&mut self, surface: ShareSurface, visible: bool, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        self.share.clock = now;
        if self.share.visible[slot(surface)] == visible {
            return out;
        }
        self.share.visible[slot(surface)] = visible;
        if surface != self.share.surface {
            return out;
        }
        if visible {
            self.share.hidden_since = None;
            if self.share.held {
                self.share.held = false;
                self.share.touched = now;
                self.share.asking_since = None;
                self.rewatch(&mut out);
                out.push(Effect::Changed);
            }
        } else if !self.share.watching.is_empty() && !self.share.held {
            self.share.hidden_since = Some(now);
        }
        out
    }

    /// The watching page's report.
    pub fn share_watch_report(
        &mut self,
        surface: ShareSurface,
        report: WatchReport,
        now: u64,
    ) -> Vec<Effect> {
        let mut out = Vec::new();
        self.share.clock = now;
        if surface != self.share.surface || report.is_oversized() {
            return out;
        }
        match report {
            WatchReport::Answer { sdp } => {
                if let Some(generation) = self.open_generation() {
                    out.push(Effect::Send {
                        generation,
                        frame: ToRoom::Wanswer {
                            sdp: crate::wire::Sdp::new(sdp),
                        },
                    });
                }
            }
            WatchReport::NeedTurn => {
                match self.request(Pending::WatchTurn, |rid| ToRoom::Turn { rid }) {
                    Ok((_, effects)) => out.extend(effects),
                    // No socket to ask on: as lost (the page waits for nothing else; the
                    // socket's return asks anew).
                    Err(_) => self.watch_lost(now, &mut out),
                }
            }
            WatchReport::Connected => {
                if !self.share.connected {
                    self.share.connected = true;
                    self.share.losses = 0;
                    out.push(Effect::Changed);
                }
            }
            WatchReport::Lost => self.watch_lost(now, &mut out),
        }
        out
    }

    /// The watch connection is gone: asked for anew, a few times in a row at most.
    fn watch_lost(&mut self, now: u64, out: &mut Vec<Effect>) {
        if self.share.watching.is_empty() {
            return;
        }
        self.share.connected = false;
        self.share.losses += 1;
        if self.share.losses >= WATCH_LOSSES_MAX {
            self.stop_watching(Some(ShareNotice::WatchFailed), out);
        } else {
            self.restart_watching(now, out);
            out.push(Effect::Changed);
        }
    }

    /// "Evet" on "Hâlâ izliyor musun?" (any touch of the page's stage too).
    pub fn share_still(&mut self, now: u64) -> Vec<Effect> {
        self.share.touched = now;
        if self.share.asking_since.take().is_some() {
            vec![Effect::Changed]
        } else {
            Vec::new()
        }
    }

    /// A sharer's stream sound for the player (percent 0 to `volume_max`, SQOL: up to 200; muted).
    pub fn set_stream_volume(&mut self, person: &str, volume: u16, muted: bool) -> Vec<Effect> {
        let volume = volume.min(self.playout().max_percent());
        let changed = self.store.set_stream(person, volume, muted);
        if changed {
            vec![Effect::Changed]
        } else {
            Vec::new()
        }
    }

    /// "Paylaşımı durdur" on someone else's share (a leader or moderator; the room checks).
    pub fn share_unshare(
        &mut self,
        person: &str,
        n: u8,
    ) -> Result<(u32, Vec<Effect>), VoiceAnswer> {
        if !self.share_on() {
            return Err(VoiceAnswer::refused(VoiceRefusal::Off));
        }
        self.request(Pending::Unshare, |rid| ToRoom::Mod {
            act: ModAct::Unshare,
            to: Some(person.to_owned()),
            room: None,
            name: None,
            n: Some(n),
            who: None,
            rid,
        })
    }

    /// "Kim paylaşabilir" in a friend room (its leader; the room checks).
    pub fn share_set_sharers(&mut self, who: ShareBy) -> Result<(u32, Vec<Effect>), VoiceAnswer> {
        if !self.share_on() {
            return Err(VoiceAnswer::refused(VoiceRefusal::Off));
        }
        self.request(Pending::Mod, |rid| ToRoom::Mod {
            act: ModAct::Sharers,
            to: None,
            room: None,
            name: None,
            n: None,
            who: Some(who),
            rid,
        })
    }

    // ---------------------------------------------------------------------------------------
    // SQOL: live changes of the player's own shares

    /// A live share of the player's that changes now (`share_switch`, `share_update`,
    /// `share_hold`), or why not.
    fn editable(&self, n: u8) -> Result<usize, ShareRefusal> {
        if !self.share_on() || !self.share_edit() {
            return Err(ShareRefusal::Off);
        }
        let at = self
            .share
            .mine
            .iter()
            .position(|mine| mine.n == n)
            .ok_or(ShareRefusal::Gone)?;
        if self.share.mine[at].phase != MinePhase::Live {
            return Err(ShareRefusal::Failed);
        }
        Ok(at)
    }

    /// "Ekranı Değiştir": share `source` in share `n` instead, the watchers staying (`label`: the
    /// window's title or the screen's name, this computer only). The publisher answers.
    pub fn share_switch(
        &mut self,
        n: u8,
        source: ShareSource,
        label: &str,
    ) -> Result<Vec<Effect>, ShareRefusal> {
        let at = self.editable(n)?;
        if !self.share.publisher {
            return Err(ShareRefusal::Unsupported);
        }
        if !source.is_plausible() {
            return Err(ShareRefusal::Failed);
        }
        if self
            .share
            .mine
            .iter()
            .any(|mine| mine.n != n && mine.source == source)
        {
            return Err(ShareRefusal::InUse);
        }
        let mine = &mut self.share.mine[at];
        if mine.source == source && mine.hold.is_none() {
            return Ok(Vec::new());
        }
        // SQOL-R3: the same source asked again while its switch is on its way (a double click in
        // the picker): one switch.
        if mine
            .switching
            .as_ref()
            .is_some_and(|switching| switching.source == source)
        {
            return Ok(Vec::new());
        }
        let label = match source.kind() {
            ShareKind::Game => String::new(),
            _ => crate::share::clip_label(label),
        };
        mine.switching = Some(Switching {
            source: source.clone(),
            label,
            auto: false,
        });
        Ok(vec![
            Effect::Share(ShareCmd::Switch { n, source }),
            Effect::Changed,
        ])
    }

    /// The publisher shows the new source: the share is that now ("Artık … paylaşılıyor."). Unasked
    /// (the game recreated its window), the share just goes on.
    fn switched(&mut self, n: u8, out: &mut Vec<Effect>) {
        let edit = self.share_edit();
        let system = self.store.settings().share.audio_source == ShareAudioSource::System;
        let generation = self.open_generation();
        let Some(at) = self.share.mine.iter().position(|mine| mine.n == n) else {
            return;
        };
        let mine = &mut self.share.mine[at];
        mine.minimized = false;
        let Some(switching) = mine.switching.take() else {
            self.sync_st(n, out);
            out.push(Effect::Changed);
            return;
        };
        let was = mine.kind;
        mine.kind = switching.source.kind();
        mine.source = switching.source;
        mine.label = switching.label.clone();
        // Another source may have sound to share.
        mine.audio_failed = false;
        // A closed source's wait is over; a pause the player chose stays (SQOL-R3: also one
        // chosen before the source closed: the publisher still holds, nothing is sent).
        if matches!(
            mine.hold,
            Some(ShareHoldWhy::Closed | ShareHoldWhy::GameClosed)
        ) {
            mine.hold_until = None;
            if mine.player_hold {
                mine.hold = Some(ShareHoldWhy::Paused);
            } else {
                mine.hold = None;
                out.push(Effect::Share(ShareCmd::Hold { n, on: false }));
            }
        }
        // "Görüntü" and the sound follow the new source (a game moves; a screen's sound is the
        // whole computer's).
        let mut opts = mine.opts;
        opts.motion = motion_of(mine.motion, mine.kind);
        opts.audio_system = mine.kind == ShareKind::Screen || system;
        if opts != mine.opts {
            mine.opts = opts;
            out.push(Effect::Share(ShareCmd::Update { n, opts }));
        }
        let src = mine.kind;
        out.push(Effect::ShareNotice(ShareNotice::Switched {
            n,
            src,
            label: switching.label,
        }));
        // The room keeps what is shared (its evidence for a report) and shows its kind.
        if edit
            && src != was
            && let Some(generation) = generation
        {
            let rid = self.rid();
            self.pending.insert(rid, Pending::ShareSet(n));
            out.push(Effect::Send {
                generation,
                frame: ToRoom::ShareSet {
                    n,
                    rid: Some(rid),
                    paused: None,
                    src: Some(src),
                    audio: None,
                    preset: None,
                    fps: None,
                    st: None,
                },
            });
        }
        self.sync_st(n, out);
        out.push(Effect::Changed);
    }

    /// The shared window closed or the game ended: the share holds a blank picture a minute for
    /// another source (a share the room cannot tell watchers about ends as before).
    fn source_gone(&mut self, n: u8, now: u64, out: &mut Vec<Effect>) {
        let edit = self.share_edit();
        let Some(mine) = self.share.mine.iter_mut().find(|mine| mine.n == n) else {
            return;
        };
        if !edit || mine.phase != MinePhase::Live {
            self.close_mine(n, Some(ShareClosedWhy::SourceGone), true, now, out);
            return;
        }
        mine.minimized = false;
        mine.hold = Some(if mine.kind == ShareKind::Game {
            ShareHoldWhy::GameClosed
        } else {
            ShareHoldWhy::Closed
        });
        mine.hold_until = Some(now + HOLD_STOP_MS);
        let src = mine.kind;
        out.push(Effect::ShareNotice(ShareNotice::SourceClosed { n, src }));
        self.sync_st(n, out);
        out.push(Effect::Changed);
    }

    /// The launcher started a game: a share whose game closed takes it (when its window comes).
    pub fn share_game_started(&mut self) -> Vec<Effect> {
        let shown = self
            .share
            .mine
            .iter()
            .any(|mine| mine.source == ShareSource::Game && mine.hold.is_none());
        if shown {
            return Vec::new();
        }
        let Some(mine) = self.share.mine.iter_mut().find(|mine| {
            mine.phase == MinePhase::Live
                && mine.hold == Some(ShareHoldWhy::GameClosed)
                && mine.switching.is_none()
        }) else {
            return Vec::new();
        };
        mine.switching = Some(Switching {
            source: ShareSource::Game,
            label: String::new(),
            auto: true,
        });
        vec![Effect::Share(ShareCmd::Switch {
            n: mine.n,
            source: ShareSource::Game,
        })]
    }

    /// "Duraklat" (`on`) / "Sürdür": watchers see a blank picture meanwhile. A share whose source
    /// closed has nothing to go on with ("gone": pick another).
    pub fn share_hold(&mut self, n: u8, on: bool) -> Result<Vec<Effect>, ShareRefusal> {
        let at = self.editable(n)?;
        let mine = &mut self.share.mine[at];
        match (on, mine.hold) {
            (true, None) => {
                mine.hold = Some(ShareHoldWhy::Paused);
                mine.player_hold = true;
            }
            (false, Some(ShareHoldWhy::Paused)) => {
                mine.hold = None;
                mine.player_hold = false;
            }
            (false, Some(_)) => return Err(ShareRefusal::Gone),
            (true, Some(_)) | (false, None) => return Ok(Vec::new()),
        }
        let mut out = vec![Effect::Share(ShareCmd::Hold { n, on })];
        self.sync_st(n, &mut out);
        out.push(Effect::Changed);
        Ok(out)
    }

    /// The player's own small live preview of share `n` (its card on screen).
    pub fn share_preview(&mut self, n: u8, on: bool) -> Vec<Effect> {
        if self.share.mine.iter().any(|mine| mine.n == n) {
            vec![Effect::Share(ShareCmd::Preview { n, on })]
        } else {
            Vec::new()
        }
    }

    /// A live change of share `n`: "Görüntü", "İmleci Göster" and "Önizleme Resmini Paylaş" at once;
    /// "Kalite", "Kare Hızı" and "Sesi de Paylaş" asked of the room first (held to the room's and
    /// this computer's best), its applied values used when it answers.
    pub fn share_update(
        &mut self,
        n: u8,
        change: ShareChange,
        now: u64,
    ) -> Result<Vec<Effect>, ShareRefusal> {
        let at = if change.asks_room() {
            self.editable(n)?
        } else {
            if !self.share_on() {
                return Err(ShareRefusal::Off);
            }
            self.share
                .mine
                .iter()
                .position(|mine| mine.n == n)
                .ok_or(ShareRefusal::Gone)?
        };
        if change.audio == Some(true) && !self.share.audio {
            return Err(ShareRefusal::Unsupported);
        }
        let max = self.max_preset();
        let generation = self.open_generation();
        // The room must hear it (its socket is coming back): nothing changes now.
        if change.asks_room() && generation.is_none() {
            return Err(ShareRefusal::Failed);
        }
        let mut out = Vec::new();
        let mine = &mut self.share.mine[at];
        let mut opts = mine.opts;
        if let Some(motion) = change.motion {
            mine.motion = motion;
            opts.motion = motion_of(motion, mine.kind);
        }
        if let Some(cursor) = change.cursor {
            opts.cursor = cursor;
        }
        if let Some(thumbs) = change.thumbs {
            mine.thumbs = thumbs;
        }
        if opts != mine.opts {
            mine.opts = opts;
            out.push(Effect::Share(ShareCmd::Update { n, opts }));
        }
        if change.asks_room() {
            // SQOL-R3: measured against what the share will have once the change still waiting
            // for the room lands: a quick change back (sound on, then off at once) is a change too.
            let waiting = if mine.editing.is_some() {
                mine.asked
            } else {
                Asked::default()
            };
            let (now_preset, now_fps, now_audio) = (
                waiting.preset.unwrap_or(mine.opts.preset),
                waiting.fps.unwrap_or(mine.opts.fps),
                waiting.audio.unwrap_or(mine.opts.audio),
            );
            let preset = change.preset.map(|preset| preset.min(max));
            let target = preset.unwrap_or(now_preset);
            let top = if target == SharePreset::High { 60 } else { 30 };
            // 60 frames only with "Yüksek": a lower quality takes the rate down with it.
            let fps = change
                .fps
                .map(|fps| normal_fps(fps).min(top))
                .or_else(|| (now_fps > top).then_some(top));
            let asked = Asked {
                preset: preset.filter(|preset| *preset != now_preset),
                fps: fps.filter(|fps| *fps != now_fps),
                audio: change.audio.filter(|audio| *audio != now_audio),
            };
            // The player's own "Sesi de Paylaş" tries the sound again.
            if change.audio == Some(true) {
                mine.audio_failed = false;
            }
            if asked != Asked::default()
                && let Some(generation) = generation
            {
                let rid = self.rid();
                self.pending.insert(rid, Pending::ShareSet(n));
                let mine = &mut self.share.mine[at];
                mine.editing = Some((rid, now));
                mine.asked = Asked {
                    preset: asked.preset.or(waiting.preset),
                    fps: asked.fps.or(waiting.fps),
                    audio: asked.audio.or(waiting.audio),
                };
                out.push(Effect::Send {
                    generation,
                    frame: ToRoom::ShareSet {
                        n,
                        rid: Some(rid),
                        paused: None,
                        src: None,
                        audio: asked.audio,
                        preset: asked.preset,
                        fps: asked.fps,
                        st: None,
                    },
                });
            }
        }
        out.push(Effect::Changed);
        Ok(out)
    }

    /// The room's applied values of a live change (`ack`): the share goes on with them (held to
    /// this computer's best).
    fn edited(
        &mut self,
        n: u8,
        rid: u32,
        preset: Option<SharePreset>,
        fps: Option<u8>,
        audio: Option<bool>,
        out: &mut Vec<Effect>,
    ) {
        let max = self.max_preset();
        let can_audio = self.share.audio;
        let Some(mine) = self.share.mine.iter_mut().find(|mine| mine.n == n) else {
            return;
        };
        if mine.editing.is_some_and(|(asked, _)| asked == rid) {
            mine.editing = None;
            mine.asked = Asked::default();
        }
        let mut opts = mine.opts;
        if let Some(preset) = preset {
            opts.preset = preset.min(max);
        }
        if let Some(fps) = fps {
            opts.fps = normal_fps(fps);
        }
        if opts.preset != SharePreset::High {
            opts.fps = opts.fps.min(30);
        }
        // SQOL-R3: sound the publisher found none of stays off whatever the room recorded.
        if let Some(audio) = audio {
            opts.audio = audio && can_audio && !mine.audio_failed;
        }
        if opts != mine.opts {
            mine.opts = opts;
            out.push(Effect::Share(ShareCmd::Update { n, opts }));
        }
        out.push(Effect::Changed);
    }

    // ---------------------------------------------------------------------------------------
    // Time

    /// The moments sharing wants a tick at.
    pub(super) fn share_deadline(&self, last_input: u64, at: &mut impl FnMut(u64)) {
        for mine in &self.share.mine {
            if let Some(since) = mine.alone_since {
                if !mine.paused {
                    at(since + PAUSE_AFTER_MS);
                }
                at(since + ALONE_STOP_MS);
            }
            // SQOL: a closed source's minute, a live change's answer.
            if let Some(until) = mine.hold_until {
                at(until);
            }
            if let Some((_, asked)) = mine.editing {
                at(asked + EDIT_WAIT_MS);
            }
        }
        if let Some(since) = self.share.hidden_since {
            at(since + HIDDEN_GRACE_MS);
        }
        // A `watch` the room has not answered.
        if let Some(asked) = self
            .share
            .watching
            .iter()
            .filter_map(|watch| watch.asked_at)
            .min()
        {
            at(asked + WATCH_OFFER_WAIT_MS);
        }
        if !self.share.watching.is_empty() && !self.share.held {
            match self.share.asking_since {
                Some(asked) => at(asked + STILL_ANSWER_MS),
                None => at(self.share.touched.max(last_input) + STILL_ASK_MS),
            }
        }
    }

    pub(super) fn share_tick(&mut self, now: u64, last_input: u64, out: &mut Vec<Effect>) {
        self.share.clock = now;
        // A `watch` with no offer in time: as a lost connection (asked again, a few times).
        if self.share.watching.iter().any(|watch| {
            watch
                .asked_at
                .is_some_and(|at| now >= at + WATCH_OFFER_WAIT_MS)
        }) {
            self.watch_lost(now, out);
        }
        // Nobody watches: rest, then end.
        let generation = self.open_generation();
        let mut ended = Vec::new();
        for mine in &mut self.share.mine {
            let Some(since) = mine.alone_since else {
                continue;
            };
            if now >= since + ALONE_STOP_MS {
                ended.push(mine.n);
            } else if !mine.paused && now >= since + PAUSE_AFTER_MS {
                mine.paused = true;
                out.push(Effect::Share(ShareCmd::Pause {
                    n: mine.n,
                    paused: true,
                }));
                if let Some(generation) = generation {
                    out.push(Effect::Send {
                        generation,
                        frame: ToRoom::share_paused(mine.n, true),
                    });
                }
                out.push(Effect::Changed);
            }
        }
        for n in ended {
            self.close_mine(n, Some(ShareClosedWhy::Idle), true, now, out);
        }
        // SQOL: a closed source's minute ran out: the share ends ("Paylaşım durdu.").
        let gone: Vec<u8> = self
            .share
            .mine
            .iter()
            .filter(|mine| mine.hold_until.is_some_and(|until| now >= until))
            .map(|mine| mine.n)
            .collect();
        for n in gone {
            self.close_mine(n, Some(ShareClosedWhy::SourceGone), true, now, out);
        }
        // SQOL: a live change the room never answered changes nothing.
        let pending = &mut self.pending;
        for mine in &mut self.share.mine {
            if let Some((rid, asked)) = mine.editing
                && now >= asked + EDIT_WAIT_MS
            {
                mine.editing = None;
                mine.asked = Asked::default();
                pending.remove(&rid);
                out.push(Effect::Changed);
            }
        }
        // Out of sight long enough: let go.
        if self
            .share
            .hidden_since
            .is_some_and(|since| now >= since + HIDDEN_GRACE_MS)
        {
            self.share.hidden_since = None;
            if !self.share.watching.is_empty() {
                self.share.held = true;
                self.share.asking_since = None;
                self.unwatch_room(out);
                self.reset_page(false, out);
                out.push(Effect::Changed);
            }
        }
        // "Hâlâ izliyor musun?".
        if !self.share.watching.is_empty() && !self.share.held {
            match self.share.asking_since {
                Some(asked) if last_input > asked => {
                    self.share.asking_since = None;
                    self.share.touched = last_input;
                    out.push(Effect::Changed);
                }
                Some(asked) if now >= asked + STILL_ANSWER_MS => {
                    self.stop_watching(Some(ShareNotice::WatchStopped), out);
                }
                Some(_) => {}
                None => {
                    if now >= self.share.touched.max(last_input) + STILL_ASK_MS {
                        self.share.asking_since = Some(now);
                        out.push(Effect::Changed);
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // The page's view

    pub(super) fn share_view(&self, now: u64, unix: i64) -> VoiceShareView {
        let on = self.share_on();
        let hw_max = if self.share.high {
            SharePreset::High
        } else {
            SharePreset::Normal
        };
        // The crew room form's "Kim paylaşabilir", in a room or not.
        let crew_mods: Vec<String> = self
            .crews
            .values()
            .flatten()
            .filter(|room| room.share_mods)
            .map(|room| room.id.clone())
            .collect();
        if !on {
            return VoiceShareView {
                service: self.share.service.is_some(),
                crew_mods,
                can_share: self.share.publisher,
                audio: self.share.audio,
                surface: self.share.surface,
                hw_max,
                ..VoiceShareView::default()
            };
        }
        let room = self.stay.as_ref().and_then(|stay| stay.room.as_ref());
        let order: Vec<String> = room
            .map(|room| {
                room.people_in_order()
                    .into_iter()
                    .map(|person| person.u.clone())
                    .collect()
            })
            .unwrap_or_default();
        let live: Vec<VoiceLiveShare> = order
            .iter()
            .filter_map(|u| self.share.remote.get(u).map(|sh| (u, sh)))
            .flat_map(|(u, sh)| {
                sh.iter().map(move |share| VoiceLiveShare {
                    person: u.clone(),
                    n: share.n,
                    kind: share.src,
                    audio: share.audio,
                    preset: share.preset,
                    fps: share.fps,
                    watchers: share.w,
                    since: share.at,
                    th: share.th,
                    st: share.st,
                })
            })
            .collect();
        let mut volumes = BTreeMap::new();
        let top = self.playout().max_percent();
        for share in &live {
            let (volume, muted) = self.store.stream(&share.person);
            if (volume, muted) != (100, false) {
                volumes.insert(
                    share.person.clone(),
                    VoiceStreamVolume {
                        volume: volume.min(top),
                        muted,
                    },
                );
            }
        }
        let connected = self.share.connected;
        VoiceShareView {
            on,
            service: self.share.service.is_some(),
            crew_mods,
            can_share: self.share.publisher,
            allowed: self.share_allowed() && self.mine_max() > 0,
            open: self.share.service.is_some_and(|service| service.open),
            share_by: self.share.share_by,
            on_top: self.store.settings().share.always_on_top,
            can_set_share_by: self
                .stay
                .as_ref()
                .is_some_and(|stay| stay.kind == crate::wire::RoomKind::Friend)
                && self.my_role() == crate::wire::Role::Leader,
            mine_max: self.mine_max(),
            max_preset: self.max_preset(),
            audio: self.share.audio,
            mine: self
                .share
                .mine
                .iter()
                .map(|mine| VoiceMyShare {
                    n: mine.n,
                    kind: mine.kind,
                    label: mine.label.clone(),
                    phase: match (mine.phase, mine.paused) {
                        (MinePhase::Live, true) => ShareMinePhase::Resting,
                        (MinePhase::Live, false) => ShareMinePhase::Live,
                        _ => ShareMinePhase::Starting,
                    },
                    audio: mine.opts.audio,
                    preset: mine.opts.preset,
                    fps: mine.opts.fps,
                    watchers: mine.watchers.clone(),
                    since: mine.since,
                    th: mine.th,
                    source: mine.source.clone(),
                    st: mine.st(),
                    hold: mine.hold,
                    hold_until: mine.hold_until.map(|until| {
                        unix + i64::try_from(until.saturating_sub(now) / 1_000).unwrap_or(0)
                    }),
                    motion: mine.motion,
                    cursor: mine.opts.cursor,
                    thumbs: mine.thumbs,
                })
                .collect(),
            live,
            watching: self
                .share
                .watching
                .iter()
                .map(|watch| VoiceWatch {
                    person: watch.u.clone(),
                    n: watch.n,
                    layer: watch.layer,
                    phase: if self.share.held {
                        ShareWatchPhase::Held
                    } else if connected && watch.sent && watch.rid.is_none() {
                        ShareWatchPhase::Live
                    } else {
                        ShareWatchPhase::Connecting
                    },
                })
                .collect(),
            surface: self.share.surface,
            still_asking: self.share.asking_since.is_some(),
            volumes,
            names: BTreeMap::new(),
            edit: self.share_edit(),
            hw_max,
        }
    }

    /// The player's shares now (the tray's "Paylaşımı durdur" and the overlay's).
    pub fn sharing(&self) -> Vec<u8> {
        self.share.mine.iter().map(|mine| mine.n).collect()
    }
}
