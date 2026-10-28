import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import { tr } from "@/locales/tr";
import { trCloudAfk } from "@/locales/tr-cloud-afk";
import { trCloudStale } from "@/locales/tr-cloud-stale";
import type { CloudAccount, CloudPageData } from "./api";
import {
  lastKnownText,
  NO_REACH,
  type ReachState,
  reachOf,
  withAway,
  withBack,
  withLive,
  withOverview,
} from "./reach";
import { cloudStatus } from "./status";

// The Turkish strings with the cloud texts, looked up like i18next does.
const strings: Record<string, unknown> = { ...tr, cloudAfk: trCloudAfk, cloudStale: trCloudStale };
const t = ((key: string, values?: Record<string, unknown>) => {
  let value: unknown = strings;
  for (const part of key.split(".")) value = (value as Record<string, unknown> | undefined)?.[part];
  if (typeof value !== "string") throw new Error(`no text for ${key}`);
  return value.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(values?.[name]));
}) as TFunction;

/** 28 Oct 2026, 14:05 on this computer's clock (local, whatever its time zone). */
const AT = new Date(2026, 9, 28, 14, 5).getTime();
const NOW = Math.floor(AT / 1000);

const RUNNING = {
  state: "running",
  since: NOW - 3_600,
  reason: null,
  attempt: null,
  retryAt: null,
};
const UNKNOWN = { state: "unknown", since: null, reason: null, attempt: null, retryAt: null };

function account(overrides: Partial<CloudAccount> = {}): CloudAccount {
  return {
    id: "a_TEST0000000000001",
    name: "Madenci07",
    server: { host: "mc.madenciler.net", port: 25570 },
    version: "26.3",
    lang: "tr",
    host: "kz1",
    createdAt: NOW - 12 * 86_400,
    expiresAt: NOW + 18 * 86_400,
    expired: false,
    graceUntil: NOW + 48 * 86_400,
    renewal: { open: true, opensAt: NOW - 86_400 },
    desired: "run",
    hold: null,
    live: RUNNING,
    everJoined: true,
    config: {
      v: 1,
      owner: "",
      antiAfk: { enabled: false, delayMin: 60, delayMax: 90 },
      commands: [],
      rules: [],
      inventory: null,
      movement: null,
    },
    rev: 1,
    serverChanges: { used: 0, limit: 6, resetAt: null },
    tier: "plain",
    plusUntil: null,
    ai: null,
    features: null,
    moderation: null,
    ...overrides,
  };
}

function page(
  accounts: CloudAccount[],
  session: "signedIn" | "signedOut" | "failed" = "signedIn",
  available = true,
): CloudPageData {
  return {
    overview: {
      available,
      mode: "on",
      maintenance: null,
      monthPrice: "60.00",
      soldOut: false,
      remaining: null,
      salesOpen: true,
      renewalWindowDays: 0,
      limits: {
        perPlayerCap: 10,
        serverChangesPerHour: 6,
        maxCommandRows: 20,
        maxRules: 10,
        minIntervalSec: 60,
        minRuleCooldownSec: 10,
        consoleLinesPerMinute: 20,
      },
      session,
      accounts,
      count: null,
      canBuy: null,
      now: NOW,
      plus: null,
      plusMe: null,
      features: null,
    },
    twins: [],
  };
}

describe("what is known of the machines", () => {
  it("keeps a read's live states, never a machine that could not be asked", () => {
    const other = account({ id: "a_TEST0000000000002", live: UNKNOWN });
    const state = withOverview(NO_REACH, page([account(), other]), AT);
    expect(state.known).toEqual({ a_TEST0000000000001: { live: RUNNING, at: AT } });
  });

  it("takes a machine as back once a read that began after it went away could ask it", () => {
    const away = withAway(NO_REACH, "kz1", AT);
    // A read under way when the socket said so still counts it away.
    expect(withOverview(away, page([account()]), AT).away).toEqual({ kz1: AT });
    expect(withOverview(away, page([account()]), AT + 1).away).toEqual({});
    // One whose accounts could not be asked is not back.
    expect(withOverview(away, page([account({ live: UNKNOWN })]), AT + 1).away).toEqual({
      kz1: AT,
    });
  });

  it("forgets everything when signed out, nothing when the service did not answer", () => {
    const state = withOverview(withAway(NO_REACH, "kz1", AT), page([account()]), AT - 5);
    expect(withOverview(state, page([], "signedOut"), AT)).toBe(NO_REACH);
    expect(withOverview(state, page([], "failed"), AT)).toBe(state);
    expect(withOverview(state, page([], "signedOut", false), AT)).toBe(state);
  });

  it("keeps when a machine first went away, and the watched account up to that moment", () => {
    const first = withAway(NO_REACH, "kz1", AT, { id: "a_TEST0000000000001", live: RUNNING });
    expect(first.known.a_TEST0000000000001).toEqual({ live: RUNNING, at: AT });
    const again = withAway(first, "kz1", AT + 60_000);
    expect(again).toBe(first);
    expect(withBack(again, "kz1").away).toEqual({});
    expect(withBack(NO_REACH, "kz1")).toBe(NO_REACH);
  });

  it("learns a streamed state, never an unknown one", () => {
    expect(withLive(NO_REACH, "a_1", UNKNOWN, AT)).toBe(NO_REACH);
    expect(withLive(NO_REACH, "a_1", RUNNING, AT).known.a_1).toEqual({ live: RUNNING, at: AT });
  });
});

