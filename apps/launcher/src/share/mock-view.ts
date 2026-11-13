// The stream viewer in the browser preview (SQOL-U1): `?voice=shareView…` dresses the voice mock's
// friend room (GeceKurdu_ 301, ElmasKazma 303, KumFirtinasi 909; the player 100) with what the
// viewer shows, and the preview's switches stand in for what only a real stream has.
//
// - `shareViewOne`: the player watches GeceKurdu_'s game (720p, 2 watch);
// - `shareViewTwo`: GeceKurdu_'s game and KumFirtinasi's screen side by side;
// - `shareViewHold`: GeceKurdu_ paused his share ("Yayın duraklatıldı.");
// - `shareViewMin`: KumFirtinasi's shared window is minimized;
// - `shareViewConnecting`: the watch is on its way ("Bağlanıyor…");
// - `shareViewLoud`: GeceKurdu_'s stream at 160 %;
// - `shareViewOwn`: the player shares the game too (with `own=1`: its tile beside GeceKurdu_'s);
// - `shareViewEnded`: nothing watched (with `ended=301:1`: the "Yayın bitti." card stays).
//
// Switches: `weak=301` (a weak connection: "Düşük Kalite"), `info=1` ("Yayın Bilgileri"),
// `forward=301:1` (brought forward), `stage=small|full`, `zoom=2.5` (the first tile zoomed),
// `chrome=1` (the tiles' controls shown without the pointer), `own=1`, `ended=…`, `unread=3`,
// `chat=1` (the stage's "Sohbet" before the room panel's part lands).
import { param } from "./param";
import type { VoiceLiveShare, VoiceShareView } from "./types";
import type { ShareInfo, ShareQuality } from "./watch";

const nowSec = () => Math.floor(Date.now() / 1000);

const NAMES: Record<string, string> = {
  "301": "GeceKurdu_",
  "302": "KorAtesi",
  "303": "ElmasKazma",
  "909": "KumFirtinasi",
};

const BASE: VoiceShareView = {
  on: true,
  service: true,
  crewMods: [],
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

const game = (watchers: number, st?: VoiceLiveShare["st"]): VoiceLiveShare => ({
  person: "301",
  n: 1,
  kind: "game",
  audio: true,
  preset: "normal",
  fps: 30,
  watchers,
  since: nowSec() - 640,
  th: nowSec() - 14,
  ...(st ? { st } : {}),
});

const screen = (watchers: number, st?: VoiceLiveShare["st"]): VoiceLiveShare => ({
  person: "909",
  n: 1,
  kind: "screen",
  audio: false,
  preset: "normal",
  fps: 30,
  watchers,
  since: nowSec() - 300,
  th: nowSec() - 9,
  ...(st ? { st } : {}),
});

/** The viewer's part of a `shareView…` scenario (null: not one of them). */
export function viewScenario(scenario: string): VoiceShareView | null {
  switch (scenario) {
    case "shareViewOne":
      return {
        ...BASE,
        live: [game(2)],
        watching: [{ person: "301", n: 1, layer: "f", phase: "live" }],
      };
    case "shareViewTwo":
      return {
        ...BASE,
        live: [game(2), screen(1)],
        watching: [
          { person: "301", n: 1, layer: "f", phase: "live" },
          { person: "909", n: 1, layer: "f", phase: "live" },
        ],
        volumes: { "909": { volume: 60, muted: false } },
      };
    case "shareViewHold":
      return {
        ...BASE,
        live: [game(2, "hold")],
        watching: [{ person: "301", n: 1, layer: "f", phase: "live" }],
      };
    case "shareViewMin":
      return {
        ...BASE,
        live: [{ ...screen(1, "min"), kind: "window" }],
        watching: [{ person: "909", n: 1, layer: "f", phase: "live" }],
      };
    case "shareViewConnecting":
      return {
        ...BASE,
        live: [game(1)],
        watching: [{ person: "301", n: 1, layer: "f", phase: "connecting" }],
      };
    case "shareViewLoud":
      return {
        ...BASE,
        live: [game(2)],
        watching: [{ person: "301", n: 1, layer: "f", phase: "live" }],
        volumes: { "301": { volume: 160, muted: false } },
      };
    case "shareViewOwn":
      return {
        ...BASE,
        live: [game(2)],
        watching: [{ person: "301", n: 1, layer: "f", phase: "live" }],
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
            since: nowSec() - 420,
          },
        ],
      };
    case "shareViewEnded":
      return { ...BASE, live: [screen(1)] };
    default:
      return null;
  }
}

/** The preview's quality: `weak=301,909` makes those weak. */
export function mockQuality(person: string): ShareQuality {
  return (param("weak") ?? "").split(",").includes(person) ? "weak" : "good";
}

const INFOS: Record<string, ShareInfo> = {
  "301:f": { w: 1280, h: 720, fps: 30 },
  "909:f": { w: 1280, h: 720, fps: 30 },
  q: { w: 640, h: 360, fps: 15 },
};

/** What a real stream would say in "Yayın Bilgileri". */
export function mockInfo(person: string, layer: "f" | "q"): ShareInfo {
  if (layer === "q") return INFOS.q as ShareInfo;
  return INFOS[`${person}:f`] ?? (INFOS["301:f"] as ShareInfo);
}

/** The preview's ended card (`ended=301:1`): it stays for the screenshot. */
export function mockEnded(): { person: string; n: number } | null {
  const [person, n] = (param("ended") ?? "").split(":");
  return person && n ? { person, n: Number(n) || 1 } : null;
}

/** The preview's zoom on the first tile (`zoom=2.5`). */
export function mockZoom(): number | null {
  const value = Number(param("zoom"));
  return Number.isFinite(value) && value > 1 ? Math.min(4, value) : null;
}
