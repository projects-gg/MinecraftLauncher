// Screen sharing in the browser preview (SHARE): `?voice=share…` dresses one of voice chat's mock
// scenarios with shares, so every surface can be looked at without a launcher. The people are the
// voice mock's (GeceKurdu_ 301, ElmasKazma 303, KumFirtinasi 909 in its friend room; KorAtesi 302
// in its call); the player is 100.
//
// - `shareOffer`: the friend room, sharing on, nobody shares (the panel's "Ekranını paylaş");
// - `shareLive`: the player shares the game, GeceKurdu_ and KumFirtinasi watch;
// - `shareStarting`: the player's share on its way;
// - `shareRest`: the player shares a window nobody watches (encoding rests);
// - `shareTwoMine`: the game and a screen at once;
// - `shareLiveRow`: GeceKurdu_ shares the game (his row's "Yayında" and "İzle");
// - `shareWatch`: the player watches GeceKurdu_'s game (the stage, one big);
// - `shareTwo`: the player watches GeceKurdu_'s game and KumFirtinasi's screen (side by side);
// - `sharePopout`: the same, in the "Yayın" window (`/share.html?voice=sharePopout`);
// - `shareStill`: watching, "Hâlâ izliyor musun?" asks;
// - `shareHeld`: watching, let go while the window was away;
// - `shareCall`: the call with KorAtesi, who shares the game (the call's stage);
// - `shareWatchOnly`: GeceKurdu_ shares, this computer cannot share;
// - `shareNotAllowed`: the room lets only its leader and moderators share;
// - `shareBudget`: the month's budget allows no new share.
import type { VoiceState } from "@/lib/api/voice-types";
import type { ShareSources, VoiceLiveShare, VoiceShareView } from "./types";

const ME = "100";
const nowSec = () => Math.floor(Date.now() / 1000);

/** The voice scenario each share scenario starts from. */
export function shareBase(scenario: string): string {
  return scenario === "shareCall" ? "call1" : "room";
}

const live = (
  person: string,
  n: number,
  kind: VoiceLiveShare["kind"],
  watchers = 1,
): VoiceLiveShare => ({
  person,
  n,
  kind,
  audio: kind !== "screen",
  preset: "normal",
  fps: kind === "game" ? 60 : 30,
  watchers,
  since: nowSec() - 640,
});

const NAMES: Record<string, string> = {
  "301": "GeceKurdu_",
  "302": "KorAtesi",
  "303": "ElmasKazma",
  "909": "KumFirtinasi",
};

const VIEW: VoiceShareView = {
  on: true,
  canShare: true,
  allowed: true,
  open: true,
  shareBy: "all",
  canSetShareBy: true,
  onTop: false,
  mineMax: 2,
  maxPreset: "high",
  audio: true,
  mine: [],
  live: [],
  watching: [],
  surface: "main",
  stillAsking: false,
  volumes: {},
  names: NAMES,
};

/** The share part of a `share…` scenario (null: no such scenario). */
export function shareViewFor(scenario: string): VoiceShareView | null {
  const since = nowSec() - 754;
  switch (scenario) {
    case "shareOffer":
      return VIEW;
    case "shareLive":
      return {
        ...VIEW,
        mine: [
          {
            n: 1,
            kind: "game",
            label: "",
            phase: "live",
            audio: true,
            preset: "normal",
            fps: 30,
            watchers: ["301", "909"],
            since,
          },
        ],
      };
    case "shareStarting":
      return {
        ...VIEW,
        mine: [
          {
            n: 1,
            kind: "game",
            label: "",
            phase: "starting",
            audio: true,
            preset: "normal",
            fps: 30,
            watchers: [],
            since: nowSec(),
          },
        ],
      };
    case "shareRest":
      return {
        ...VIEW,
        mine: [
          {
            n: 1,
            kind: "window",
            label: "Kale planı - Paint",
            phase: "resting",
            audio: false,
            preset: "normal",
            fps: 30,
            watchers: [],
            since,
          },
        ],
      };
    case "shareTwoMine":
      return {
        ...VIEW,
        mine: [
          {
            n: 1,
            kind: "game",
            label: "",
            phase: "live",
            audio: true,
            preset: "high",
            fps: 60,
            watchers: ["301"],
            since,
          },
          {
            n: 2,
            kind: "screen",
            label: "Ekran 2",
            phase: "live",
            audio: false,
            preset: "normal",
            fps: 30,
            watchers: ["909", "303", "301"],
            since: since + 300,
          },
        ],
      };
    case "shareLiveRow":
      return { ...VIEW, live: [live("301", 1, "game", 2)] };
    case "shareWatch":
      return {
        ...VIEW,
        live: [live("301", 1, "game", 2)],
        watching: [{ person: "301", n: 1, layer: "f", phase: "live" }],
      };
    case "shareTwo":
      return {
        ...VIEW,
        live: [live("301", 1, "game", 2), live("909", 1, "screen", 1)],
        watching: [
          { person: "301", n: 1, layer: "f", phase: "live" },
          { person: "909", n: 1, layer: "f", phase: "live" },
        ],
        volumes: { "909": { volume: 60, muted: false } },
      };
    case "sharePopout":
      return {
        ...VIEW,
        live: [live("301", 1, "game", 2)],
        watching: [{ person: "301", n: 1, layer: "f", phase: "live" }],
        surface: "popout",
      };
    case "shareStill":
      return {
        ...VIEW,
        live: [live("301", 1, "game", 2)],
        watching: [{ person: "301", n: 1, layer: "f", phase: "live" }],
        stillAsking: true,
      };
    case "shareHeld":
      return {
        ...VIEW,
        live: [live("301", 1, "game", 2)],
        watching: [{ person: "301", n: 1, layer: "f", phase: "held" }],
      };
    case "shareCall":
      return { ...VIEW, live: [live("302", 1, "game", 0)] };
    case "shareWatchOnly":
      return { ...VIEW, canShare: false, live: [live("301", 1, "game", 2)] };
    case "shareNotAllowed":
      return { ...VIEW, allowed: false, shareBy: "mods", canSetShareBy: false };
    case "shareBudget":
      return { ...VIEW, open: false };
    default:
      return null;
  }
}

