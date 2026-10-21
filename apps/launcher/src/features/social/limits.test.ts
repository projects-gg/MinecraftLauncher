import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import { isLimited, limitOf, opensIn } from "./limits";

const NOW = 1_792_500_000_000;
const me = {
  limited: {
    until: NOW / 1000 + 3 * 86_400 - 60,
    items: [{ key: "crewFound" as const }, { key: "crewInvites" as const, perDay: 5 }],
  },
};

const t = ((key: string, options?: { count?: number }) =>
  options?.count === undefined ? key : `${key}:${options.count}`) as unknown as TFunction;

describe("limited accounts", () => {
  it("know what is limited while the limits last", () => {
    expect(isLimited(me, NOW)).toBe(true);
    expect(limitOf(me, "crewFound", NOW)).toEqual({ until: me.limited.until, perDay: undefined });
    expect(limitOf(me, "crewInvites", NOW)?.perDay).toBe(5);
    expect(limitOf(me, "settingsBackup", NOW)).toBeNull();
    // Once the account is old enough, nothing is limited any more.
    expect(limitOf(me, "crewFound", me.limited.until * 1000)).toBeNull();
    expect(isLimited(me, me.limited.until * 1000 + 1)).toBe(false);
    expect(isLimited({}, NOW)).toBe(false);
  });

  it("say when a limit opens", () => {
    expect(opensIn(t, me.limited.until, NOW)).toBe("safety.limited.opensDays:3");
    expect(opensIn(t, NOW / 1000 + 5 * 3600 - 30, NOW)).toBe("safety.limited.opensHours:5");
    expect(opensIn(t, NOW / 1000 + 600, NOW)).toBe("safety.limited.opensSoon");
  });
});
