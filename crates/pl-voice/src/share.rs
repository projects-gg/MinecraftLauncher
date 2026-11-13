//! Screen sharing in a voice room (SHARE, the lead's contract `docs/tasks/share.md` §3, §4, §6):
//! the room socket's share frames (capability [`CAP`]), what the launcher's page sees of it and
//! asks, the publisher's commands and events, and the player's share choices.
//!
//! The machine stays free of I/O: it sends [`ShareCmd`]s to the publisher (`pl-share`: capture,
//! encoding and the SFU session, SHARE-N) and takes its [`ShareEvent`]s back; it sends
//! [`WatchCmd`]s to the page that watches (the main window or the "Yayın" window) and takes its
//! [`WatchReport`]s back. Window titles stay on this computer: a share tells the room only its
//! kind (`game`, `window`, `screen`).
//!
//! The service side may say more than this launcher knows: an unknown share kind reads as a
//! window, an entry that does not read is dropped (never the whole frame).

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::engine::EngineIce;

/// The room socket capability this launcher asks for (`join.caps`, `sync.caps`).
pub const CAP: &str = "share1";
/// SHARE-LIVE (the lead's contract `docs/tasks/share-live.md` §2): asked for with [`CAP`]: the
/// player's live shares send their still picture (`share_thumb`) and the room's share state says
/// when each one's last picture came (`th`).
pub const LIVE_CAP: &str = "live1";
/// SHARE-LIVE: a still picture's bytes at most (the publisher makes at most this much, so the
/// frame stays within the room socket's ordinary frame limit); a larger one is not sent.
pub const THUMB_MAX_BYTES: usize = 12_000;
/// SHARE-LIVE: the same sharing sound plays at most once this often (two shares that start or end
/// together sound once).
pub const SOUND_GAP_MS: u64 = 1_000;
/// SQOL (the lead's contract `docs/tasks/sqol.md` §2.1): asked for with [`CAP`]: live changes of
/// the player's own shares (`share_set` with `rid`: source, quality, frame rate, sound, `st`) and
/// the share state's `st` (`hold`, `min`).
pub const SHARE2_CAP: &str = "share2";
/// SQOL (§2.2): the room's own text chat.
pub const RCHAT_CAP: &str = "rchat1";
/// SQOL: a share whose source closed waits this long for another one ("Başka Ekran Seç"), then
/// ends ("Paylaşım durdu.").
pub const HOLD_STOP_MS: u64 = 60_000;
/// SQOL: "Kısma Miktarı" (percent the stream sound goes down while someone speaks).
pub const DUCK_LEVEL_MIN: u8 = 20;
pub const DUCK_LEVEL_MAX: u8 = 90;
pub const DUCK_LEVEL_DEFAULT: u8 = 60;
/// Shares a player may run at once unless the room says fewer (Studio 1–3).
pub const MINE_MAX: u8 = 2;
/// Shares a player may watch at once.
pub const WATCH_MAX: usize = 2;
/// A share nobody watches stops encoding after this long (`share_set paused`).
pub const PAUSE_AFTER_MS: u64 = 60_000;
/// A share nobody watched for this long ends ("Kimse izlemediği için paylaşım durdu.").
pub const ALONE_STOP_MS: u64 = 15 * 60_000;
/// The watching window out of sight this long: watching is let go (it comes back with the window).
pub const HIDDEN_GRACE_MS: u64 = 3_000;
/// A `watch` the room has not answered with an offer by then is lost (asked again, a few times).
pub const WATCH_OFFER_WAIT_MS: u64 = 20_000;
/// SQOL-R5: a `watch` the room could not set up on the SFU this time (`sfu_failed`, a passing
/// fault) is asked again after these waits; then it goes with the room's refusal. Never for an
/// answer that will not change (`not_found`, `watch_full`, the budget).
pub const WATCH_RETRY_MS: [u64; 2] = [1_500, 4_000];
/// Nobody touched the computer this long while watching: "Hâlâ izliyor musun?".
pub const STILL_ASK_MS: u64 = 30 * 60_000;
/// No answer to "Hâlâ izliyor musun?" this long: watching stops.
pub const STILL_ANSWER_MS: u64 = 60_000;
/// A watch connection lost this many times in a row stops ("Yayına bağlanılamadı.").
pub const WATCH_LOSSES_MAX: u32 = 3;
/// A window title or screen name the picker hands back is held to this many characters.
pub const LABEL_MAX: usize = 120;

// --- kinds ---------------------------------------------------------------------------------------

/// What is shared: the game the launcher started, a window, or a whole screen (wire `src`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum ShareKind {
    Game,
    Screen,
    /// Also any kind a newer service names.
    #[default]
    Window,
}

/// Any kind a newer service names reads as a window (by hand: the page's bindings cannot write
/// `#[serde(other)]` for a plain string enum).
impl<'de> Deserialize<'de> for ShareKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "game" => Self::Game,
            "screen" => Self::Screen,
            _ => Self::Window,
        })
    }
}

/// "Kalite": Düşük (480p30) / Normal (720p30) / Yüksek (1080p30; 1080p60 for a game with
/// "Akıcılık"). Ordered: Studio's `shareMaxPreset` and the budget's step cap it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SharePreset {
    Low,
    #[default]
    Normal,
    High,
}

/// Any preset a newer service names reads as "Normal".
impl<'de> Deserialize<'de> for SharePreset {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "low" => Self::Low,
            "high" => Self::High,
            _ => Self::Normal,
        })
    }
}

/// A simulcast layer a watcher gets: `f` full, `q` a quarter (a small tile).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
pub enum ShareLayer {
    #[default]
    #[serde(rename = "f")]
    F,
    #[serde(rename = "q")]
    Q,
}

