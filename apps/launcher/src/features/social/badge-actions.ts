// What the crew tags do (TEAM6): putting a tag on or taking it off (shown at once, settled by the
// service, said in a toast with "Geri al"), "Etiketi tak" on the toast that says the player joined
// or founded a crew (only while they wear none; no other reminder anywhere), "Katılma isteği gönder"
// from a tag's card and its refusals, which this device remembers until the Turkey day ends, and
// opening a crew's page from a card.
import { toast } from "@pl/ui";
import { useMemo } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { announce, friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { BadgeAsk, CrewBadge, SocialTeam } from "@/lib/api/social-types";
import i18n from "@/lib/i18n";
import { attempt } from "./actions";
import { type RefusalMemory, refusalWords, wearOutcome, withRefusal } from "./badges";
import { useSocialUi } from "./store";

/** A crew's tag card as the service makes it, for the player's own change while it settles. */
export function badgeOfTeam(team: SocialTeam): CrewBadge {
  return {
    crew: team.id,
    tag: team.tag,
    color: team.color,
    frame: team.frame,
    emblem: team.emblem,
    name: team.name,
    motto: team.motto,
    createdOn: team.createdOn,
    members: team.members.length,
    requests: team.badgeRequests ?? true,
  };
}

/** The crew whose tag the player wears now (a change under way included), or null. */
export function wornNow(): string | null {
  const { badgePending, view } = useSocialUi.getState();
  return badgePending !== undefined ? badgePending : (view.me?.badge?.crew ?? null);
}

/** The tag the player wears, as their own card and rows show it (a change shows at once). */
export function useMyBadge(): CrewBadge | undefined {
  const on = useSocialUi((s) => s.view.crew.badges);
  const pending = useSocialUi((s) => s.badgePending);
  const worn = useSocialUi((s) => s.view.me?.badge);
  const crews = useSocialUi((s) => s.view.crews);
  return useMemo(() => {
    if (!on) return undefined;
    if (pending === undefined) return worn;
    const team = pending === null ? undefined : crews.find((crew) => crew.id === pending);
    return team ? badgeOfTeam(team) : undefined;
  }, [on, pending, worn, crews]);
}

/** A change the service has not shown yet goes after this long anyway (the list shows the truth). */
const PENDING_MS = 3_000;

/**
 * Puts on `next`'s tag (in place of any worn) or takes it off (`null`): shown at once, the old one
 * back if the service refuses. The toast says what happened and offers "Geri al"; going back is
 * said aloud only (no toast answering a toast).
 */
export async function wearTag(next: string | null, options: { undo?: boolean } = {}) {
  const before = wornNow();
  if (before === next) return;
  const ui = useSocialUi.getState();
  const tag = next ? (ui.view.crews.find((crew) => crew.id === next)?.tag ?? "") : "";
  ui.setBadgePending(next);
  const done = await attempt(() => api.social.setBadge(next));
  const settle = () => {
    if (useSocialUi.getState().badgePending === next)
      useSocialUi.getState().setBadgePending(undefined);
  };
  if (done === undefined) {
    settle();
    return;
  }
  // The list's own answer usually came already; else it ends the wait (setView), or this does.
  window.setTimeout(settle, PENDING_MS);
  const outcome = wearOutcome(before, next);
  const text =
    outcome === "wore"
      ? i18n.t("team6.woreToast", { tag })
      : outcome === "switched"
        ? i18n.t("team6.switchedToast", { tag })
        : i18n.t("team6.offToast");
  announce(text);
  if (options.undo) return;
  toast.success(text, {
    action: { label: i18n.t("team6.undo"), onClick: () => void wearTag(before, { undo: true }) },
  });
}

/**
 * Says that the player joined or founded a crew. While they wear no tag (and the service has tags)
 * a toast offers "Etiketi tak" for it (`crew` names it once known); otherwise it is said aloud only,
 * as before, unless `show` asks for a toast anyway (a request taken, said while the player was away:
 * its "Göster" stays then).
 */
export function sayJoined(
  text: string,
  crew: () => string | undefined,
  show?: { label: string; onClick: () => void },
) {
  announce(text);
  const { view } = useSocialUi.getState();
  const offer = view.crew.badges && wornNow() === null;
  if (!offer) {
    if (show) toast.success(text, { action: show });
    return;
  }
  toast.success(text, {
    action: {
      label: i18n.t("team6.joinedAction"),
      onClick: () => {
        const id = crew();
        if (id) void wearTag(id);
      },
    },
    ...(show ? { cancel: show } : {}),
  });
}

/** Opens a crew's page (its "Ekip" pane) in the friends panel. */
export function openCrewPage(id: string) {
  const ui = useSocialUi.getState();
  if (!ui.open) ui.openPanel({ page: "team" });
  else ui.goTo("team");
  ui.setCrewTab(id);
  ui.setCrewPane("crew");
}

// --- requests from a tag ------------------------------------------------------------------------

interface Refusals {
  /** Crew id → until when (Unix seconds) its card shows no request button on this device. */
  until: RefusalMemory;
  refuse: (crew: string, until: number) => void;
}

/** The crews this device was refused a request from a tag, until the Turkey day ends
 * (`pl.badge-requests`). */
export const useBadgeRefusals = create<Refusals>()(
  persist(
    (set) => ({
      until: {},
      refuse: (crew, until) =>
        set((s) => ({ until: withRefusal(s.until, crew, until, Date.now()) })),
    }),
    {
      name: "pl.badge-requests",
      version: 1,
      partialize: (s) => ({ until: s.until }),
    },
  ),
);

export type AskOutcome =
  | { kind: "sent" }
  /** The card says why in one quiet line, and the button stays away until the day ends. */
  | { kind: "refused"; text: string }
  /** The player was in the crew already: the list shows it. */
  | { kind: "member" }
  /** It did not reach the service (a toast said so): the button stays. */
  | { kind: "failed" };

/** "Katılma isteği gönder" on a tag's card: the request waits for the crew's leaders. */
export async function askFromBadge(crew: string, via: string): Promise<AskOutcome> {
  let answer: BadgeAsk;
  try {
    answer = await api.social.requestFromBadge(crew, via);
  } catch (error) {
    toast.error(friendlyError(error, i18n.t("social.actions.failed")).message);
    return { kind: "failed" };
  }
  if (answer.status === "sent") {
    announce(i18n.t("crewWave2.join.sent"));
    return { kind: "sent" };
  }
  const words = refusalWords(answer.reason);
  if (!words) return { kind: "member" };
  useBadgeRefusals.getState().refuse(crew, answer.until);
  const text = i18n.t(
    words.key,
    words.count ? { count: useSocialUi.getState().view.crewLimit } : undefined,
  );
  announce(text);
  return { kind: "refused", text };
}
