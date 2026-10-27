// The way after the question's answer (PLAY-ACCT, PLAY-AGAIN), one at a time. "Bir tane daha aç"
// and "Yine de aç" start the game at once (the games on screen stay). "Durdur ve oyna" stops the
// launch on its way (nothing to save yet: its files, or a game still opening), then starts this
// one; should that launch have come on screen meanwhile, the rules decide again (another account's
// game beside it, or the owner's question for the same name). Tested in flow.test.ts; the dialog
// is switch-dialog.tsx.
import { announce } from "@/features/accounts/use-accounts";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { gameStage } from "@/stores/launch-owner";
import { usePrefs } from "@/stores/prefs";
import { type SwitchRequest, useGameSwitch } from "./store";
import type { SwitchSides } from "./words";

interface Flow {
  id: number;
  request: SwitchRequest;
  unsubscribe: () => void;
}

let flow: Flow | null = null;
let flows = 0;

function leave() {
  if (!flow) return;
  flow.unsubscribe();
  flow = null;
}

function close() {
  leave();
  useGameSwitch.setState({ request: null, step: null, sides: null });
}

/**
 * "Vazgeç". At the question nothing changes, and another installation's "Oyna" gives the selection
 * back. On the way, nothing more starts (a launch already asked to stop may still stop).
 */
export function cancelSwitch() {
  const { request, step } = useGameSwitch.getState();
  close();
  if (!step && request?.restore) usePrefs.getState().selectInstance(request.restore);
}

/**
 * The answer. One more game ("again", "memory"): it starts now. "Durdur ve oyna": the launch on
 * its way stops first, then this one goes through OYNA's rules again; with nothing on its way any
 * more it goes through them at once. `sides`: the words it was asked with (they stay).
 */
export function confirmSwitch(request: SwitchRequest, sides: SwitchSides | null) {
  const launch = useLaunch.getState();
  if (request.kind !== "stop") {
    close();
    void launch.start(request.instanceId, request.options);
    return;
  }
  const stopping = launch.progress;
  if (!stopping) {
    close();
    void launch.play(request.instanceId, request.options);
    return;
  }
  leave();
  flows += 1;
  const id = flows;
  // The launch is gone once it is neither on its way nor a game that runs.
  const unsubscribe = useLaunch.subscribe((state) => {
    const gone = state.progress !== stopping && !state.games.some((g) => g.run === stopping);
    if (!gone || flow?.id !== id) return;
    close();
    void useLaunch.getState().play(request.instanceId, request.options);
  });
  flow = { id, request, unsubscribe };
  const stage = gameStage(launch.phase) ?? "preparing";
  useGameSwitch.setState({ step: { kind: "stopping" }, sides: sides ? { ...sides, stage } : null });
  announce(i18n.t("playAcct.switch.closingTitle"));
  // Nothing to save yet: the start stops (a game that was opening closes with it).
  void launch.cancel();
}