/// "Netlik ↔ Akıcılık" (Gelişmiş): sharp text and still pictures, or smooth motion. `Auto` follows
/// the source (a game moves, a window or a screen is read).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareMotion {
    #[default]
    Auto,
    Clarity,
    Motion,
}

/// SQOL "Yayın Kalitesi" while watching: `auto` (the small layer for a small tile or a weak
/// connection), `high` (always the full layer), `saver` (always the small layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareViewQuality {
    #[default]
    Auto,
    High,
    Saver,
}

/// Anything else reads as `auto` (a newer launcher's choice in a backup).
impl<'de> Deserialize<'de> for ShareViewQuality {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "high" => Self::High,
            "saver" => Self::Saver,
            _ => Self::Auto,
        })
    }
}

/// Where a share's sound comes from (Gelişmiş): the shared app's processes, or the whole computer
/// without voice chat. A screen always shares the whole computer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareAudioSource {
    #[default]
    App,
    System,
}

/// Where the player watches: the launcher's window, or its own "Yayın" window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareSurface {
    #[default]
    Main,
    Popout,
}

impl ShareSurface {
    /// The surface's window label (`main`, `share`).
    pub fn window(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Popout => SHARE_WINDOW,
        }
    }

    /// The surface of a window label (none: a window that never watches).
    pub fn of_window(label: &str) -> Option<Self> {
        match label {
            "main" => Some(Self::Main),
            SHARE_WINDOW => Some(Self::Popout),
            _ => None,
        }
    }
}

/// The "Yayın" window's label.
pub const SHARE_WINDOW: &str = "share";
/// Its page (`apps/launcher/share.html`).
pub const SHARE_PAGE: &str = "share.html";

// --- the player's choices ------------------------------------------------------------------------

/// Settings → Sesli sohbet → "Ekran paylaşımı" (the picker writes back what the player chose, so
/// the next share starts the same). Travels in the settings backup with the rest of voice chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceShareSettings {
    /// "Kalite".
    pub quality: SharePreset,
    /// "Sesi de paylaş".
    pub audio: bool,
    /// Gelişmiş: "Netlik ↔ Akıcılık".
    pub motion: ShareMotion,
    /// Gelişmiş: "Kare hızı" (15, 30, 60); none follows the quality (30, a game's "Yüksek" with
    /// "Akıcılık" 60).
    pub fps: Option<u8>,
    /// Gelişmiş: "İmleci göster".
    pub cursor: bool,
    /// Gelişmiş: "Launcher'ı yayında gizle" (`WDA_EXCLUDEFROMCAPTURE` on the launcher's windows).
    pub hide_launcher: bool,
    /// Gelişmiş: "Paylaşırken bildirimleri gizle" (the launcher's own Windows notifications).
    pub quiet: bool,
    /// Gelişmiş: "Ses kaynağı".
    pub audio_source: ShareAudioSource,
    /// Gelişmiş: "Kendi önizlememi göster" (the player's own stream, small, in the panel).
    pub self_preview: bool,
    /// The "Yayın" window stays on top ("Her zaman üstte").
    pub always_on_top: bool,
    /// SQOL "Biri Konuşunca Yayın Sesini Kıs" (watching; off by default).
    pub duck: bool,
    /// SQOL "Kısma Miktarı": percent, 20–90.
    pub duck_level: u8,
    /// SQOL "Ben Konuşunca da Kıs".
    pub duck_self: bool,
    /// SQOL "Yayın Kalitesi" while watching.
    pub view_quality: ShareViewQuality,
    /// SQOL "Önizleme Resmini Paylaş": the room gets the share's still picture (`share_thumb`).
    pub thumbs: bool,
}

impl Default for VoiceShareSettings {
    fn default() -> Self {
        Self {
            quality: SharePreset::Normal,
            audio: true,
            motion: ShareMotion::Auto,
            fps: None,
            cursor: true,
            hide_launcher: true,
            quiet: true,
            audio_source: ShareAudioSource::App,
            self_preview: false,
            always_on_top: false,
            duck: false,
            duck_level: DUCK_LEVEL_DEFAULT,
            duck_self: false,
            view_quality: ShareViewQuality::Auto,
            thumbs: true,
        }
    }
}

impl VoiceShareSettings {
    /// Values held to their ranges: 15, 30 or 60 frames a second; "Kısma Miktarı" 20–90.
    pub fn normalized(mut self) -> Self {
        self.fps = self.fps.map(normal_fps);
        self.duck_level = self.duck_level.clamp(DUCK_LEVEL_MIN, DUCK_LEVEL_MAX);
        self
    }
}

/// A frame rate as one of 15, 30 and 60.
pub fn normal_fps(fps: u8) -> u8 {
    match fps {
        0..=22 => 15,
        23..=45 => 30,
        _ => 60,
    }
}

// --- sources (this computer only) ----------------------------------------------------------------

/// What the player picked to share. Titles and ids stay on this computer: the room hears only the
/// kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShareSource {
    /// The game the launcher started (its window, its processes' sound).
    Game,
    /// A window by the publisher's id for it (a window handle, as text).
    Window { id: String },
    /// A screen by the publisher's id for it.
    Screen { id: String },
}

impl ShareSource {
    pub fn kind(&self) -> ShareKind {
        match self {
            Self::Game => ShareKind::Game,
            Self::Window { .. } => ShareKind::Window,
            Self::Screen { .. } => ShareKind::Screen,
        }
    }