describe("an account's reach", () => {
  const known: ReachState = withOverview(NO_REACH, page([account()]), AT);

  it("is live while its machine answers", () => {
    expect(reachOf(account(), known)).toEqual({ stale: false });
  });

  it("is the last known state while its machine is away or could not be asked", () => {
    const last = { stale: true, known: { live: RUNNING, at: AT } };
    expect(reachOf(account(), withAway(known, "kz1", AT + 1))).toEqual(last);
    expect(reachOf(account({ live: UNKNOWN }), known)).toEqual(last);
    expect(reachOf(account({ live: UNKNOWN }), NO_REACH)).toEqual({ stale: true, known: null });
    // Another machine's absence says nothing of this one.
    expect(reachOf(account(), withAway(known, "kz2", AT + 1))).toEqual({ stale: false });
  });

  it("never covers the service's own words: a hold, an expired account", () => {
    const away = withAway(known, "kz1", AT + 1);
    const held = account({ hold: { kind: "cause", cause: "wrongPassword", at: NOW, row: null } });
    expect(reachOf(held, away)).toEqual({ stale: false });
    expect(reachOf(account({ expired: true }), away)).toEqual({ stale: false });
  });
});

describe("the last known line", () => {
  it("names the state at its clock time, with the ending the time takes", () => {
    expect(lastKnownText({ live: RUNNING, at: AT }, t, AT)).toBe("Son bilinen: 14:05'te oyunda");
    const three = new Date(2026, 9, 28, 15, 0).getTime();
    const queued = { ...RUNNING, state: "queued" };
    expect(lastKnownText({ live: queued, at: three }, t, three)).toBe(
      "Son bilinen: 15:00'te sırada",
    );
  });

  it("reads a wait between attempts as connecting", () => {
    const waiting = { ...RUNNING, state: "waiting", attempt: 2, retryAt: NOW + 14 };
    expect(lastKnownText({ live: waiting, at: AT }, t, AT)).toBe(
      "Son bilinen: 14:05'te bağlanıyor",
    );
  });

  it("says the date of another day", () => {
    const tomorrow = AT + 86_400_000;
    expect(lastKnownText({ live: RUNNING, at: AT }, t, tomorrow)).toBe(
      "Son bilinen: 28 Ekim 14:05'te oyunda",
    );
  });

  it("says the state cannot be had when nothing is known", () => {
    expect(lastKnownText(null, t, AT)).toBe("Durumu şu an alınamıyor");
    const odd = { ...RUNNING, state: "unknown" };
    expect(lastKnownText({ live: odd, at: AT }, t, AT)).toBe("Durumu şu an alınamıyor");
  });

  it("is still and dim on the state line, the hold's words winning", () => {
    const stale = { stale: true as const, known: { live: RUNNING, at: AT } };
    const status = cloudStatus(account(), AT, t, "tr", stale);
    expect(status).toMatchObject({ signal: "idle", tone: "stale", nextChangeMs: null });
    // Its day is today's or not by the real clock: the time and the ending stay.
    expect(status.text).toMatch(/^Son bilinen: .*14:05'te oyunda$/);
    const held = account({ hold: { kind: "cause", cause: "wrongPassword", at: NOW, row: null } });
    expect(cloudStatus(held, AT, t, "tr", stale).text).toBe("Durdu: Giriş şifresi yanlış");
  });
});
