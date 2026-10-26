// BETA-GATE: the beta update channel is for the Ar-Ge team while Studio's switch is on; the friends
// service decides (pl_social::beta: a role on the Projects Discord, asked of ProBot). Picking
// "Beta" asks first (`join`); on the beta channel every update look asks again (`keepBeta`), and
// an answer that says no moves the launcher back to Kararlı with one quiet toast. It never installs
// an older version: the launcher keeps its beta until Kararlı passes it. What cannot be asked
// keeps a launcher on beta for a week after the last yes (the backend decides).
import { toast } from "@pl/ui";
import { create } from "zustand";
import { type BetaAccess, commands } from "@/bindings";
import i18n from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { type UpdateChannel, usePrefs } from "@/stores/prefs";

/** The application page when nothing better is known (the service names its own). */
export const APPLY_FALLBACK = "https://projects.gg/basvuru";

interface BetaGateState {
  /** The question is out: Settings shows the plate on Beta meanwhile. */
  asking: boolean;
  /** Why this launcher is not on Beta (the last answer that said no), for Settings' line. */
  refusal: BetaAccess | null;
  /** Settings' channel rows on screen: while one is, its line says why and no toast repeats it. */
  rowsShown: number;
  /** Picks Beta when the answer lets this launcher in; true when it did. */
  join: () => Promise<boolean>;
  /** Kararlı was picked: an answer still out changes nothing, and the line goes. */
  clear: () => void;
}

type Asked = { status: "ok"; data: BetaAccess } | { status: "error"; error: unknown };

/**
 * The browser preview's stand-in (`?beta=member|none|signin|unknown`, `&betaWait=<ms>`): the same
 * answers after a short wait, so every state of the row can be seen without the backend.
 */
function previewAnswer(): Promise<Asked> {
  const params = new URLSearchParams(window.location.search);
  const kind = params.get("beta") ?? "member";
  const wait = Number(params.get("betaWait") ?? 400);
  const verdict =
    kind === "none"
      ? "notMember"
      : kind === "signin"
        ? "signIn"
        : kind === "unknown"
          ? "unknown"
          : "allowed";
  return new Promise((resolve) =>
    setTimeout(
      () => resolve({ status: "ok", data: { verdict, apply: APPLY_FALLBACK } }),
      Number.isFinite(wait) ? wait : 400,
    ),
  );
}

async function ask(kind: "join" | "keep"): Promise<BetaAccess | null> {
  try {
    const asked: Asked = !isTauri()
      ? await previewAnswer()
      : kind === "join"
        ? await commands.updateBetaJoin()
        : await commands.updateBetaKeep();
    return asked.status === "ok" ? asked.data : null;
  } catch {
    // The command itself failed (not the service: the backend answers every outage itself).
    return null;
  }
}

/** Joins answered after this many `clear`s are stale (Kararlı was picked meanwhile). */
let generation = 0;

export const useBetaGate = create<BetaGateState>()((set, get) => ({
  asking: false,
  refusal: null,
  rowsShown: 0,
  async join() {
    if (get().asking) return false;
    const mine = ++generation;
    set({ asking: true });
    const access = (await ask("join")) ?? { verdict: "unknown", apply: APPLY_FALLBACK };
    if (mine !== generation) return false;
    if (access.verdict === "allowed") {
      usePrefs.getState().set("updateChannel", "beta");
      set({ asking: false, refusal: null });
      return true;
    }
    set({ asking: false, refusal: access });
    return false;
  },
  clear() {
    generation += 1;
    set({ asking: false, refusal: null });
  },
}));

/**
 * The toast when an update look moved the launcher back to Kararlı. It only tells (toasts are for
 * news, actions stay inline: Settings' line keeps why with "Ekibe başvur", "Discord ile giriş yap" or
 * "Tekrar dene"), so no second gold button sits over OYNA.
 */
function movedToast(access: BetaAccess) {
  const t = i18n.t.bind(i18n);
  if (access.verdict === "notMember") toast(t("betaGate.movedNotMember"));
  else if (access.verdict === "signIn") toast(t("betaGate.movedSignIn"));
  // A check that failed: the warning state (the row's mark).
  else toast.warning(t("betaGate.movedUnknown"));
}

/**
 * Before an update look on the beta channel: the channel to look at. A no moves the launcher to
 * Kararlı (Settings' line keeps why, one toast says it now); a command that failed keeps it where
 * it is (the backend's own outages already are a "yes" within the week).
 */
export async function keepBeta(): Promise<UpdateChannel> {
  const access = await ask("keep");
  if (access === null || access.verdict === "allowed") return "beta";
  usePrefs.getState().set("updateChannel", "stable");
  useBetaGate.setState({ refusal: access });
  if (useBetaGate.getState().rowsShown === 0) movedToast(access);
  return "stable";
}

// The browser preview has no update looks: its screenshots call this to show the toast.
if (typeof window !== "undefined" && !isTauri()) {
  (window as unknown as { __plBetaGate?: object }).__plBetaGate = { keepBeta };
}