    /// An id the publisher could have given (short, printable).
    pub fn is_plausible(&self) -> bool {
        match self {
            Self::Game => true,
            Self::Window { id } | Self::Screen { id } => {
                !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_graphic())
            }
        }
    }
}

/// One thing the picker offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareSourceInfo {
    pub source: ShareSource,
    /// The window's title, or the screen's name ("Ekran 1" is the page's when empty).
    pub label: String,
    /// The window's program ("javaw", "chrome"), when known.
    pub app: Option<String>,
    /// A small picture of it (`data:image/jpeg;base64,…`), when the publisher made one.
    pub thumb: Option<String>,
    /// Screens: the main one.
    pub primary: bool,
    /// Screens: pixels.
    pub width: u32,
    pub height: u32,
}

/// The picker's list (`share_sources`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareSources {
    /// "Oyunu paylaş": the game the launcher started, while it runs with a window.
    pub game: Option<ShareSourceInfo>,
    /// Other windows (never the launcher's own).
    pub windows: Vec<ShareSourceInfo>,
    pub screens: Vec<ShareSourceInfo>,
    /// The computer can share an app's own sound (Windows 10 2004 / build 20348 and later).
    pub audio: bool,
}

// --- the publisher (pl-share) --------------------------------------------------------------------

/// How one share is captured and sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareOpts {
    pub preset: SharePreset,
    pub fps: u8,
    /// Resolved from [`ShareMotion`]: true favours smooth motion, false sharp text.
    pub motion: bool,
    pub cursor: bool,
    pub audio: bool,
    /// The whole computer's sound (without voice chat) rather than the app's own.
    pub audio_system: bool,
}

impl ShareOpts {
    /// The player's choices for `kind`, held to what the room allows (`max`) and what the computer
    /// can do (`audio`).
    pub fn of(
        settings: &VoiceShareSettings,
        kind: ShareKind,
        max: SharePreset,
        audio: bool,
    ) -> Self {
        let preset = settings.quality.min(max);
        let motion = match settings.motion {
            ShareMotion::Auto => kind == ShareKind::Game,
            ShareMotion::Clarity => false,
            ShareMotion::Motion => true,
        };
        let top = if preset == SharePreset::High { 60 } else { 30 };
        let fps = match settings.fps {
            Some(fps) => fps.min(top),
            None if preset == SharePreset::High && kind == ShareKind::Game && motion => 60,
            None => 30,
        };
        Self {
            preset,
            fps,
            motion,
            cursor: settings.cursor,
            audio: settings.audio && audio,
            audio_system: kind == ShareKind::Screen
                || settings.audio_source == ShareAudioSource::System,
        }
    }
}

/// Why the publisher lost a share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareLostWhy {
    /// It never reached the SFU (UDP closed on this network, likely).
    Connect,
    /// Anything else (the capture, the encoder, the connection later).
    Other,
}

/// Launcher → publisher (`pl-share`). `n` is the player's share number in the room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareCmd {
    /// Capture `source` and make an offer for a new SFU session ([`ShareEvent::Offer`]).
    Prepare {
        n: u8,
        source: ShareSource,
        opts: ShareOpts,
    },
    /// The SFU's answer to that offer.
    Answer {
        n: u8,
        sdp: crate::wire::Sdp,
    },
    /// Nobody watches: hold (one frame a second keeps the tracks alive), or go on.
    Pause {
        n: u8,
        paused: bool,
    },
    /// A watcher came: a key frame now.
    KeyFrame {
        n: u8,
    },
    /// The room allowed less than asked (`ack.preset`), or the player changed the share live
    /// (SQOL, with the room's applied values): the share goes on with these.
    Update {
        n: u8,
        opts: ShareOpts,
    },
    Stop {
        n: u8,
    },
    /// SQOL "Ekranı Değiştir": share `source` instead, watchers staying ([`ShareEvent::Switched`]
    /// or [`ShareEvent::SwitchFailed`]).
    Switch {
        n: u8,
        source: ShareSource,
    },
    /// SQOL "Duraklat" (`on`) / "Sürdür": a blank picture and no sound meanwhile.
    Hold {
        n: u8,
        on: bool,
    },
    /// SQOL: the player's own small live preview while `on`.
    Preview {
        n: u8,
        on: bool,
    },
}

/// Publisher → launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareEvent {
    Offer {
        n: u8,
        sdp: crate::wire::Sdp,
    },
    /// Frames flow to the SFU.
    Started {
        n: u8,
    },
    Stats {
        n: u8,
        kbps: u32,
        fps: u8,
        width: u32,
        height: u32,
    },
    /// The share cannot go on (the encoder, the capture, the connection).
    Lost {
        n: u8,
        why: ShareLostWhy,
    },
    /// The shared window closed, or the game ended (SQOL: the publisher holds a blank picture
    /// meanwhile; the machine decides).
    SourceGone {
        n: u8,
    },
    /// SQOL: the share shows its new source (the player's switch, or the game's window found again
    /// after it was recreated, unasked): the full layer's size now.
    Switched {
        n: u8,
        width: u32,
        height: u32,
        /// SQOL-R4: the new source's window is minimized now.
        minimized: bool,
    },
    /// SQOL: a switch did not happen (the old source goes on).
    SwitchFailed {
        n: u8,
    },
    /// SQOL: the shared window was minimized (`on`) or came back.
    Minimized {
        n: u8,
        on: bool,
    },
    /// The app's sound cannot be had: the share goes on without it.
    AudioUnavailable {
        n: u8,
    },
    /// SHARE-LIVE: the share's still picture for people who do not watch (about 3 s after it went
    /// live, then once a minute): it goes to the room as `share_thumb`.
    Thumb {
        n: u8,
        jpg: ThumbJpeg,
    },
}

