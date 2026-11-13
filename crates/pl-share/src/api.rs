//! The controller's language: what the launcher asks ([`ShareCmd`]) and what comes back
//! ([`ShareEvent`]). `pl-voice` stays pure: its machine produces `Effect::Share(ShareCmd)`, the
//! launcher's glue hands it to [`crate::ShareController::send`] and feeds every [`ShareEvent`] back
//! into the machine. Window titles stay on this computer: they are for the picker only and must
//! never be sent to the service.

use serde::{Deserialize, Serialize};
use specta::Type;

/// A share's number in its room (`n` on the room socket): 1, 2 or 3.
pub type ShareNo = u8;

/// What to capture.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SourceId {
    /// The Minecraft window the launcher started, found by its process (the main visible window of
    /// `pid`, also when the game recreated it).
    Game { pid: u32 },
    /// A top-level window. `hwnd` is the window handle (window handles are 32-bit significant on
    /// 64-bit Windows); `pid` guards against a reused handle.
    Window { hwnd: u32, pid: u32 },
    /// A monitor by its GDI device name (`\\.\DISPLAY1`), stable while the display layout holds.
    Monitor { device: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    Game,
    Window,
    Monitor,
}

/// One row of the picker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: SourceId,
    pub kind: SourceKind,
    /// The window title; empty for monitors (the picker names them "Ekran 1", "Ekran 2").
    pub title: String,
    /// The program's file name without `.exe` (windows and the game); empty for monitors.
    pub app: String,
    /// 1-based monitor number in display order (0 for windows).
    pub index: u8,
    pub primary: bool,
    /// Size in physical pixels.
    pub w: u32,
    pub h: u32,
    /// Minimized windows are listed (they come back when restored) but have no thumbnail.
    pub minimized: bool,
    /// A small JPEG (about 240 px wide), base64, when thumbnails were asked for and could be made.
    pub thumb: Option<String>,
}

/// What this computer can do; the picker greys out what is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareCaps {
    /// Process audio (Windows 10 build 20348 and newer, i.e. Windows 11).
    pub audio: bool,
    /// The capture border can be turned off (Windows 11).
    pub border_off: bool,
    /// The cursor can be turned off in the capture (Windows 10 2004 and newer).
    pub cursor_toggle: bool,
    /// The hardware H.264 encoder's name, or `None` when only the software encoder is there
    /// (then "Yüksek" is not offered: software encodes at most 720p30).
    pub hardware_encoder: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Preset {
    /// 480p, at most 30 fps.
    Low,
    /// 720p, at most 30 fps (the default).
    Normal,
    /// 1080p, 30 fps, or 60 fps when asked for ("Akıcı").
    High,
}

/// "Netlik ↔ Akıcılık": what to give up first when the network is short.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Tuning {
    /// Text and menus: keep the picture sharp, lower the frame rate.
    Clarity,
    /// Games: keep the frame rate, let the picture soften.
    Motion,
}

/// Whose sound goes with the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AudioScope {
    /// The shared program and its child processes (the game, a window). For a monitor this means
    /// the whole computer.
    App,
    /// The whole computer except the voice chat (the WebView2 browser process tree).
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareOpts {
    pub preset: Preset,
    /// 15, 30 or 60; clamped to the preset (60 only with `High`).
    pub fps: u8,
    pub tuning: Tuning,
    pub cursor: bool,
    pub audio: bool,
    pub audio_scope: AudioScope,
}

impl Default for ShareOpts {
    fn default() -> Self {
        Self {
            preset: Preset::Normal,
            fps: 30,
            tuning: Tuning::Clarity,
            cursor: true,
            audio: true,
            audio_scope: AudioScope::App,
        }
    }
}

/// The environment the controller needs from the launcher.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareEnv {
    /// Our own processes: their windows are never listed (the launcher, its tools).
    pub own_pids: Vec<u32>,
    /// The voice window's WebView2 browser process (`ICoreWebView2.BrowserProcessId`): excluded
    /// with its tree from "whole computer" audio so the voice chat is not sent back into the room.
    /// When it is unknown, whole-computer audio is not offered (`AudioUnavailable`).
    pub voice_browser_pid: Option<u32>,
}

/// Commands from the launcher (the voice machine's `Effect::Share`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum ShareCmd {
    /// List what can be shared → [`ShareEvent::Sources`]. `game_pid` is the Minecraft process the
    /// launcher started (it comes first as "Oyunu paylaş"); `thumbs` adds small previews.
    Sources {
        game_pid: Option<u32>,
        thumbs: bool,
    },
    /// Start a share: capture is checked, the SFU offer is made → [`ShareEvent::Offer`] (or
    /// [`ShareEvent::Lost`] when the source cannot be captured).
    Prepare {
        n: ShareNo,
        source: SourceId,
        opts: ShareOpts,
    },
    /// The SFU's answer (from the room's `ack`) → capture, encoding and sending start;
    /// [`ShareEvent::Started`] once media flows.
    Answer {
        n: ShareNo,
        sdp: String,
    },
    /// Nobody watches: stop encoding (a 1 fps keep-alive frame keeps the SFU track alive), or
    /// resume with a keyframe.
    Pause {
        n: ShareNo,
        paused: bool,
    },
    /// A watcher came (the room's `watchers`): send a keyframe on every layer now.
    KeyFrame {
        n: ShareNo,
    },
    /// Change quality, frame rate, cursor or tuning while live (no renegotiation; the size of the
    /// picture changes only when the preset does).
    Update {
        n: ShareNo,
        opts: ShareOpts,
    },
    Stop {
        n: ShareNo,
    },
    StopAll,
    /// The environment changed (e.g. the voice window restarted with another browser process).
    Env {
        env: ShareEnv,
    },
    /// SQOL: share another source in the same share (no new SFU session: watchers stay). The new
    /// source is captured first, then the old capture goes → [`ShareEvent::Switched`]; when it
    /// cannot be captured the old source goes on → [`ShareEvent::SwitchFailed`]. A game whose
    /// window is not up yet (a game that is starting) is waited for up to
    /// [`crate::switch::GAME_WINDOW_WAIT`].
    Switch {
        n: ShareNo,
        source: SourceId,
    },
    /// SQOL "Duraklat": watchers see a blank picture and hear nothing until `on: false` (never the
    /// last frame: the player may have paused to hide something).
    Hold {
        n: ShareNo,
        on: bool,
    },
    /// SQOL: the sharer's own small live picture ([`ShareEvent::PreviewFrame`], at most
    /// [`crate::thumb::PREVIEW_PER_SECOND`] a second, only for new frames) while `on`.
    Preview {
        n: ShareNo,
        on: bool,
    },
}

