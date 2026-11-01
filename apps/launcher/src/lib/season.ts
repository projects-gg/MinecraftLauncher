import type { EmberPalette } from "@pl/ui";
import type { ParticleKind, SeasonKind } from "@/lib/api/types";
import { isWindowAway, onWindowAwayChange } from "@/lib/window-away";

/**
 * What a season changes on Home (SEASON-AUTO): the particles over the art, their colours and the
 * tint of the scrim behind the hero's words. Everything else stays: the art is Studio's, and the
 * particles keep every rule of the ember field (Performance mode, reduced motion, rest, a running
 * game, the tray).
 */
export interface SeasonLook {
  /** The season's particles; missing keeps Studio's choice for the hero. */
  particles?: ParticleKind;
  palette: EmberPalette;
  /** The scrim's colour as `R G B` (as dark as the brand's maroon, so the words keep AA). */
  scrim: string;
}

/** The brand's maroon (`ember-900`'s glow): the scrim outside the seasons. */
export const DEFAULT_SCRIM = "34 6 6";

export function seasonLook(kind: SeasonKind | undefined): SeasonLook {
  switch (kind) {
    case "halloween":
      // Pumpkin embers with witch violet, over a night-violet scrim.
      return { particles: "embers", palette: "halloween", scrim: "24 7 28" };
    case "winter":
      return { particles: "snow", palette: "ember", scrim: "6 13 28" };
    case "holiday":
      // Bayram: a slow rise of gold.
      return { particles: "sparks", palette: "gold", scrim: DEFAULT_SCRIM };
    case "event":
      return { particles: "sparks", palette: "ember", scrim: DEFAULT_SCRIM };
    default:
      return { palette: "ember", scrim: DEFAULT_SCRIM };
  }
}

/**
 * Home's particles: none while the player turned them off (Ayarlar → Görünüm) or Studio's hero
 * has none; else the season's, else the hero's.
 */
export function homeParticles(
  playerOn: boolean,
  hero: ParticleKind,
  look: SeasonLook,
): ParticleKind {
  if (!playerOn || hero === "none") return "none";
  return look.particles ?? hero;
}

/** setTimeout's longest reliable wait (~23 days); beyond it the clock wakes early and waits again. */
export const MAX_TIMER_MS = 2_000_000_000;

/** The look's change has come (an unreadable moment never comes). */
export function seasonDue(until: string | undefined, now: number): boolean {
  if (!until) return false;
  const at = Date.parse(until);
  return !Number.isNaN(at) && now >= at;
}

/**
 * How long the season clock sleeps: until the change (a quarter second after it, so the backend's
 * clock is past it too), at most [`MAX_TIMER_MS`]; `undefined` while nothing changes ahead.
 */
export function seasonWait(until: string | undefined, now: number): number | undefined {
  if (!until) return undefined;
  const at = Date.parse(until);
  if (Number.isNaN(at)) return undefined;
  return Math.min(Math.max(0, at - now) + 250, MAX_TIMER_MS);
}

/** What the season clock needs from the page (a fake one in tests). */
export interface ClockEnv {
  now(): number;
  setTimeout(run: () => void, ms: number): number;
  clearTimeout(id: number): void;
  /** The window is on screen (not in the tray, minimized or behind a game). */
  visible(): boolean;
  /** Calls `run` when the window shows again; returns the unsubscribe. */
  onVisible(run: () => void): () => void;
}

/**
 * The season clock: one timer to the moment the look may change (`until`), then `refresh` once,
 * which asks the backend for the home content (and so the new look and the next moment). No
 * polling: a long wait wakes once every ~23 days and sleeps again. While the window is hidden
 * nothing happens at the moment; the refresh waits for the window to show, which also covers
 * waking from sleep past the moment. Returns the stop.
 */
export function startSeasonClock(
  until: string | undefined,
  refresh: () => void,
  env: ClockEnv,
): () => void {
  const first = seasonWait(until, env.now());
  if (first === undefined) return () => {};
  let done = false;
  let timer = 0;
  const fire = () => {
    if (done) return;
    done = true;
    stopVisible();
    refresh();
  };
  const tick = () => {
    if (!seasonDue(until, env.now())) {
      timer = env.setTimeout(tick, seasonWait(until, env.now()) ?? MAX_TIMER_MS);
      return;
    }
    if (env.visible()) fire();
  };
  const stopVisible = env.onVisible(() => {
    if (seasonDue(until, env.now())) fire();
  });
  timer = env.setTimeout(tick, first);
  return () => {
    done = true;
    env.clearTimeout(timer);
    stopVisible();
  };
}

/**
 * The page's clock: real timers, and whether the window is out of sight from `window-away`
 * (WebView2 keeps reporting a page in the tray as visible).
 */
export const pageClock: ClockEnv = {
  now: () => Date.now(),
  setTimeout: (run, ms) => window.setTimeout(run, ms),
  clearTimeout: (id) => window.clearTimeout(id),
  visible: () => !isWindowAway(),
  onVisible: (run) =>
    onWindowAwayChange((away) => {
      if (!away) run();
    }),
};
