// The way after the question's answer (PLAY-ACCT), one at a time: the game that is on closes the
// way its own close button does (or, still opening or preparing its files, simply stops), then
// "Oyna" starts again for the selected account. The steps and their time limits are FIX2's
// ("Bağlan", social/connect.ts): after 20 s the player may wait on or end it, after one sentence
// about what may be lost. Tested in flow.test.ts; the dialog is switch-dialog.tsx.
import { announce } from "@/features/accounts/use-accounts";
import {
  CLOSE_GRACE_MS,
  type CloseEvent,
  type CloseStep,
  closeNext,
  FORCE_GRACE_MS,
} from "@/features/social/connect";
import { api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { gameStage } from "@/stores/launch-owner";
import { usePrefs } from "@/stores/prefs";
import { type SwitchRequest, useGameSwitch } from "./store";
import type { SwitchSides } from "./words";

interface Flow {
  id: number;
  request: SwitchRequest;
  /** The step's own timer (its time limit). */
  timer?: ReturnType<typeof setTimeout>;
  unsubscribe: () => void;
}

let flow: Flow | null = null;
let flows = 0;

function leave() {
  if (!flow) return;
  clearTimeout(flow.timer);
  flow.unsubscribe();
  flow = null;
}

function close() {
  leave();
  useGameSwitch.setState({ request: null, step: null, sides: null });
}

function start(request: SwitchRequest) {
  void useLaunch.getState().play(request.instanceId, request.options);
}

/**
 * "Vazgeç". At the question nothing changes, and another installation's "Oyna" gives the selection
 * back. On the way, nothing more starts (a game already asked to close may still close).
 */
export function cancelSwitch() {
  const { request, step } = useGameSwitch.getState();
  close();
  if (!step && request?.restore) usePrefs.getState().selectInstance(request.restore);
}

/** Each step's title, which a screen reader hears as it begins. */
const STEP_TITLES: Record<CloseStep["kind"], string> = {
  inGame: "fix2.connect.closingTitle",
  closing: "fix2.connect.closingTitle",
  stopping: "fix2.connect.closingTitle",
  overdue: "fix2.connect.overdueTitle",
  forcing: "fix2.connect.forcingTitle",
};

/** One step on the way: the answers, the timers and the game's end lead here. */
function step(id: number, event: CloseEvent) {
  const current = useGameSwitch.getState().step;
  if (!flow || flow.id !== id || !current) return;
  const next = closeNext(current, event);
  if (next === current) return;
  if (next === "start" || next === "connected") {
    const { request } = flow;
    close();
    start(request);
    return;
  }
  useGameSwitch.setState({ step: next });
  begin(flow, next);
}

/** What a step does as it begins; its time limit, if it has one. */
function begin(current: Flow, next: CloseStep) {
  const { id } = current;
  const limit = (ms: number, event: () => CloseEvent) => {
    clearTimeout(current.timer);
    current.timer = setTimeout(() => step(id, event()), ms);
  };
  announce(i18n.t(STEP_TITLES[next.kind]));
  switch (next.kind) {
    case "closing":
      limit(CLOSE_GRACE_MS, () => ({ type: "tick", now: Date.now() }));
      void api
        .closeGame()
        .catch(() => false)
        .then((asked) => step(id, { type: "asked", asked }));
      break;
    case "forcing":
      limit(FORCE_GRACE_MS, () => ({ type: "tick", now: Date.now() }));
      void api.killGame();
      break;
    case "stopping":
      // Nothing to save yet: the start stops (a game that was opening closes with it).
      void useLaunch.getState().cancel();
      break;
    default:
      clearTimeout(current.timer);
      break;
  }
}

/**
 * The answer ("Kapat ve oyna", "Durdur ve oyna"): straight to the start when the game is gone
 * already; else the way, from its first step. `sides`: the words it was asked with (they stay).
 */
export function confirmSwitch(request: SwitchRequest, sides: SwitchSides) {
  const stage = gameStage(useLaunch.getState().phase);
  if (stage === null) {
    close();
    start(request);
    return;
  }
  leave();
  flows += 1;
  const id = flows;
  // The game's end, however it comes (asked, ended, or closed by the player meanwhile).
  const unsubscribe = useLaunch.subscribe((state, before) => {
    if (gameStage(state.phase) === null && gameStage(before.phase) !== null) {
      step(id, { type: "exited" });
    }
  });
  flow = { id, request, unsubscribe };
  const first: CloseStep =
    stage === "open" ? { kind: "closing", since: Date.now() } : { kind: "stopping" };
  useGameSwitch.setState({ step: first, sides: { ...sides, stage } });
  begin(flow, first);
}

/** The step's buttons ("Beklemeye devam et", "Zorla kapat") send it on. */
export function sendSwitch(event: CloseEvent) {
  if (flow) step(flow.id, event);
}
