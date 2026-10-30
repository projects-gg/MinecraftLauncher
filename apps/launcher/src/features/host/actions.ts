// What world sharing's buttons do (HOST1), in one place: the call, its toast and what a screen
// reader hears. A failure becomes one short sentence (the details stay copyable, `attempt`).
// Host: "Dünyayı aç", "Davet et", "Dünyadan çıkar", "Paylaşımı kapat", "Projects'te buluşun",
// "Tekrar dene". Guest: "Katıl" (and its "Tekrar dene" and "Yeniden katıl"), "Kapat".
import { toast } from "@pl/ui";
import { announce } from "@/features/accounts/use-accounts";
import { attempt } from "@/features/social/actions";
import { joinOrAsk, joinWorld, PROJECTS } from "@/features/social/join-dialog";
import { hostApi } from "@/lib/api/host-api";
import type {
  HostSession,
  JoinStage,
  OpenRequest,
  WorldGuest,
  WorldInvite,
} from "@/lib/api/host-types";
import i18n from "@/lib/i18n";
import { startOwnWorld } from "./launch";
import { joinActive, refusalKey, retryWithCopy } from "./rules";
import { useHostUi } from "./store";

/**
 * "Dünyayı aç": the backend makes the backup the world needs, then the game starts straight into
 * it (or the running game opens it in place). False when it did not start.
 */
export async function openWorld(request: OpenRequest): Promise<boolean> {
  useHostUi.getState().setLastOpen(request);
  const how = await attempt(() => hostApi.open(request));
  if (how === undefined || how === "cancelled") return false;
  if (how === "start" && !startOwnWorld(request.instanceId, request.folder)) {
    // Another game started meanwhile: nothing opens.
    void hostApi.cancelOpen();
    toast(i18n.t("play.oneAtATime"));
    return false;
  }
  return true;
}

/** "Tekrar dene" on a failed opening: the same request again, or the window to make one. */
export function retryOpen(session: Extract<HostSession, { phase: "failed" }>) {
  const ui = useHostUi.getState();
  const last = ui.lastOpen;
  ui.showFailure(null);
  if (last && last.instanceId === session.instanceId && last.folder === session.folder) {
    void openWorld(last);
    return;
  }
  void hostApi.dismissFailure();
  ui.openPicker({
    kind: "open",
    instanceId: session.instanceId,
    folder: session.folder,
    worldName: session.worldName,
    gameMode: "unknown",
    hardcore: false,
    preselect: session.guests.map((g) => g.friend),
  });
}

/** "Projects'te birlikte oyna": the friends get a "Birlikte oyna" invite there, the player "Bağlan". */
export async function meetOnProjects(friends: string[]) {
  useHostUi.getState().showFailure(null);
  const done = await attempt(() => hostApi.meetOnProjects(friends));
  if (done === undefined) return;
  joinOrAsk(PROJECTS);
}

/** "Davet et" while the world is open (the window, the person menu); true when they went. */
export async function inviteFriends(friends: string[]): Promise<boolean> {
  const outcome = await attempt(() => hostApi.invite(friends));
  if (outcome === undefined) return false;
  if (outcome !== "sent") {
    toast(i18n.t(refusalKey(outcome)));
    return false;
  }
  const said = i18n.t("host.invites.sent");
  toast.success(said);
  announce(said);
  return true;
}

/** "Dünyadan çıkar" (after its one sentence). */
export async function removeGuest(guest: Pick<WorldGuest, "friend" | "name">) {
  const done = await attempt(() => hostApi.remove(guest.friend));
  if (done === undefined) return;
  const said = i18n.t("host.remove.done", { name: guest.name });
  toast(said);
  announce(said);
}

/** "Paylaşımı kapat". */
export async function stopSharing(): Promise<boolean> {
  const done = await attempt(() => hostApi.stop());
  if (done === undefined) return false;
  announce(i18n.t("host.stopped"));
  return true;
}

/** "Vazgeç" while an opening makes its backup (before the game starts). */
export function cancelOpening() {
  void attempt(() => hostApi.cancelOpen());
}

/** "Kapat" on a failed opening. */
export function dismissFailure() {
  useHostUi.getState().showFailure(null);
  void attempt(() => hostApi.dismissFailure());
}

// --- guest ------------------------------------------------------------------------------------------

/** The way a ready join takes: straight in, or FIX2's "Bağlan" with a game open. */
export function goToWorld(
  invite: Pick<WorldInvite, "session" | "name">,
  stage: Extract<JoinStage, { kind: "ready" }>,
) {
  joinWorld({
    kind: "world",
    session: invite.session,
    instance: stage.instanceId,
    port: stage.port,
    label: i18n.t("host.copy.name", { name: invite.name }),
  });
}

/**
 * "Katıl" on a world invite, also its "Tekrar dene" and "Yeniden katıl": a ready join goes on
 * (the game was not started, or the "Bağlan" question was left); one under way waits.
 */
export function pressJoin(invite: WorldInvite) {
  const join = invite.join;
  if (join?.kind === "ready") {
    goToWorld(invite, join);
    return;
  }
  if (joinActive(join)) return;
  void attempt(() =>
    hostApi.join(invite.session, retryWithCopy(join) ? { copy: copyOf(invite) } : undefined),
  );
}

/** The install a copy of the friend's makes, named in the player's language. */
export function copyOf(invite: Pick<WorldInvite, "name">): { name: string } {
  return { name: i18n.t("host.copy.name", { name: invite.name }) };
}

/** "Kur ve katıl" after the game said mods were missing. */
export function copyAndJoin(session: string) {
  const invite = useHostUi.getState().view.invites.find((one) => one.session === session);
  if (!invite) return;
  void attempt(() => hostApi.join(session, { copy: copyOf(invite) }));
}

/** "Kapat" on a world invite (a join under way is left first). */
export function dismissWorldInvite(invite: Pick<WorldInvite, "session">) {
  void attempt(() => hostApi.dismissInvite(invite.session));
}
