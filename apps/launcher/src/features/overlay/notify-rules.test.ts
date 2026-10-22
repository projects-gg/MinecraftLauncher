// The page's notification rules against the table the in-game overlay's rules run too
// (crates/pl-overlay/tests/notify-rules.json, policy.rs): a rule changed on one side only fails the
// other side's tests. The page's own window parts (in front, on screen, a game running) stay out:
// in game they mean nothing.
import { describe, expect, it } from "vitest";
import { chatMutedNow } from "@/features/social/chat/conversations";
import { chatNoticeWay } from "@/features/social/chat/notify";
import { type NoticePrefs, wantsNotice } from "@/features/social/model";
import { crewMutedNow } from "@/features/social/mute";
import type {
  CrewCall,
  CrewNotifications,
  SocialInvite,
  SocialNotice,
  SocialTeam,
  StatusMode,
} from "@/lib/api/social-types";
import table from "../../../../../crates/pl-overlay/tests/notify-rules.json";

interface TableCrew {
  id: string;
  muted?: boolean;
  muteUntil?: number | null;
}

interface TableSituation {
  status?: StatusMode;
  crewCalls?: boolean;
  crewChat?: boolean;
  chatMutes?: Record<string, number | null>;
  service?: { calls: boolean; muted: boolean; muteUntil: number | null } | null;
  crews?: TableCrew[];
}

type TableEvent =
  | { kind: "direct"; conv: string; mine?: boolean; blocked?: boolean }
  | { kind: "crew"; conv: string; crew: string; mine?: boolean; blocked?: boolean }
  | { kind: "call"; crews?: string[] }
  | { kind: "invite"; quiet?: boolean };

interface TableCase {
  name: string;
  event: TableEvent;
  situation?: TableSituation;
  heard: boolean;
}

const rules = table as unknown as { now: number; cases: TableCase[] };

/** Whether the page would tell the player, by the rules both sides share. */
function heard(event: TableEvent, s: TableSituation, now: number): boolean {
  const status = s.status ?? "online";
  const crews = (s.crews ?? []).map(
    (crew) =>
      ({
        id: crew.id,
        members: [],
        mute: { muted: crew.muted ?? false, muteUntil: crew.muteUntil ?? null },
      }) as unknown as SocialTeam,
  );
  const service: CrewNotifications | undefined = s.service ?? undefined;
  switch (event.kind) {
    case "direct":
    case "crew": {
      // The player's own messages and blocked people never reach the page (pl-social drops them).
      if (event.mine || event.blocked) return false;
      const crew = event.kind === "crew" ? crews.find((one) => one.id === event.crew) : undefined;
      const way = chatNoticeWay({
        kind: event.kind === "crew" ? "crew" : "direct",
        status,
        gameBusy: false,
        inFront: false,
        onScreen: false,
        mutedHere: chatMutedNow(event.conv, s.chatMutes ?? {}, now),
        crewMuted: crew ? crewMutedNow(crew, service, now) : false,
        crewChatOn: s.crewChat ?? false,
      });
      return way !== "none";
    }
    case "call":
    case "invite": {
      const prefs: NoticePrefs = {
        favorites: [],
        notifyFavoriteOnline: false,
        notifyFavoriteProjects: false,
        notifyCrewCalls: s.crewCalls ?? true,
      };
      const notice: SocialNotice =
        event.kind === "call"
          ? { kind: "crewCall", call: { crews: event.crews ?? [] } as unknown as CrewCall }
          : {
              kind: "invite",
              invite: { quiet: event.quiet ?? false } as unknown as SocialInvite,
            };
      return wantsNotice(notice, status, prefs, service, now, crews);
    }
  }
}

describe("the notification rules the overlay shares", () => {
  it("keeps enough cases", () => {
    expect(rules.cases.length).toBeGreaterThanOrEqual(30);
  });

  for (const rule of rules.cases) {
    it(rule.name, () => {
      expect(heard(rule.event, rule.situation ?? {}, rules.now)).toBe(rule.heard);
    });
  }
});
