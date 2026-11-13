// Screen sharing in a voice room (SHARE; the lead's contract docs/tasks/share.md §3–§6): what the
// launcher's page sees of it and what it asks. Rust builds these (`pl_voice::share`); this is their
// mirror, field for field, so the preview's mock (mock.ts) and the app (tauri.ts) answer the same
// shapes. People are ids: the page names them as voice chat does (its faces); the "Yayın" window
// has no friend list and reads `names`.

/** What is shared: the game the launcher started, a window, or a whole screen. */
export type ShareKind = "game" | "window" | "screen";

/** "Kalite": Düşük (480p30) / Normal (720p30) / Yüksek (1080p30; a game's "Akıcılık" 1080p60). */
export type SharePreset = "low" | "normal" | "high";

/** A watcher's simulcast layer: full, or a quarter for a small tile. */
export type ShareLayer = "f" | "q";

/** Gelişmiş: "Netlik ↔ Akıcılık" (`auto` follows the source). */
export type ShareMotion = "auto" | "clarity" | "motion";

/** Gelişmiş: where a share's sound comes from (a screen always shares the whole computer's). */
export type ShareAudioSource = "app" | "system";

/** Where the player watches. */
export type ShareSurface = "main" | "popout";

/** "Kim paylaşabilir": everyone, or the room's leader and moderators. */
export type ShareBy = "all" | "mods";

/** SQOL: "Yayın Kalitesi" while watching: Otomatik / Yüksek (always `f`) / Veri Tasarrufu (`q`). */
export type ShareViewQuality = "auto" | "high" | "saver";

/** Settings → Sesli sohbet → "Ekran paylaşımı" (also what the picker chose last). */
export interface VoiceShareSettings {
  quality: SharePreset;
  /** "Sesi de paylaş". */
  audio: boolean;
  motion: ShareMotion;
  /** 15, 30 or 60; null follows the quality. */
  fps: number | null;
  /** "İmleci göster". */
  cursor: boolean;
  /** "Launcher'ı yayında gizle". */
  hideLauncher: boolean;
  /** "Paylaşırken bildirimleri gizle". */
  quiet: boolean;
  audioSource: ShareAudioSource;
  /** "Kendi önizlememi göster". */
  selfPreview: boolean;
  /** The "Yayın" window stays on top. */
  alwaysOnTop: boolean;
  /** SQOL: "Önizleme Resmini Paylaş" (the room's small picture of the share; off: none goes). */
  thumbs?: boolean;
  /** SQOL-U1: "Yayın Kalitesi" (every share the player watches). */
  viewQuality: ShareViewQuality;
  /** SQOL-U1: "Biri Konuşunca Yayın Sesini Kıs". */
  duck: boolean;
  /** SQOL-U1: "Kısma Miktarı", percent 20–90. */
  duckLevel: number;
  /** SQOL-U1: "Ben Konuşunca da Kıs". */
  duckSelf: boolean;
}

export const DEFAULT_SHARE_SETTINGS: VoiceShareSettings = {
  quality: "normal",
  audio: true,
  motion: "auto",
  fps: null,
  cursor: true,
  hideLauncher: true,
  quiet: true,
  audioSource: "app",
  selfPreview: false,
  alwaysOnTop: false,
  thumbs: true,
  viewQuality: "auto",
  duck: false,
  duckLevel: 60,
  duckSelf: false,
};

/** What the player picked (ids stay on this computer; the room hears only the kind). */
export type ShareSource =
  | { kind: "game" }
  | { kind: "window"; id: string }
  | { kind: "screen"; id: string };

/** One thing the picker offers. */
export interface ShareSourceInfo {
  source: ShareSource;
  /** The window's title, or the screen's name ("Ekran 1" is the page's when empty). */
  label: string;
  app: string | null;
  /** `data:image/jpeg;base64,…`, when the publisher made one. */
  thumb: string | null;
  primary: boolean;
  width: number;
  height: number;
}

/** The picker's list. */
export interface ShareSources {
  game: ShareSourceInfo | null;
  windows: ShareSourceInfo[];
  screens: ShareSourceInfo[];
  /** The computer can share an app's own sound. */
  audio: boolean;
}

export type ShareMinePhase = "starting" | "live" | "resting";