/// SHARE-LIVE: a still picture's JPEG bytes (never in a log line: `Debug` says only its length).
#[derive(Clone, PartialEq, Eq)]
pub struct ThumbJpeg(pub Vec<u8>);

impl std::fmt::Debug for ThumbJpeg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<jpeg {} bytes>", self.0.len())
    }
}

// --- the watching page ---------------------------------------------------------------------------

/// What one of the watch session's m-lines carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WatchMid {
    pub mid: String,
    /// The one who shares.
    pub person: String,
    pub n: u8,
    /// `video` or `audio`.
    pub video: bool,
}

/// Launcher → the watching page, through the channel it attached (`share_watch_attach`).
#[derive(Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum WatchCmd {
    /// The SFU's offer for the watch session: a new peer connection (`fresh`: a new session), or
    /// the one it has again; `mids` are the m-lines this offer adds.
    Offer {
        sdp: String,
        mids: Vec<WatchMid>,
        fresh: bool,
    },
    /// TURN for the next peer connection (only after ICE failed).
    Ice { ice: EngineIce },
    /// Close the peer connection. `keep_turn`: the session is asked for anew (after a loss, which
    /// is what TURN was asked for): TURN the page has stays for the next connection (MOB-WATCH).
    /// Otherwise nothing is watched on this page now, or TURN ran out: it goes.
    #[serde(rename_all = "camelCase")]
    Reset { keep_turn: bool },
}

impl std::fmt::Debug for WatchCmd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offer { mids, .. } => write!(f, "Offer(<sdp>, {} mids)", mids.len()),
            Self::Ice { .. } => f.write_str("Ice(<ice>)"),
            Self::Reset { keep_turn } => write!(f, "Reset(keep_turn: {keep_turn})"),
        }
    }
}

/// The watching page → launcher (`share_watch_report`).
#[derive(Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum WatchReport {
    /// The answer to the last offer.
    Answer { sdp: String },
    /// ICE did not connect in 5 s: TURN, please.
    NeedTurn,
    /// The peer connection works.
    Connected,
    /// The peer connection is gone: watching starts anew (a few times, then stops).
    Lost,
}

impl std::fmt::Debug for WatchReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Answer { sdp } => write!(f, "Answer(<sdp {} chars>)", sdp.len()),
            Self::NeedTurn => f.write_str("NeedTurn"),
            Self::Connected => f.write_str("Connected"),
            Self::Lost => f.write_str("Lost"),
        }
    }
}

impl WatchReport {
    /// SDP bigger than a room frame could carry is no answer.
    pub fn is_oversized(&self) -> bool {
        matches!(self, Self::Answer { sdp } if sdp.len() > crate::wire::limits::MAX_FRAME_CHARS)
    }
}

// --- what the page sees --------------------------------------------------------------------------

/// Where one of the player's shares is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareMinePhase {
    /// Capture and the room's answer on their way.
    Starting,
    Live,
    /// Live, nobody watches: encoding rests.
    Resting,
}

/// SQOL: what watchers see of a share (wire `st`): its picture, a blank one ("Yayın
/// duraklatıldı."), or the minimized window's last frame ("Paylaşılan pencere küçültüldü.").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareSt {
    #[default]
    Live,
    Hold,
    Min,
}

/// Anything else (a newer state) reads as live.
impl<'de> Deserialize<'de> for ShareSt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(
            match Option::<serde_json::Value>::deserialize(deserializer)?
                .as_ref()
                .and_then(serde_json::Value::as_str)
            {
                Some("hold") => Self::Hold,
                Some("min") => Self::Min,
                _ => Self::Live,
            },
        )
    }
}

/// SQOL: why one of the player's shares holds a blank picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareHoldWhy {
    /// "Duraklat".
    Paused,
    /// The shared window closed or the screen went: "Paylaştığın pencere kapandı."
    Closed,
    /// The shared game closed: "Oyun kapandı."
    GameClosed,
}

/// One of the player's shares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceMyShare {
    pub n: u8,
    pub kind: ShareKind,
    /// The window's title or the screen's name (this computer only); empty for the game.
    pub label: String,
    pub phase: ShareMinePhase,
    pub audio: bool,
    pub preset: SharePreset,
    pub fps: u8,
    /// Who watches, as the room said (the panel shows their faces and names).
    pub watchers: Vec<String>,
    /// Unix seconds.
    pub since: i64,
    /// SHARE-LIVE: when the room took this share's last still picture (Unix seconds, the room's
    /// clock); none before the first. `share_thumb` answers the player's own from this computer.
    pub th: Option<i64>,
    /// SQOL: what is shared now (this computer only): the picker marks it "Paylaşılıyor".
    pub source: ShareSource,
    /// SQOL: what watchers see.
    pub st: ShareSt,
    /// SQOL: why the picture is blank (`st` "hold").
    pub hold: Option<ShareHoldWhy>,
    /// SQOL: a closed source's share ends then unless another source is picked (Unix seconds).
    pub hold_until: Option<i64>,
    /// SQOL: "Görüntü" as chosen (`auto` follows the source).
    pub motion: ShareMotion,
    /// SQOL: "İmleci Göster".
    pub cursor: bool,
    /// SQOL: "Önizleme Resmini Paylaş" for this share.
    pub thumbs: bool,
}

