// "Ekibi çağır" actions shared by the crew page, the list, Home and the call toast: one place for
// the call, its answer (with the join flow when the caller is on Projects), ending it, and what the
// player hears about each (a toast only where the screen does not already show it).
import { toast } from "@pl/ui";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { CallAnswer, CrewCall } from "@/lib/api/social-types";
import i18n from "@/lib/i18n";
import { attempt } from "./actions";
import { waitMinutes } from "./calls";
import { joinCall } from "./join-dialog";
import { useSocialUi } from "./store";

/** Calls being sent or answered, so a second press waits for the first. */
const busy = new Set<string>();

/** "Ekibi çağır". A refused call says when the next one can go; the button waits that long. */
export async function callCrew(): Promise<void> {
  if (busy.has("call")) return;
  busy.add("call");
  const outcome = await attempt(() => api.social.callCrew());
  busy.delete("call");
  if (!outcome) return;
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
  }
}

/**
 * "Geliyorum", "Birazdan" or "Gelemem". "Geliyorum" to a caller on projects.gg goes on to join
 * them (the game opens straight into Projects after one question, or the open game is left alone).
 */
export async function answerCall(call: CrewCall, answer: CallAnswer): Promise<void> {
  const key = `answer:${call.id}`;
  if (busy.has(key)) return;
  const joins = answer === "yes" && call.kind === "join";
  if (call.myAnswer === answer) {
    if (joins) joinCall();
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
  if (joins) joinCall();
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