/** One of the player's shares. */
export interface VoiceMyShare {
  n: number;
  kind: ShareKind;
  /** The window's title or the screen's name (this computer only); empty for the game. */
  label: string;
  phase: ShareMinePhase;
  audio: boolean;
  preset: SharePreset;
  fps: number;
  /** Who watches (their faces and names in the panel). */
  watchers: string[];
  /** Unix seconds. */
  since: number;
  /** SHARE-LIVE: when this computer last sent the share's small picture (Unix seconds; `th`). */
  th?: number | null;
  // SQOL (sqol.md §1.2, §3; sqol-r.md "API"): a live share changes without stopping. Optional
  // here: an older launcher says none of them.
  /** What its watchers see (`st`); absent: live. */
  st?: ShareState;
  /** Why it holds: the player paused it, or the window/screen or the game it showed closed. */
  hold?: ShareHoldWhy | null;
  /** When a share whose source closed stops by itself (Unix seconds, about a minute on). */
  holdUntil?: number | null;
  /** What it shows now (this computer only): "Ekranı Değiştir" marks it "Paylaşılıyor". */
  source?: ShareSource | null;
  motion?: ShareMotion;
  cursor?: boolean;
  /** Its small picture goes to the room ("Önizleme Resmini Paylaş"). */
  thumbs?: boolean;
}

/** Someone else's share. */
export interface VoiceLiveShare {
  person: string;
  n: number;
  kind: ShareKind;
  audio: boolean;
  preset: SharePreset;
  fps: number;
  /** How many watch. */
  watchers: number;
  since: number;
  /**
   * SHARE-LIVE: when the room last stored the share's small picture (Unix seconds; `th`); an open
   * preview asks again when it changes. None yet: no picture so far (or an older launcher).
   */
  th?: number | null;
  /** SQOL: paused by its sharer or its window minimized (absent: live, or an older room). */
  st?: ShareState;
}

/**
 * SHARE-LIVE (`share_thumb(room, user, n)`): a share's last small picture for the preview card.
 * `locked`: the player sees the room but may not join it (never a picture then); `none`: no picture
 * yet, the share is gone, or there is nothing the player may see.
 */
export type ShareThumb = { kind: "image"; dataUrl: string } | { kind: "locked" } | { kind: "none" };

export type ShareWatchPhase = "connecting" | "live" | "held";

/** A share the player watches. */
export interface VoiceWatch {
  person: string;
  n: number;
  layer: ShareLayer;
  phase: ShareWatchPhase;
}

export interface VoiceStreamVolume {
  /** Percent, 0–100. */
  volume: number;
  muted: boolean;
}

/** Screen sharing in the room now (`VoiceState.share`). */
export interface VoiceShareView {
  /** The room shares and watches; false: nothing of it anywhere. */
  on: boolean;
  /** The service's switch is on: the crew room form offers "Kim paylaşabilir". */
  service: boolean;
  /** The crew rooms where only the crew's leader and moderators may share (their ids). */
  crewMods: string[];
  /** This computer can capture and send (false: watching only). */
  canShare: boolean;
  /** "Kim paylaşabilir" lets the player share. */
  allowed: boolean;
  /** A new share may start now (false: the month's budget). */
  open: boolean;
  shareBy: ShareBy;
  /** The player may change "Kim paylaşabilir" (a friend room's leader). */
  canSetShareBy: boolean;
  /** The "Yayın" window stays on top (the player's choice). */
  onTop: boolean;
  mineMax: number;
  maxPreset: SharePreset;
  /** The computer can share an app's own sound. */
  audio: boolean;
  mine: VoiceMyShare[];
  live: VoiceLiveShare[];
  watching: VoiceWatch[];
  surface: ShareSurface;
  /** "Hâlâ izliyor musun?" waits. */
  stillAsking: boolean;
  /** Sharers' stream sound for the player (only people with a choice of their own). */
  volumes: Record<string, VoiceStreamVolume>;
  /** Names for the "Yayın" window. */
  names: Record<string, string>;
  /** SQOL: the service lets a live share change (`share.edit`); off: "Paylaşımı Durdur" alone. */
  edit?: boolean;
  /** SQOL: this computer's highest quality ("normal" without a hardware encoder). */
  hwMax?: SharePreset;
}

