import { describe, expect, it } from "vitest";
import type { Friend, SocialSnapshot, SocialTeam, TeamMember } from "@/lib/api/social-types";
import { HIDDEN_SNAPSHOT } from "@/lib/api/social-types";
import { cleanNickname, NICKNAME_MAX, noticeName, realNameOf, withNicknames } from "./nickname";

const friend = (id: string, name: string): Friend => ({
  id,
  name,
  username: name.toLowerCase(),
  avatar: "discord",
  state: "launcher",
  dnd: false,
  discordUrl: "",
});

const member = (id: string, name: string): TeamMember =>
  ({ ...friend(id, name), role: "member", moderator: false }) as unknown as TeamMember;

const crew = (members: TeamMember[]): SocialTeam =>
  ({ id: "0123456789abcdef", name: "Gece Kurtları", members }) as unknown as SocialTeam;

const view = (patch: Partial<SocialSnapshot>): SocialSnapshot => ({ ...HIDDEN_SNAPSHOT, ...patch });

describe("cleanNickname", () => {
  it("collapses spaces, drops control characters and keeps 24 characters", () => {
    expect(cleanNickname("  Kaan   abi ")).toBe("Kaan abi");
    expect(cleanNickname(["a", "b", "c"].join(String.fromCodePoint(0, 0x200b)))).toBe("a b c");
    expect(cleanNickname("   ")).toBeNull();
    expect(cleanNickname("x".repeat(NICKNAME_MAX))).toBe("x".repeat(NICKNAME_MAX));
    expect(cleanNickname("x".repeat(NICKNAME_MAX + 1))).toBeNull();
    // An emoji with its tone counts as one character.
    expect(cleanNickname(`${"y".repeat(NICKNAME_MAX - 1)}👍🏽`)).not.toBeNull();
  });
});

describe("withNicknames", () => {
  const kaan = friend("101", "Kaan_07");
  const mert = friend("102", "MertBlocks");
  const stranger = member("103", "Zeyno_X");

  it("keeps the very snapshot without a nickname for a friend", () => {
    const snapshot = view({ friends: [kaan, mert] });
    expect(withNicknames(snapshot, {})).toBe(snapshot);
    // A nickname of someone who is no friend (any more) changes nothing.
    expect(withNicknames(snapshot, { "103": "Zeyno" })).toBe(snapshot);
  });

  it("names a friend by their nickname wherever the snapshot names them", () => {
    const snapshot = view({
      friends: [kaan, mert],
      crews: [crew([member("101", "Kaan_07"), stranger])],
      invites: [{ id: "i1", from: "101", name: "Kaan_07", at: 1, quiet: false }],
      calls: [{ id: "c1", from: "101", name: "Kaan_07" } as never],
      plans: [{ id: "p1", by: "101", byName: "Kaan_07" } as never],
      teamInvites: [{ id: "t1", from: friend("101", "Kaan_07") } as never],
    });
    const shown = withNicknames(snapshot, { "101": "Kuzenim" });
    expect(shown.friends[0]?.name).toBe("Kuzenim");
    expect(realNameOf(shown.friends[0] as Friend)).toBe("Kaan_07");
    expect(shown.friends[1]).toBe(mert);
    expect(shown.crews[0]?.members.map((m) => m.name)).toEqual(["Kuzenim", "Zeyno_X"]);
    expect(shown.invites[0]?.name).toBe("Kuzenim");
    expect((shown.calls[0] as { name?: string }).name).toBe("Kuzenim");
    expect((shown.plans[0] as { byName?: string }).byName).toBe("Kuzenim");
    expect((shown.teamInvites[0] as { from: { name: string } }).from.name).toBe("Kuzenim");
    // The seq stays: the store keeps ignoring older snapshots.
    expect(shown.seq).toBe(snapshot.seq);
  });

  it("names a notice's sender by their nickname", () => {
    const shown = withNicknames(view({ friends: [kaan] }), { "101": "Kuzenim" });
    expect(noticeName("101", "Kaan_07", shown)).toBe("Kuzenim");
    expect(noticeName("999", "Biri", shown)).toBe("Biri");
    expect(noticeName(undefined, "Biri", shown)).toBe("Biri");
  });
});
