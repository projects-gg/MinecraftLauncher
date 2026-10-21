import { describe, expect, it } from "vitest";
import type { TeamPermissions } from "@/lib/api/social-types";
import {
  admits,
  answersRequests,
  crewKeyOf,
  crewLinkIn,
  formatJoinInput,
  joinOffer,
  managesLink,
  normalizeCrewLink,
  REQUEST_LIFETIME,
  showsLink,
  stillWaits,
  waitingRequests,
} from "./crew-joining";

const permissions = (admit: "moderators" | "leader"): TeamPermissions => ({
  invite: "members",
  call: "members",
  kick: "moderators",
  edit: "leader",
  admit,
});

describe("crew invite links", () => {
  it("are twelve symbols in any case, read like codes, and never a code", () => {
    expect(normalizeCrewLink("abcd efgh jkmn")).toBe("ABCD-EFGH-JKMN");
    expect(normalizeCrewLink("ABCDEFGHJKMN")).toBe("ABCD-EFGH-JKMN");
    expect(normalizeCrewLink("o0il_1234-5678")).toBe("0011-1234-5678");
    // A link may start with the code's letters: only a separator after them makes a code.
    expect(normalizeCrewLink("EKPA-BCDE-FGHJ")).toBe("EKPA-BCDE-FGHJ");
    for (const not of [
      "ABCD-EFGH",
      "EKP-ABCD-EFGH",
      "EKP-ABCD-EFGHJ",
      "PRJ-ABCD-EFGH-J",
      "ABCD-EFGH-JKMU",
      "ABCD-EFGH-JKMNP",
      `${"A".repeat(12)}${" ".repeat(30)}`,
    ]) {
      expect(normalizeCrewLink(not)).toBeNull();
    }
  });

  it("are found in what the player pastes", () => {
    expect(crewLinkIn("https://social.projects.gg/e/ABCD-EFGH-JKMN")).toBe("ABCD-EFGH-JKMN");
    expect(crewLinkIn("Ekibimize katıl: https://social.projects.gg/e/abcd-efgh-jkmn?x=1 :)")).toBe(
      "ABCD-EFGH-JKMN",
    );
    expect(crewLinkIn("projects://crew/ABCDEFGHJKMN")).toBe("ABCD-EFGH-JKMN");
    expect(crewLinkIn("projects://ekip/0011-1234-5678")).toBe("0011-1234-5678");
    expect(crewLinkIn("  ABCD-EFGH-JKMN ")).toBe("ABCD-EFGH-JKMN");
    expect(crewLinkIn("Ekip kodu: EKP-7Q2X-9KDA")).toBeNull();
    expect(crewLinkIn("https://social.projects.gg/f/PRJ-7K3M-Q9TD")).toBeNull();
  });

  it("show in the join field as links, codes as codes", () => {
    expect(formatJoinInput("https://social.projects.gg/e/abcd-efgh-jkmn")).toBe("ABCD-EFGH-JKMN");
    expect(formatJoinInput("7q2x9kda")).toBe("EKP-7Q2X-9KDA");
    // Typing past a code's end keeps the code.
    expect(formatJoinInput("EKP-7Q2X-9KDAZ")).toBe("EKP-7Q2X-9KDA");
    // Editing a shown link keeps it a link.
    expect(formatJoinInput("ABCD-EFGH-JKM", true)).toBe("ABCD-EFGH-JKM");
    expect(formatJoinInput("ABCD-EFGH-JK", showsLink("ABCD-EFGH-JKM"))).toBe("ABCD-EFGH-JK");
    expect(formatJoinInput("", true)).toBe("");
    expect(showsLink("EKP-7Q2X")).toBe(false);
    expect(showsLink("")).toBe(false);
  });

  it("tell a link from a code when looking up", () => {
    expect(crewKeyOf("ABCD-EFGH-JKMN")).toEqual({ kind: "link", link: "ABCD-EFGH-JKMN" });
    expect(crewKeyOf("EKP-7Q2X-9KDA")).toEqual({ kind: "code", code: "EKP-7Q2X-9KDA" });
    expect(crewKeyOf("ABCD-EFGH-JK")).toBeNull();
    expect(crewKeyOf("PRJ-7K3M-Q9TD")).toBeNull();
  });
});

describe("join requests", () => {
  const leader = { role: "founder" as const, moderator: false };
  const moderator = { role: "member" as const, moderator: true };
  const member = { role: "member" as const, moderator: false };
  const on = { requests: true, link: true };
  const off = { requests: false, link: false };

  it("are answered by the roles `admit` names", () => {
    expect(admits({ ...moderator, permissions: permissions("moderators") })).toBe(true);
    expect(admits({ ...moderator, permissions: permissions("leader") })).toBe(false);
    expect(admits({ ...leader, permissions: permissions("leader") })).toBe(true);
    expect(admits({ ...member, permissions: permissions("moderators") })).toBe(false);
    expect(answersRequests({ ...leader, permissions: permissions("leader") }, on)).toBe(true);
    expect(answersRequests({ ...leader, permissions: permissions("leader") }, off)).toBe(false);
    expect(managesLink({ ...moderator, permissions: permissions("moderators") }, on)).toBe(true);
    expect(managesLink({ ...moderator, permissions: permissions("moderators") }, off)).toBe(false);
  });

  it("wait seven days, then go by themselves", () => {
    const now = 1_800_000_000_000;
    const sentAt = now / 1000 - REQUEST_LIFETIME;
    expect(stillWaits({ sentAt: sentAt + 1 }, now)).toBe(true);
    expect(stillWaits({ sentAt }, now)).toBe(false);
    const from = { id: "1", name: "Ayşe", username: "ayse", avatar: "discord" };
    expect(
      waitingRequests(
        {
          requests: [
            { ...from, sentAt },
            { ...from, id: "2", sentAt: sentAt + 60 },
          ],
        },
        now,
      ).map((r) => r.id),
    ).toEqual(["2"]);
  });

  it("decide what the found crew's button does", () => {
    const open = { relation: "none" as const, join: "open" as const };
    const approval = { relation: "none" as const, join: "approval" as const };
    const context = { requests: true, invited: false, ownRequests: 0 };
    expect(joinOffer(open, context)).toBe("join");
    expect(joinOffer(approval, context)).toBe("ask");
    // An invite still lets the player in directly; an older service never asks.
    expect(joinOffer(approval, { ...context, invited: true })).toBe("join");
    expect(joinOffer(approval, { ...context, requests: false })).toBe("join");
    expect(joinOffer({ ...approval, requestedAt: 1 }, context)).toBe("waiting");
    expect(joinOffer(approval, { ...context, ownRequests: 3 })).toBe("askLimit");
    expect(joinOffer({ ...approval, relation: "full" }, context)).toBe("full");
    expect(joinOffer({ ...open, relation: "member" }, context)).toBe("member");
  });
});
