// What world sharing (HOST1, "Dünyanı arkadaşlarına aç") keeps mounted with the friends feature
// (SocialHost): the backend's view (event driven, nothing is asked for on a timer), its notices
// (a friend's world invite as a toast and, while the launcher is away, a system notification; an
// opening that failed; a join that ended), the step from a ready join to the game (straight in,
// or FIX2's "Bağlan" with a game open), and the windows: the invite window, the world's sheet, a
// join's question, the sentence before "Dünyadan çıkar" and a failed opening's ways out.
import { toast } from "@pl/ui";
import { useEffect } from "react";
import { announce } from "@/features/accounts/use-accounts";
import { shownInGame } from "@/features/overlay/store";
import { notifyOutside } from "@/features/social/actions";
import { showWorldInviteToast } from "@/features/social/invite-toast";
import { useSocialUi } from "@/features/social/store";
import { hostApi } from "@/lib/api/host-api";
import type { HostNotice, HostView, WorldInvite } from "@/lib/api/host-types";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { copyAndJoin, goToWorld, pressJoin } from "./actions";
import { JoinAskDialog, OpenFailedDialog, RemoveGuestDialog } from "./join-dialogs";
import { PickerDialog } from "./picker-dialog";
import { failureWords, refusalKey } from "./rules";
import { useHostUi } from "./store";
import { WorldSheet } from "./world-sheet";
import { openWorldSheet } from "./world-strip";

/** Toasts with an action stay a little longer (they ask for an answer). */
const ACTION_TOAST_MS = 9000;

function inviteOf(session: string): WorldInvite | undefined {
  return useHostUi.getState().view.invites.find((invite) => invite.session === session);
}

function openFriends() {
  useSocialUi.getState().openPanel({ page: "list" });
}

function handleNotice(notice: HostNotice) {
  switch (notice.kind) {
    case "invite": {
      if (notice.invite.quiet) return;
      const text = i18n.t("host.invite.title", { name: notice.invite.name });
      showWorldInviteToast(notice.invite);
      // In game the overlay's card says it (one notification, never two).
      if (!shownInGame("invite")) void notifyOutside(text, { onClick: openFriends });
      announce(text);
      return;
    }
    case "opened": {
      const text = i18n.t("host.opened");
      announce(text);
      // Only while the launcher is in front: in game the overlay's card says it.
      if (document.hasFocus()) {
        toast.success(text, {
          action: { label: i18n.t("host.strip.manage"), onClick: openWorldSheet },
        });
      }
      return;
    }
    case "openFailed": {
      announce(i18n.t(`host.failed.${notice.reason}`));
      // In game the overlay's window says it; the world's row keeps it for later.
      if (document.hasFocus()) useHostUi.getState().showFailure(notice.reason);
      return;
    }
    case "inviteRefused":
      toast(i18n.t(refusalKey(notice.why)));
      return;
    case "joinFailed": {
      const { key, action } = failureWords(notice.failure);
      const text = i18n.t(key, { name: notice.name });
      announce(text);
      const session = notice.session;
      toast(text, {
        duration: action ? ACTION_TOAST_MS : undefined,
        action:
          action === "retry"
            ? {
                label: i18n.t("host.join.retry"),
                onClick: () => {
                  const invite = inviteOf(session);
                  if (invite) pressJoin(invite);
                },
              }
            : action === "copy"
              ? { label: i18n.t("host.join.copyAction"), onClick: () => copyAndJoin(session) }
              : undefined,
      });
      return;
    }
  }
}

/** Ready joins already sent on their way (session and the way's place), so each goes once. */
const sent = new Map<string, string>();

/** A join that turned ready: the game starts straight into the world, or "Bağlan" asks first. */
function onView(view: HostView) {
  for (const invite of view.invites) {
    const join = invite.join;
    if (join?.kind !== "ready") {
      sent.delete(invite.session);
      continue;
    }
    const key = `${join.instanceId}:${join.port}`;
    if (sent.get(invite.session) === key) continue;
    sent.set(invite.session, key);
    if (join.missing > 0) toast(i18n.t("host.join.missing", { count: join.missing }));
    goToWorld(invite, join);
  }
}

function useHostSync() {
  useEffect(() => {
    let alive = true;
    const apply = (view: HostView) => {
      useHostUi.getState().setView(view);
      onView(useHostUi.getState().view);
    };
    const stopView = hostApi.onChange(apply);
    const stopNotice = hostApi.onNotice(handleNotice);
    void hostApi
      .state()
      .then((view) => {
        if (alive) apply(view);
      })
      .catch(() => {
        // The feature stays hidden; nothing else depends on it.
      });
    // A launcher opening whose game ended before the world opened (cancelled, failed) is dropped.
    // The launch's own end, also while other games run on (PLAY-AGAIN).
    const stopLaunch = useLaunch.subscribe((state, before) => {
      const run = before.progress;
      const launchEnded =
        run !== 0 && state.progress !== run && !state.games.some((game) => game.run === run);
      const gameEnded = state.games.length < before.games.length && state.games.length === 0;
      if (!launchEnded && !gameEnded) return;
      if (useHostUi.getState().view.session?.phase === "opening") void hostApi.cancelOpen();
    });
    return () => {
      alive = false;
      stopView();
      stopNotice();
      stopLaunch();
    };
  }, []);
}

export function WorldShareHost() {
  useHostSync();
  return (
    <>
      <PickerDialog />
      <WorldSheet />
      <JoinAskDialog />
      <RemoveGuestDialog />
      <OpenFailedDialog />
    </>
  );
}