/// Why a share ended on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LostWhy {
    /// No connection to the SFU within 15 s (UDP is likely blocked): the launcher may offer the
    /// browser fallback.
    Connect,
    /// The connection dropped and did not come back within 10 s.
    Network,
    /// Capture could not start or stopped with an error (e.g. the source is protected).
    Capture,
    /// No working H.264 encoder.
    Encoder,
    /// The answer was not usable (codec or simulcast not accepted).
    Negotiation,
    /// SQOL: the graphics device was lost (a driver update or reset) and the share could not be
    /// rebuilt on a new one.
    Device,
    /// Anything else (logged).
    Internal,
}

/// SQOL: why a [`ShareCmd::Switch`] did not happen (the old source goes on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SwitchWhy {
    /// The window closed or the monitor went away meanwhile (or the game's window never came).
    Gone,
    /// It cannot be captured (protected content, an error).
    Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AudioWhy {
    /// This Windows cannot capture one program's sound (older than build 20348).
    Unsupported,
    /// Whole-computer sound without the voice chat needs the voice window's process.
    NoVoiceProcess,
    /// The audio device failed (logged).
    Failed,
}

/// Live numbers for the sharer's panel (every 5 s while sending).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareStats {
    /// Media sent, all layers and audio, kbit/s over the last interval.
    pub kbps: u32,
    /// Frames encoded per second on the full layer.
    pub fps: u8,
    /// Full layer size.
    pub w: u32,
    pub h: u32,
    /// The encoder's name ("NVIDIA H.264 Encoder MFT", "Microsoft H.264 Encoder").
    pub encoder: String,
    /// True when the software encoder is used (720p30 at most).
    pub software: bool,
    /// The send-side bandwidth estimate, kbit/s (0 until known).
    pub bwe_kbps: u32,
    /// Audio is being sent.
    pub audio: bool,
    pub paused: bool,
}

/// Events back to the launcher (into the voice machine).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum ShareEvent {
    Sources {
        list: Vec<SourceInfo>,
        caps: ShareCaps,
    },
    /// The str0m offer for the room's `share_open` (`sdp`).
    Offer {
        n: ShareNo,
        sdp: String,
    },
    /// Connected to the SFU and sending.
    Started {
        n: ShareNo,
    },
    Stats {
        n: ShareNo,
        stats: ShareStats,
    },
    /// The share ended on its own; the launcher sends `share_close`.
    Lost {
        n: ShareNo,
        why: LostWhy,
    },
    /// The shared window closed (for the game: no new window of it came within
    /// [`crate::switch::GAME_REACQUIRE`], as when it goes fullscreen) or the monitor went away.
    /// SQOL: the share goes on with a blank picture and no sound until a [`ShareCmd::Switch`] or a
    /// [`ShareCmd::Stop`]; the launcher decides.
    SourceGone {
        n: ShareNo,
    },
    /// The share runs without sound.
    AudioUnavailable {
        n: ShareNo,
        why: AudioWhy,
    },
    /// A `Stop` finished (everything released).
    Stopped {
        n: ShareNo,
    },
    /// SHARE-LIVE: the live share's still picture for people who do not watch it (a JPEG of at
    /// most [`crate::thumb::MAX_BYTES`], [`crate::thumb::SIDE`] px on its long side): about 3 s
    /// after `Started`, then once a minute, also while encoding rests. The launcher sends it to the
    /// room (`share_thumb`) and keeps nothing on disk.
    Thumb {
        n: ShareNo,
        jpg: Vec<u8>,
    },
    /// SQOL: the share shows its new source (a [`ShareCmd::Switch`], or the game's window found
    /// again after it was recreated, unasked): the full layer's size now.
    Switched {
        n: ShareNo,
        w: u32,
        h: u32,
    },
    /// SQOL: a [`ShareCmd::Switch`] did not happen; the old source goes on.
    SwitchFailed {
        n: ShareNo,
        why: SwitchWhy,
    },
    /// SQOL: the shared window was minimized (`on`) or came back: watchers keep its last picture.
    Minimized {
        n: ShareNo,
        on: bool,
    },
    /// SQOL: the sharer's own preview (a JPEG, [`crate::thumb::PREVIEW_SIDE`] px on its long side)
    /// while [`ShareCmd::Preview`] is on; never sent anywhere else, never written to disk.
    PreviewFrame {
        n: ShareNo,
        jpg: Vec<u8>,
    },
}