/// Someone else's share in the room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceLiveShare {
    pub person: String,
    pub n: u8,
    pub kind: ShareKind,
    pub audio: bool,
    pub preset: SharePreset,
    pub fps: u8,
    /// How many watch.
    pub watchers: u16,
    /// Unix seconds.
    pub since: i64,
    /// SHARE-LIVE: when its last still picture came (Unix seconds); none before the first (or in a
    /// room from before SHARE-LIVE). The hover card's `share_thumb` keeps its answer by it.
    pub th: Option<i64>,
    /// SQOL: what watchers see of it now (a room from before SQOL: always live).
    pub st: ShareSt,
}

/// Where one watched share is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareWatchPhase {
    /// Asked for, the picture on its way.
    Connecting,
    Live,
    /// Let go while the window is out of sight; it comes back with the window.
    Held,
}

/// A share the player watches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceWatch {
    pub person: String,
    pub n: u8,
    pub layer: ShareLayer,
    pub phase: ShareWatchPhase,
}

/// A sharer's stream sound for the player (kept per person, like their voice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStreamVolume {
    /// Percent, 0 to `VoiceState.volumeMax` (SQOL: up to 200).
    pub volume: u16,
    pub muted: bool,
}

/// Screen sharing in the room now (`VoiceState.share`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceShareView {
    /// The room shares and watches (the service's switch, its `share1`); false: no share anywhere.
    pub on: bool,
    /// The service's switch is on (`hello.voice.share`): the crew room form offers "Kim
    /// paylaşabilir".
    pub service: bool,
    /// The crew rooms where only the crew's leader and moderators may share (their ids).
    pub crew_mods: Vec<String>,
    /// This launcher can capture and send (false: watching only).
    pub can_share: bool,
    /// The room lets the player share ("Kim paylaşabilir").
    pub allowed: bool,
    /// A new share may start now (false: the month's budget, "Ekran paylaşımı şu an
    /// kullanılamıyor.").
    pub open: bool,
    /// "Kim paylaşabilir" in this room.
    pub share_by: ShareBy,
    /// The player may change "Kim paylaşabilir" (a friend room's leader).
    pub can_set_share_by: bool,
    /// The "Yayın" window stays on top ("Her zaman üstte", the player's choice).
    pub on_top: bool,
    /// Shares the player may run at once here.
    pub mine_max: u8,
    /// The best quality the room allows now ("Yüksek" may be off for the month's budget).
    pub max_preset: SharePreset,
    /// The computer can share an app's own sound.
    pub audio: bool,
    pub mine: Vec<VoiceMyShare>,
    /// Others' shares, in the room's order of people.
    pub live: Vec<VoiceLiveShare>,
    pub watching: Vec<VoiceWatch>,
    /// Where watching shows.
    pub surface: ShareSurface,
    /// "Hâlâ izliyor musun?" waits for an answer.
    pub still_asking: bool,
    /// Sharers' stream sound for the player (people with a choice of their own only).
    pub volumes: BTreeMap<String, VoiceStreamVolume>,
    /// Names for the "Yayın" window, which has no friend list (the launcher fills them).
    pub names: BTreeMap<String, String>,
    /// SQOL: the player's shares change live here ("Ekranı Değiştir", quality, sound, "Duraklat";
    /// the service's `share.edit` and a `share2` room). False: only "Durdur".
    pub edit: bool,
    /// SQOL: this computer's best quality (no hardware encoder: "normal", "Bu bilgisayarda en çok
    /// Normal."), apart from the room's (`max_preset` holds both).
    pub hw_max: SharePreset,
}

impl VoiceShareView {
    /// SHARE-LIVE: the player is live now (a share the room took, resting or not): the taskbar's
    /// mark and the tray's "Yayındasın" follow it.
    pub fn live_now(&self) -> bool {
        self.mine
            .iter()
            .any(|mine| mine.phase != ShareMinePhase::Starting)
    }
}

/// Why a share or a watch was refused (`share_start`, `share_watch` and the room's errors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareRefusal {
    /// Sharing is off (Studio, not in a room, an older room): "Ekran paylaşımı şu an kullanılamıyor."
    Off,
    /// This computer cannot share.
    Unsupported,
    /// The room lets only its leader and moderators share.
    NotAllowed,
    /// A new (limited) account may watch but not share yet.
    Limited,
    /// The player shares as much as they may.
    Full,
    /// The room has as many shares as it may.
    RoomFull,
    /// The share has as many watchers as it may.
    WatchFull,
    /// The player watches as many as they may.
    WatchMax,
    /// The month's budget: no new share or watch now.
    Budget,
    /// That share ended.
    Gone,
    /// The room did not answer: "Bağlanamadı.".
    Failed,
    /// SQOL: that source is in another of the player's shares already.
    InUse,
}

impl ShareRefusal {
    /// The room's error code.
    pub fn from_code(code: &str) -> Self {
        match code {
            "share_off" | "voice_off" => Self::Off,
            "share_full" => Self::RoomFull,
            "share_not_allowed" | "not_allowed" => Self::NotAllowed,
            "limited" => Self::Limited,
            "watch_full" => Self::WatchFull,
            "budget" => Self::Budget,
            "not_found" | "share_not_found" => Self::Gone,
            _ => Self::Failed,
        }
    }
}

/// Why one of the player's shares ended without the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareClosedWhy {
    /// A leader or moderator stopped it: "Bir moderatör paylaşımını durdurdu."
    Mod,
    /// The month's budget.
    Budget,
    /// Nobody watched for 15 minutes.
    Idle,
    /// A limit changed, or Studio switched sharing off (`off`).
    Limit,
    /// The player left the room.
    Left,
    /// The shared window closed, or the game ended (this launcher's own reason).
    SourceGone,
    /// It could not reach the server from this network (this launcher's own reason).
    Network,
    /// Anything else (also any reason a newer service names).
    #[default]
    Error,
}