/** A voice scenario dressed with a share scenario's part. */
export function withShare(scenario: string, state: VoiceState): VoiceState {
  const share = shareViewFor(scenario);
  return share ? { ...state, share } : state;
}

// --- the picker's list -------------------------------------------------------------------------

/** A small picture as an SVG (the preview has no capture): a sky, hills and a sun. */
function scene(sky: string, ground: string, accent: string, seed: number): string {
  const hills = Array.from({ length: 8 }, (_, i) => {
    const x = i * 40;
    const y = 104 + ((seed * (i + 3)) % 22);
    return `<rect x="${x}" y="${y}" width="40" height="${180 - y}" fill="${ground}"/>`;
  }).join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 180"><rect width="320" height="180" fill="${sky}"/><rect x="${210 + (seed % 40)}" y="26" width="34" height="34" fill="${accent}"/>${hills}</svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

/** A window's picture: a title strip and a few lines. */
function windowShot(bg: string, bar: string, lines: number): string {
  const rows = Array.from(
    { length: lines },
    (_, i) =>
      `<rect x="18" y="${40 + i * 18}" width="${180 + ((i * 53) % 90)}" height="8" rx="2" fill="#ffffff" fill-opacity="0.18"/>`,
  ).join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 180"><rect width="320" height="180" fill="${bg}"/><rect width="320" height="22" fill="${bar}"/>${rows}</svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

/** What a mock stream shows (the stage in the preview). */
export function mockFrame(person: string): string {
  return person === "909"
    ? windowShot("#1d2433", "#2b3446", 7)
    : scene("#6fa3d8", "#3f6b2e", "#f5d76e", Number(person) || 7);
}

export const MOCK_SOURCES: ShareSources = {
  game: {
    source: { kind: "game" },
    label: "Minecraft 26.1.2",
    app: "javaw",
    thumb: scene("#7fb0de", "#4a7a35", "#f6dc78", 5),
    primary: false,
    width: 1920,
    height: 1080,
  },
  windows: [
    {
      source: { kind: "window", id: "0x2a01" },
      label: "Kale planı - Paint",
      app: "mspaint",
      thumb: windowShot("#f2efe8", "#d9d4c9", 4),
      primary: false,
      width: 1280,
      height: 720,
    },
    {
      source: { kind: "window", id: "0x2a02" },
      label: "Redstone notları - Not Defteri",
      app: "notepad",
      thumb: windowShot("#202225", "#2c2f33", 6),
      primary: false,
      width: 1280,
      height: 720,
    },
    {
      source: { kind: "window", id: "0x2a03" },
      label: "Sunucu haritası",
      app: "brave",
      thumb: scene("#24324a", "#1d5e4a", "#9fd3c7", 11),
      primary: false,
      width: 1600,
      height: 900,
    },
  ],
  screens: [
    {
      source: { kind: "screen", id: "\\\\.\\DISPLAY1" },
      label: "",
      app: null,
      thumb: windowShot("#14161c", "#22252e", 3),
      primary: true,
      width: 2560,
      height: 1440,
    },
    {
      source: { kind: "screen", id: "\\\\.\\DISPLAY2" },
      label: "",
      app: null,
      thumb: windowShot("#1b1f29", "#262b37", 2),
      primary: false,
      width: 1920,
      height: 1080,
    },
  ],
  audio: true,
};

export { ME as MOCK_ME };
