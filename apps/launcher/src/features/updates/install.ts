// Installing the launcher update without dead ends (docs/tasks/q2.md, proposal 9): a running game
// is never interrupted (the update waits for after it), open AFK accounts are named first and come
// back by themselves after the restart (the backend stops them and leaves a restart note).
import { toast } from "@pl/ui";
import type { UpdateStep } from "@/bindings";
import { type AfkAccount, api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { type AskFrom, type UpdatePhase, useUpdate } from "@/stores/update";

/** A game runs or is being prepared: installing would end it, so the update waits for after it. */
export function isGameBusy(kind: string): boolean {
  return kind === "working" || kind === "launching" || kind === "running";
}

/**
 * An AFK account the update stops and brings back: in game, connecting, waiting to reconnect or
 * queued in "Tümünü bağla" (one already stopping stays stopped, like the backend's list).
 */
export function isAfkOpen(account: AfkAccount): boolean {
  const kind = account.state.kind;
  return kind !== "stopped" && kind !== "error" && kind !== "stopping";
}

/** What the title bar pill shows. */
export type PillView =
  | { kind: "hidden" }
  /** Found, not downloaded (background downloads are off, or the download failed): click gets it. */
  | { kind: "available" }
  /** A download the player asked for. */
  | { kind: "downloading"; percent: number }
  /** Downloaded: click restarts into it. */
  | { kind: "ready" }
  /** Downloaded while a game is open: passive until the game closes. */
  | { kind: "afterGame" }
  | { kind: "installing"; step: UpdateStep };

export interface PillInput {
  phase: UpdatePhase;
  version: string | null;
  mandatory: boolean;
  manual: boolean;
  percent: number;
  step: UpdateStep | null;
  gameBusy: boolean;
}

export function pillView(s: PillInput): PillView {
  // The mandatory update screen covers the launcher and offers every way forward itself.
  if (s.mandatory) return { kind: "hidden" };
  switch (s.phase) {
    case "applying":
      return { kind: "installing", step: s.step ?? "installing" };
    case "ready":
      return s.gameBusy ? { kind: "afterGame" } : { kind: "ready" };
    case "downloading":
      // Background downloads stay out of sight until they are ready.
      return s.manual
        ? { kind: "downloading", percent: Math.round(s.percent) }
        : { kind: "hidden" };
    case "checking":
      // "Güncelleme var" was pressed: the pill holds on while the download is looked up.
      return s.manual && s.version ? { kind: "downloading", percent: 0 } : { kind: "hidden" };
    case "available":
      return { kind: "available" };
    case "error":
      // A failed download of a known update: offer it again instead of going quiet.
      return s.version ? { kind: "available" } : { kind: "hidden" };
    default:
      return { kind: "hidden" };
  }
}

/** What pressing "restart and update" does now. */
export type InstallDecision = "ignore" | "ask" | "install";

export function installDecision(s: {
  phase: UpdatePhase;
  asking: boolean;
  gameBusy: boolean;
  openAfk: number;
}): InstallDecision {
  if (s.phase !== "ready" || s.asking || s.gameBusy) return "ignore";
  return s.openAfk > 0 ? "ask" : "install";
}

/** Open AFK accounts right now (asked once, on the press: nothing listens all the time). */
async function openAfkCount(): Promise<number> {
  try {
    return (await api.afkAccounts()).filter(isAfkOpen).length;
  } catch {
    // Unknown: the backend stops whatever runs anyway and brings it back after the restart.
    return 0;
  }
}

/**
 * "Yeniden başlat" from the pill, Settings or the toast after a game: installs at once, or first
 * says in a small popover at `from` that the open AFK accounts reconnect after the update.
 */
export async function requestInstall(from: AskFrom): Promise<void> {
  const decide = (openAfk: number) => {
    const { phase, asking } = useUpdate.getState();
    const gameBusy = isGameBusy(useLaunch.getState().phase.kind);
    return installDecision({ phase, asking: asking !== null, gameBusy, openAfk });
  };
  // Nothing to do (not ready, already asking, a game is open): the backend is not even asked.
  if (decide(0) === "ignore") return;
  const openAfk = await openAfkCount();
  const decision = decide(openAfk);
  if (decision === "ask") useUpdate.setState({ asking: { from, count: openAfk } });
  else if (decision === "install") await useUpdate.getState().apply();
}

/** "Güncelle" in the AFK popover. */
export function confirmInstall(): void {
  const { asking, apply } = useUpdate.getState();
  if (!asking) return;
  useUpdate.setState({ asking: null });
  // A game started meanwhile: the update waits for it after all.
  if (isGameBusy(useLaunch.getState().phase.kind)) return;
  void apply({ afkOpen: true });
}

/** "Sonra", Esc or a click elsewhere. */
export function cancelInstall(): void {
  useUpdate.setState({ asking: null });
}

/**
 * A check the player asked for (it downloads what it finds), answered in a toast. From the pill
 * the answer "ready" needs no toast: the pill itself turns into "Güncelleme hazır".
 */
export async function checkNow(from: AskFrom): Promise<void> {
  const result = await useUpdate.getState().check(true);
  if (result === "upToDate") toast.success(i18n.t("settings.updates.upToDate"));
  else if (result === "ready" && from === "settings")
    toast.success(i18n.t("settings.updates.ready"));
  else if (result === "unavailable") {
    // The backend's own words when it said why ("Güncelleme indirilemedi.").
    toast.error(useUpdate.getState().error ?? i18n.t("settings.updates.unavailable"));
  }
}
