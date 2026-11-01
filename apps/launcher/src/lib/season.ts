import type { EmberPalette, ParticleKind as FieldKind } from "@pl/ui";
import type { ParticleKind, SeasonKind } from "@/lib/api/types";
import { isWindowAway, onWindowAwayChange } from "@/lib/window-away";

/**
 * What a season changes on Home (SEASON-AUTO): the particles over the art, their colours and how
 * many, the tint of the scrim behind the hero's words and, for Halloween's night, a grade over the
 * art. Everything else stays: the art is Studio's, and the particles keep every rule of the ember
 * field (Performance mode, reduced motion, rest, a running game, the tray). Each season has a glow
 * of its own colour and a particle of its own, so none reads as "no season" on the fiery art (the
 * review of 1 November).
 */
export interface SeasonLook {
  /** The season's particles; missing keeps Studio's choice for the hero. */
  particles?: FieldKind;
  palette: EmberPalette;
  /** How many particles, against the embers' count (missing: as many). */
  density?: number;
  /** The scrim's colour as `R G B` (as dark as the brand's maroon, so the words keep AA). */
  scrim: string;
  /**
   * A gradient the art is blended with in soft light, top to bottom: dark values deepen the
   * shadows and mid-tones and leave the fire's highlights (Halloween's night). A multiplied grade
   * dimmed the whole art, the dragon's head most, and read as disabled.
   */
  grade?: string;
}

/** The brand's maroon (`ember-900`'s glow): the scrim outside the seasons. */
export const DEFAULT_SCRIM = "34 6 6";

/** Halloween's night: the sky darkest, the ground less; neutral, so nothing turns violet. */
export const NIGHT_GRADE =
  "linear-gradient(180deg, rgb(14 12 22) 0%, rgb(40 36 48) 60%, rgb(70 66 78) 100%)";

export function seasonLook(kind: SeasonKind | undefined): SeasonLook {
  switch (kind) {
    case "halloween":
      // Pumpkin embers with witch violet, in the night.
      return { particles: "embers", palette: "halloween", scrim: "16 8 24", grade: NIGHT_GRADE };
    case "winter":
      return { particles: "snow", palette: "ember", scrim: "6 13 28" };
    case "holiday":
      // Bayram: warm lights floating up, over a deep teal glow (gold and teal, a feast's colours).
      return { particles: "lights", palette: "gold", scrim: "6 24 22" };
    case "event":
      // Bright gold glints, more of them, over an amber glow.
      return { particles: "sparks", palette: "spark", density: 1.3, scrim: "36 20 4" };
    default:
      return { palette: "ember", scrim: DEFAULT_SCRIM };
  }
}

/**
 * Home's particles: none while the player turned them off (Ayarlar → Görünüm) or Studio's hero
 * has none; else the season's, else the hero's.
 */
export function homeParticles(playerOn: boolean, hero: ParticleKind, look: SeasonLook): FieldKind {
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