export const HIDDEN_SHARE_VIEW: VoiceShareView = {
  on: false,
  service: false,
  crewMods: [],
  canShare: false,
  allowed: false,
  open: false,
  shareBy: "all",
  canSetShareBy: false,
  onTop: false,
  mineMax: 0,
  maxPreset: "high",
  audio: false,
  mine: [],
  live: [],
  watching: [],
  surface: "main",
  stillAsking: false,
  volumes: {},
  names: {},
};

export type ShareRefusal =
  | "off"
  | "unsupported"
  | "notAllowed"
  | "limited"
  | "full"
  | "roomFull"
  | "watchFull"
  | "watchMax"
  | "budget"
  | "gone"
  | "failed"
  // SQOL: the source is the player's other share's.
  | "inUse";

export type ShareClosedWhy =
  | "mod"
  | "budget"
  | "idle"
  | "limit"
  | "left"
  | "sourceGone"
  | "network"
  | "error";

/** A moment worth a toast (`share-notice`). */
export type ShareNotice =
  | { kind: "closed"; why: ShareClosedWhy; src: ShareKind }
  | { kind: "refused"; refusal: ShareRefusal }
  | { kind: "audioOff" }
  | { kind: "watchStopped" }
  | { kind: "watchFailed" }
  // SQOL: a share now shows another source ("Ekranı Değiştir", or the launcher's new game window
  // within the minute after the game closed); `label` is empty for the game.
  | { kind: "switched"; n: number; src: ShareKind; label: string }
  // SQOL: "Ekranı Değiştir" could not start the new source: the old one goes on.
  | { kind: "switchFailed"; n: number }
  // SQOL: the window, screen or game a share showed closed: it holds a minute for another source,
  // then ends (`closed`, `sourceGone`).
  | { kind: "sourceClosed"; n: number; src: ShareKind };

// --- SQOL (sqol.md §1.2, §3; sqol-r.md "API"): the player's own live share -----------------------

/** What the watchers see of a share (`st`): live, held (an empty frame) or its window minimised. */
export type ShareState = "live" | "hold" | "min";

/**
 * Why the player's own share holds: they paused it ("Duraklat"), or the window/screen
 * (`closed`) or the game (`gameClosed`) it showed closed (a minute to pick another source).
 */
export type ShareHoldWhy = "paused" | "closed" | "gameClosed";

/** `share_update`: what a live share changes (only the fields given). */
export interface ShareChange {
  preset?: SharePreset;
  fps?: number;
  motion?: ShareMotion;
  cursor?: boolean;
  audio?: boolean;
  thumbs?: boolean;
}

/** `share-stats` (every 5 s): what one of the player's shares really sends. */
export interface ShareStats {
  n: number;
  w: number;
  h: number;
  fps: number;
  kbps: number;
  /** A hardware encoder; false: the processor's (at most "Normal" then). */
  hw: boolean;
}

/** `share-preview` (while asked, at most twice a second): the own share's small live picture. */
export interface SharePreviewFrame {
  n: number;
  /** `data:image/jpeg;base64,…`. */
  dataUrl: string;
}

/** One m-line of the watch session. */
export interface WatchMid {
  mid: string;
  person: string;
  n: number;
  video: boolean;
}

export interface WatchIce {
  urls: string[];
  username: string;
  credential: string;
}

/** Launcher → the watching page. */
export type WatchCmd =
  /** `fresh`: a new watch session (a new connection); `mids`: the m-lines this offer adds. */
  | { t: "offer"; sdp: string; mids: WatchMid[]; fresh: boolean }
  | { t: "ice"; ice: WatchIce }
  /**
   * The connection closes. `keepTurn`: the session is asked for anew (after a loss, which is what
   * TURN was asked for), TURN stays for its next connection; otherwise nothing is watched here now,
   * or TURN ran out.
   */
  | { t: "reset"; keepTurn: boolean };

/** The watching page → launcher. */
export type WatchReport =
  | { t: "answer"; sdp: string }
  | { t: "needTurn" }
  | { t: "connected" }
  | { t: "lost" };

/** A share's key in the page's maps. */
export function shareKey(person: string, n: number): string {
  return `${person}:${n}`;
}
