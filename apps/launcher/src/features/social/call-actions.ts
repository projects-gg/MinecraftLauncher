// "Ekibi çağır" actions shared by the crew page, the list, Home, the crew picker and the call
// toast: one place for the call, its answer (with the join flow to the caller's server), ending it,
// and what the player hears about each (a toast only where the screen does not already show it).
// A call that cannot start where the player is (CALL1) comes back to the caller, which says why in
// place, under the button (from a menu or Home: in the crew picker).
import { toast } from "@pl/ui";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { CallAnswer, CallOutcome, CrewCall } from "@/lib/api/social-types";
import i18n from "@/lib/i18n";
import { attempt } from "./actions";
import { waitMinutes } from "./calls";
import { callWhere } from "./crews";
import { joinCall } from "./join-dialog";
import { useSocialUi } from "./store";

/** Calls being sent or answered, so a second press waits for the first. */
const busy = new Set<string>();

/**
 * "Ekibi çağır" for `crews` (empty: the primary crew; one call however many it names). A refused
 * call says when the next one can go (the button waits that long); not being on a server it may
 * start from is returned unsaid, for the line under the button.
 */
export async function callCrew(crews: string[] = []): Promise<CallOutcome | undefined> {
  if (busy.has("call")) return undefined;
  busy.add("call");
  const outcome = await attempt(() => api.social.callCrew(crews));
  busy.delete("call");
  if (!outcome) return undefined;
  const ui = useSocialUi.getState();
  const now = Date.now();
  switch (outcome.status) {
    case "called":
      ui.setCallWait({ lastCallAt: now });
      // The card turns into the call in place; screen readers hear it.
      announce(i18n.t("teams.call.sent"));
      break;
    case "wait": {
      const until = now + outcome.seconds * 1000;
      ui.setCallWait({ refusedUntil: until });
      toast(i18n.t("teams.call.wait", { count: waitMinutes(until, now) }));
      break;
    }
    case "active":
      toast(i18n.t("teams.call.active"));
      break;
    case "needsServer":
    case "serverHidden":
      break;
  }
  return outcome;
}

/**
 * "Ekibi çağır" from a menu or Home, where no button has a line under it: it calls at once where a
 * call may start; elsewhere (CALL1) the crew picker opens with the reason (and "Aç ve çağır").
 */
export async function callOrAsk(crews: string[] = []): Promise<void> {
  const ui = useSocialUi.getState();
  if (ui.view.callPlace !== "ok") {
    ui.openCallPicker(crews);
    return;
  }
  const outcome = await callCrew(crews);
  if (outcome?.status === "needsServer" || outcome?.status === "serverHidden") {
    useSocialUi.getState().openCallPicker(crews);
  }
}

/**
 * "Aç ve çağır" on another server: the server's address is shown to friends from now on
 * ("Başka sunuculardaysam adresini göster"), then the crews are called there.
 */
export async function shareAndCall(crews: string[] = []): Promise<CallOutcome | undefined> {
  if (busy.has("share")) return undefined;
  busy.add("share");
  const shared = await attempt(() => api.social.setPrivacy({ shareServer: true }));
  busy.delete("share");
  if (shared === undefined) return undefined;
  return callCrew(crews);
}

/**
 * "Geliyorum", "Birazdan" or "Gelemem". "Geliyorum" to a call on a server goes on to join the
 * caller there: after one question the game opens straight into it, or an open game goes there
 * (FIX2 "Bağlan": in game through the overlay, else it closes as its X does and starts again).
 */
export async function answerCall(call: CrewCall, answer: CallAnswer): Promise<void> {
  const key = `answer:${call.id}`;
  if (busy.has(key)) return;
  const where = callWhere(call);
  const joins = answer === "yes" && where.kind !== "ready";
  if (call.myAnswer === answer) {
    if (joins) joinCall(where);
    return;
  }
  busy.add(key);
  const live = await attempt(() => api.social.answerCall(call.id, answer));
  busy.delete(key);
  if (live === undefined) return;
  if (!live) {
    toast(i18n.t("teams.call.ended"));
    return;
  }
  announce(i18n.t("teams.call.answered"));
  if (joins) joinCall(where);
}

/** "Bitir": the player's own call. The five minutes until the next one still count. */
export async function endCall(call: CrewCall): Promise<void> {
  const key = `end:${call.id}`;
  if (busy.has(key)) return;
  busy.add(key);
  const done = await attempt(() => api.social.endCall(call.id));
  busy.delete(key);
  if (done === undefined) return;
  const ui = useSocialUi.getState();
  ui.setCallWait({ lastCallAt: Math.max(ui.lastCallAt ?? 0, call.startedAt * 1000) });
  announce(i18n.t("teams.call.ended"));
}