/// The room's `share_closed.why`: `off` reads as a limit, anything unknown as an error.
impl<'de> Deserialize<'de> for ShareClosedWhy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "mod" => Self::Mod,
            "budget" => Self::Budget,
            "idle" => Self::Idle,
            "limit" | "off" => Self::Limit,
            "left" => Self::Left,
            _ => Self::Error,
        })
    }
}

/// SQOL-R3: the room-checked values a live change asked for (`share_set`), as sent; none: not
/// asked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareAsked {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<SharePreset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<bool>,
}

/// A moment of screen sharing worth a toast (`share-notice`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShareNotice {
    /// One of the player's shares ended without them (`src`: what it showed).
    Closed { why: ShareClosedWhy, src: ShareKind },
    /// A share or a watch could not start; SQOL-R3: or a live change of share `n` the room
    /// refused, with what it asked (`change`: the page rolls back exactly those).
    Refused {
        refusal: ShareRefusal,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        n: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        change: Option<ShareAsked>,
    },
    /// The share goes on without its sound.
    AudioOff,
    /// Watching stopped: nobody answered "Hâlâ izliyor musun?".
    WatchStopped,
    /// Watching could not connect.
    WatchFailed,
    /// SQOL: "Artık {label} paylaşılıyor." (`label` empty for the game: the page names it).
    Switched {
        n: u8,
        src: ShareKind,
        label: String,
    },
    /// SQOL: "Bu pencere paylaşılamadı." (the old source goes on).
    SwitchFailed { n: u8 },
    /// SQOL: the shared window closed ("Paylaştığın pencere kapandı.") or the game did ("Oyun
    /// kapandı."): "Başka Ekran Seç" / "Paylaşımı Durdur"; the share ends after [`HOLD_STOP_MS`]
    /// (`closed`, `sourceGone`).
    SourceClosed { n: u8, src: ShareKind },
}

impl ShareNotice {
    /// A share or a watch that could not start.
    pub fn refused(refusal: ShareRefusal) -> Self {
        Self::Refused {
            refusal,
            n: None,
            change: None,
        }
    }
}

// --- the room socket's share frames --------------------------------------------------------------

/// One live share as the room tells it (`state.sh[]`, `hello.people[].sh[]`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ShareWire {
    pub n: u8,
    #[serde(default)]
    pub src: ShareKind,
    #[serde(default)]
    pub audio: bool,
    #[serde(default)]
    pub preset: SharePreset,
    #[serde(default)]
    pub fps: u8,
    /// How many watch.
    #[serde(default)]
    pub w: u16,
    /// Unix seconds.
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub paused: bool,
    /// SHARE-LIVE (`live1`): when its last still picture came (Unix seconds); anything but a whole
    /// number reads as none.
    #[serde(default, deserialize_with = "unix_option")]
    pub th: Option<i64>,
    /// SQOL (`share2`): what watchers see (absent: live).
    #[serde(default)]
    pub st: ShareSt,
}

fn unix_option<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    Ok(Option::<serde_json::Value>::deserialize(deserializer)?
        .and_then(|value| value.as_i64())
        .filter(|at| *at > 0))
}

/// What the service allows of screen sharing now (`hello.voice.share` on the presence socket,
/// SHARE-S; the launcher's glue reads it from pl-social). None: no sharing (an older service, or
/// Studio's switch off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceShare {
    /// Shares a player may run at once.
    pub per_user: u8,
    /// The best quality a new share may use.
    pub max_preset: SharePreset,
    /// A new share may start now (the month's budget).
    pub open: bool,
    /// A new watch may start now (the month's budget).
    pub watch: bool,
    /// SQOL: live changes of a share (`hello.voice.share.edit`, Studio's "Canlı Yayın Ayarları").
    pub edit: bool,
}

impl Default for ServiceShare {
    fn default() -> Self {
        Self {
            per_user: MINE_MAX,
            max_preset: SharePreset::Normal,
            open: true,
            watch: true,
            edit: false,
        }
    }
}

// --- SQOL: the page's live changes and the watching window's sound ------------------------------

/// "Kalite", "Kare Hızı", "Görüntü", "İmleci Göster", "Sesi de Paylaş" and "Önizleme Resmini
/// Paylaş" on a live share (`share_update`): only what changes is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ShareChange {
    pub preset: Option<SharePreset>,
    pub fps: Option<u8>,
    pub motion: Option<ShareMotion>,
    pub cursor: Option<bool>,
    pub audio: Option<bool>,
    pub thumbs: Option<bool>,
}

impl ShareChange {
    /// What the room must agree to first (`share_set` with `rid`).
    pub fn asks_room(&self) -> bool {
        self.preset.is_some() || self.fps.is_some() || self.audio.is_some()
    }
}

/// The watching choices both windows change (`share_view_prefs`): "Yayın Kalitesi" and the
/// stream's ducking; only what changes is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ShareViewPrefs {
    pub view_quality: Option<ShareViewQuality>,
    pub duck: Option<bool>,
    pub duck_level: Option<u8>,
    pub duck_self: Option<bool>,
}

impl ShareViewPrefs {
    /// `settings` with these changes (held to their ranges).
    pub fn apply(&self, settings: &mut VoiceShareSettings) {
        if let Some(quality) = self.view_quality {
            settings.view_quality = quality;
        }
        if let Some(duck) = self.duck {
            settings.duck = duck;
        }
        if let Some(level) = self.duck_level {
            settings.duck_level = level.clamp(DUCK_LEVEL_MIN, DUCK_LEVEL_MAX);
        }
        if let Some(duck_self) = self.duck_self {
            settings.duck_self = duck_self;
        }
    }
}

/// SQOL `share-stats`: one of the player's live shares now (every 5 s while it sends): "1280×720 ·
/// 30 fps · donanım".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareStatsView {
    pub n: u8,
    /// The full layer's size.
    pub w: u32,
    pub h: u32,
    /// Frames sent a second.
    pub fps: u8,
    /// Everything sent (both layers and the sound), kbit/s.
    pub kbps: u32,
    /// The hardware encoder ("donanım"); false: the software one ("yazılım").
    pub hw: bool,
}

/// SQOL `share-preview`: the player's own small live picture of a share (while asked for).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SharePreviewView {
    pub n: u8,
    /// `data:image/jpeg;base64,…`.
    pub data_url: String,
}

/// What the "Yayın" window plays streams with (`share_popout_audio`, `share-audio`): it cannot
/// read voice chat's settings itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PopoutAudio {
    /// Ses çıkışı (none: the system's default).
    pub device: Option<String>,
    /// "Herkesin Sesi", percent 0–200.
    pub volume: u16,
    /// Deafened: streams are silent too.
    pub deaf: bool,
    /// The stream volume slider's top (percent).
    pub volume_max: u16,
    pub duck: bool,
    pub duck_level: u8,
    pub duck_self: bool,
    pub view_quality: ShareViewQuality,
}

/// "Kim paylaşabilir" (`hello.room.shareBy`, the `room` message): everyone, or the room's leader
/// and moderators. A room sends it only to `share1` sockets: its presence says the room shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareBy {
    Mods,
    #[default]
    All,
}

/// Anything but `mods` reads as everyone.
impl<'de> Deserialize<'de> for ShareBy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "mods" => Self::Mods,
            _ => Self::All,
        })
    }
}

/// One share the room's watch session carries for the player (`hello.me.watch`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WatchWire {
    pub u: String,
    pub n: u8,
}

/// `woffer.mids[mid]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WatchMidWire {
    pub u: String,
    pub n: u8,
    /// `v` or `a`.
    #[serde(default)]
    pub k: String,
}

/// A list of shares where every entry that does not read is dropped.
pub(crate) fn shares_list<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ShareWire>, D::Error> {
    Ok(shares_of(Option::<serde_json::Value>::deserialize(
        deserializer,
    )?))
}

/// `state.sh`: absent (nothing said about shares) or the person's whole list.
pub(crate) fn shares_option<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<ShareWire>>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw.map(|value| shares_of(Some(value))))
}

fn shares_of(raw: Option<serde_json::Value>) -> Vec<ShareWire> {
    match raw {
        Some(serde_json::Value::Array(items)) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value::<ShareWire>(item).ok())
            .filter(|share| share.n > 0)
            .take(8)
            .collect(),
        _ => Vec::new(),
    }
}

/// `hello.me.watch`, every entry that does not read dropped (absent: none said).
pub(crate) fn watch_list<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<WatchWire>>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw.map(|value| match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value::<WatchWire>(item).ok())
            .take(8)
            .collect(),
        _ => Vec::new(),
    }))
}

/// `woffer.mids` where every entry that does not read is dropped.
pub(crate) fn watch_mids<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, WatchMidWire>, D::Error> {
    let raw = Option::<BTreeMap<String, serde_json::Value>>::deserialize(deserializer)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(mid, value)| Some((mid, serde_json::from_value(value).ok()?)))
        .collect())
}

/// A label the publisher gave, held to [`LABEL_MAX`] characters without control characters.
pub fn clip_label(label: &str) -> String {
    label
        .chars()
        .filter(|c| !c.is_control())
        .take(LABEL_MAX)
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn options_follow_the_choices_and_the_rooms_limit() {
        let settings = VoiceShareSettings::default();
        let game = ShareOpts::of(&settings, ShareKind::Game, SharePreset::High, true);
        assert_eq!(game.preset, SharePreset::Normal);
        assert_eq!(game.fps, 30);
        assert!(game.motion);
        assert!(game.audio);
        assert!(!game.audio_system);
        let high = VoiceShareSettings {
            quality: SharePreset::High,
            ..settings.clone()
        };
        assert_eq!(
            ShareOpts::of(&high, ShareKind::Game, SharePreset::High, true).fps,
            60
        );
        // A window is read: sharp text, 30 frames.
        let window = ShareOpts::of(&high, ShareKind::Window, SharePreset::High, true);
        assert!(!window.motion);
        assert_eq!(window.fps, 30);
        // The room's cap and the computer's sound.
        let capped = ShareOpts::of(&high, ShareKind::Game, SharePreset::Low, false);
        assert_eq!(capped.preset, SharePreset::Low);
        assert_eq!(capped.fps, 30);
        assert!(!capped.audio);
        // 60 frames only with "Yüksek".
        let sixty = VoiceShareSettings {
            fps: Some(60),
            ..settings.clone()
        };
        assert_eq!(
            ShareOpts::of(&sixty, ShareKind::Game, SharePreset::High, true).fps,
            30
        );
        // A screen shares the whole computer's sound.
        assert!(ShareOpts::of(&settings, ShareKind::Screen, SharePreset::High, true).audio_system);
        assert_eq!(
            VoiceShareSettings {
                fps: Some(44),
                ..settings
            }
            .normalized()
            .fps,
            Some(30)
        );
    }

    #[test]
    fn room_frames_read_leniently() {
        #[derive(Deserialize)]
        struct State {
            #[serde(default, deserialize_with = "shares_option")]
            sh: Option<Vec<ShareWire>>,
        }
        let state: State = serde_json::from_value(json!({"sh": [
            {"n":1,"src":"game","audio":true,"preset":"normal","fps":30,"w":3,"at":1730800000,"th":1730800060},
            {"n":"x"},
            {"n":2,"src":"hologram","preset":"ultra"},
            {"n":3,"src":"screen","th":"later"}
        ]}))
        .unwrap();
        let sh = state.sh.unwrap();
        assert_eq!(sh.len(), 3);
        assert_eq!(sh[0].src, ShareKind::Game);
        assert_eq!(sh[0].w, 3);
        // SHARE-LIVE: the picture's time; an odd one costs nothing but itself.
        assert_eq!(sh[0].th, Some(1_730_800_060));
        assert_eq!(sh[1].th, None);
        assert_eq!((sh[2].n, sh[2].th), (3, None));
        // Unknown kinds read as a window, unknown presets as "Normal".
        assert_eq!(sh[1].src, ShareKind::Window);
        assert_eq!(sh[1].preset, SharePreset::Normal);
        let none: State = serde_json::from_value(json!({})).unwrap();
        assert_eq!(none.sh, None);
        let cleared: State = serde_json::from_value(json!({"sh": []})).unwrap();
        assert_eq!(cleared.sh, Some(Vec::new()));
    }

    #[test]
    fn written_as_the_page_reads_them() {
        assert_eq!(
            serde_json::to_value(ShareNotice::Closed {
                why: ShareClosedWhy::Idle,
                src: ShareKind::Game,
            })
            .unwrap(),
            json!({"kind":"closed","why":"idle","src":"game"})
        );
        assert_eq!(
            serde_json::to_value(ShareSource::Window { id: "0x1f".into() }).unwrap(),
            json!({"kind":"window","id":"0x1f"})
        );
        assert_eq!(
            serde_json::to_value(WatchCmd::Reset { keep_turn: true }).unwrap(),
            json!({"t":"reset","keepTurn":true})
        );
        // SQOL-R3: a refused live change says its share and what it asked; a share or a watch
        // that could not start says neither.
        assert_eq!(
            serde_json::to_value(ShareNotice::refused(ShareRefusal::Full)).unwrap(),
            json!({"kind":"refused","refusal":"full"})
        );
        assert_eq!(
            serde_json::to_value(ShareNotice::Refused {
                refusal: ShareRefusal::Budget,
                n: Some(1),
                change: Some(ShareAsked {
                    preset: Some(SharePreset::High),
                    fps: Some(60),
                    audio: None,
                }),
            })
            .unwrap(),
            json!({"kind":"refused","refusal":"budget","n":1,"change":{"preset":"high","fps":60}})
        );
        let report: WatchReport =
            serde_json::from_value(json!({"t":"answer","sdp":"v=0"})).unwrap();
        assert_eq!(report, WatchReport::Answer { sdp: "v=0".into() });
        assert_eq!(serde_json::to_value(ShareLayer::Q).unwrap(), json!("q"));
        assert_eq!(
            ShareRefusal::from_code("watch_full"),
            ShareRefusal::WatchFull
        );
        assert_eq!(ShareRefusal::from_code("anything"), ShareRefusal::Failed);
        assert!(!ShareSource::Window { id: String::new() }.is_plausible());
        assert!(
            ShareSource::Screen {
                id: "\\\\.\\DISPLAY1".into()
            }
            .is_plausible()
        );
        assert_eq!(clip_label("a\u{7}b"), "ab");
    }

    /// SQOL: what watchers see, the page's live changes and watching choices, read leniently.
    #[test]
    fn sqol_values_read_leniently() {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(default)]
            st: ShareSt,
        }
        let st = |json: serde_json::Value| serde_json::from_value::<Wire>(json).unwrap().st;
        assert_eq!(st(json!({"st": "hold"})), ShareSt::Hold);
        assert_eq!(st(json!({"st": "min"})), ShareSt::Min);
        assert_eq!(st(json!({})), ShareSt::Live);
        assert_eq!(st(json!({"st": "later"})), ShareSt::Live);
        assert_eq!(st(json!({"st": null})), ShareSt::Live);
        let change: ShareChange =
            serde_json::from_value(json!({"preset": "high", "cursor": false})).unwrap();
        assert_eq!(change.preset, Some(SharePreset::High));
        assert!(change.asks_room());
        assert!(!ShareChange::default().asks_room());
        assert!(
            !serde_json::from_value::<ShareChange>(json!({"thumbs": false, "motion": "clarity"}))
                .unwrap()
                .asks_room()
        );
        let mut settings = VoiceShareSettings::default();
        assert_eq!(
            (settings.duck, settings.duck_level, settings.thumbs),
            (false, DUCK_LEVEL_DEFAULT, true)
        );
        serde_json::from_value::<ShareViewPrefs>(
            json!({"viewQuality": "saver", "duck": true, "duckLevel": 99}),
        )
        .unwrap()
        .apply(&mut settings);
        assert_eq!(settings.view_quality, ShareViewQuality::Saver);
        assert!(settings.duck);
        assert_eq!(settings.duck_level, DUCK_LEVEL_MAX);
        // A backup from a newer launcher, or a hand-edited file.
        let read: VoiceShareSettings =
            serde_json::from_value(json!({"viewQuality": "8k", "duckLevel": 3})).unwrap();
        assert_eq!(read.view_quality, ShareViewQuality::Auto);
        assert_eq!(read.normalized().duck_level, DUCK_LEVEL_MIN);
        assert_eq!(
            serde_json::to_value(ShareNotice::SourceClosed {
                n: 1,
                src: ShareKind::Game
            })
            .unwrap(),
            json!({"kind":"sourceClosed","n":1,"src":"game"})
        );
    }
}
