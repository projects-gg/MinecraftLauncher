import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import { tr } from "@/locales/tr";
import { trCloudAfk } from "@/locales/tr-cloud-afk";
import type { CloudAccount } from "./api";
import {
  causeText,
  cloudPrimary,
  cloudStatus,
  formatPrice,
  isCloudActive,
  isInGame,
  localized,
  serverLabel,
  timeLeft,
} from "./status";

// The Turkish strings with the cloud texts, looked up like i18next does (plural forms included).
const strings: Record<string, unknown> = { ...tr, cloudAfk: trCloudAfk };
const lookup = (key: string): unknown => {
  let value: unknown = strings;
  for (const part of key.split(".")) value = (value as Record<string, unknown> | undefined)?.[part];
  return value;
};
const t = ((key: string, values?: Record<string, unknown>) => {
  let value = lookup(key);
  if (value === undefined && typeof values?.count === "number") {
    value = lookup(`${key}_${values.count === 1 ? "one" : "other"}`);
  }
  if (typeof value !== "string") throw new Error(`no text for ${key}`);
  return value.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(values?.[name]));
}) as TFunction;

const NOW = 1_800_000_000;
const DAY = 86_400;

function account(overrides: Partial<CloudAccount> = {}): CloudAccount {
  const expiresAt = overrides.expiresAt ?? NOW + 18 * DAY + 3_600;
  return {
    id: "a_TEST0000000000001",
    name: "Madenci07",
    server: { host: "mc.madenciler.net", port: 25570 },
    version: "26.3",
    lang: "tr",
    host: "kz1",
    createdAt: NOW - 12 * DAY,
    expiresAt,
    expired: false,
    graceUntil: expiresAt + 30 * DAY,
    renewal: { open: false, opensAt: expiresAt - 7 * DAY },
    desired: "run",
    hold: null,
    live: { state: "running", since: NOW - 3_600, reason: null, attempt: null, retryAt: null },
    everJoined: true,
    config: {
      v: 1,
      owner: "",
      antiAfk: { enabled: true, delayMin: 60, delayMax: 90 },
      commands: [],
      rules: [],
    },
    rev: 4,
    serverChanges: { used: 0, limit: 6, resetAt: null },
    ...overrides,
  };
}

const live = (state: string, extra: Partial<CloudAccount["live"]> = {}): CloudAccount["live"] => ({
  state,
  since: null,
  reason: null,
  attempt: null,
  retryAt: null,
  ...extra,
});

describe("cloudStatus", () => {
  it("lets a hold win over the live state", () => {
    const status = cloudStatus(
      account({ hold: { kind: "cause", cause: "wrongPassword", at: NOW - 60 } }),
      NOW * 1000,
      t,
    );
    expect(status.signal).toBe("offline");
    expect(status.tone).toBe("error");
    expect(status.text).toBe("Durdu: Giriş şifresi yanlış");
    expect(status.hint).toBe(trCloudAfk.hint.wrongPassword);
    expect(status.nextChangeMs).toBeNull();
  });

  it("reads an unknown stop cause as a plain stop and gives no hint for a ban", () => {
    expect(causeText("somethingNew", t)).toBe(trCloudAfk.cause.other);
    const banned = cloudStatus(
      account({ hold: { kind: "cause", cause: "banned", at: NOW } }),
      NOW * 1000,
      t,
    );
    expect(banned.hint).toBeNull();
  });

  it("names Studio's reason of a blocked server in the player's language, the other one if empty", () => {
    const blocked = account({
      hold: { kind: "server_blocked", reason: { tr: "", en: "Not allowed" } },
    });
    expect(cloudStatus(blocked, NOW * 1000, t, "tr").text).toContain("Not allowed");
    const quiet = account({ hold: { kind: "server_blocked", reason: null } });
    expect(cloudStatus(quiet, NOW * 1000, t).text).toBe(trCloudAfk.state.serverBlocked);
  });

  it("tells a paused service from a closed one and keeps an unknown hold generic", () => {
    const paused = account({ hold: { kind: "paused", mode: "paused" } });
    const off = account({ hold: { kind: "paused", mode: "off" } });
    expect(cloudStatus(paused, NOW * 1000, t).text).toBe(trCloudAfk.state.paused);
    expect(cloudStatus(off, NOW * 1000, t).text).toBe(trCloudAfk.state.off);
    const unknown = account({ hold: { kind: "other" } });
    expect(cloudStatus(unknown, NOW * 1000, t)).toMatchObject({
      text: trCloudAfk.state.stopped,
      tone: "muted",
    });
  });

  it("shows an ended account as expired whatever its live state says", () => {
    const status = cloudStatus(account({ expired: true }), NOW * 1000, t);
    expect(status).toMatchObject({ text: trCloudAfk.state.expired, tone: "warn" });
  });

  it("counts the time in the game and wakes up when the shown minute changes", () => {
    const status = cloudStatus(
      account({ live: live("running", { since: NOW - 3_600 - 30 }) }),
      NOW * 1000,
      t,
    );
    expect(status.signal).toBe("online");
    expect(status.tone).toBe("ok");
    expect(status.nextChangeMs).toBe(30_000);
  });

  it("counts down to the next try and then reads as connecting", () => {
    const waiting = account({ live: live("waiting", { retryAt: NOW + 11, attempt: 2 }) });
    const status = cloudStatus(waiting, NOW * 1000, t);
    expect(status.text).toBe("Yeniden bağlanacak, 11 sn");
    expect(status.nextChangeMs).toBe(1000);
    const due = cloudStatus(waiting, (NOW + 10.5) * 1000, t);
    expect(due).toMatchObject({ signal: "pending", text: trCloudAfk.state.connecting });
  });

  it("says why an account waits for a slot", () => {
    const full = account({ live: live("queued", { reason: "target_cap" }) });
    expect(cloudStatus(full, NOW * 1000, t).text).toBe(trCloudAfk.state.queuedFull);
    const queued = account({ live: live("queued") });
    expect(cloudStatus(queued, NOW * 1000, t).text).toBe(trCloudAfk.state.queued);
  });
});

describe("cloudPrimary", () => {
  it("renews an ended account and offers nothing while it is suspended", () => {
    expect(cloudPrimary(account({ expired: true }))).toBe("renew");
    expect(cloudPrimary(account({ hold: { kind: "expired", at: NOW } }))).toBe("renew");
    expect(
      cloudPrimary(account({ hold: { kind: "suspended", until: null, reason: "spam" } })),
    ).toBe("none");
  });

  it("stops a running account and starts one that stopped for good", () => {
    expect(cloudPrimary(account())).toBe("stop");
    expect(cloudPrimary(account({ desired: "stop", live: live("stopped") }))).toBe("start");
    expect(
      cloudPrimary(account({ hold: { kind: "cause", cause: "wrongPassword", at: NOW } })),
    ).toBe("start");
    expect(cloudPrimary(account({ hold: { kind: "paused", mode: "paused" } }))).toBe("stop");
  });
});

describe("isCloudActive and isInGame", () => {
  it("count an account as active only while it should run and nothing holds it", () => {
    expect(isCloudActive(account())).toBe(true);
    expect(isInGame(account())).toBe(true);
    expect(isInGame(account({ live: live("connecting") }))).toBe(false);
    expect(isCloudActive(account({ desired: "stop" }))).toBe(false);
    expect(isCloudActive(account({ hold: { kind: "no_host", since: NOW } }))).toBe(false);
  });
});

describe("timeLeft", () => {
  it("counts whole days, then hours, then minutes", () => {
    expect(timeLeft(account({ expiresAt: NOW + 5 * DAY + 100 }), NOW * 1000, t)).toBe(
      "5 gün kaldı",
    );
    expect(timeLeft(account({ expiresAt: NOW + 5 * 3_600 + 10 }), NOW * 1000, t)).toBe(
      "5 saat kaldı",
    );
    expect(timeLeft(account({ expiresAt: NOW + 30 }), NOW * 1000, t)).toBe("1 dakika kaldı");
  });

  it("tells an ended account how long its settings are kept", () => {
    const ended = account({ expired: true, expiresAt: NOW - DAY, graceUntil: NOW + 27.5 * DAY });
    expect(timeLeft(ended, NOW * 1000, t)).toBe("Ayarları 28 gün daha saklanır");
  });
});

describe("formatPrice, serverLabel and localized", () => {
  it("drops zero kuruş and writes the rest the Turkish way", () => {
    expect(formatPrice("60.00")).toBe("60 TL");
    expect(formatPrice("59.90")).toBe("59,90 TL");
    expect(formatPrice("abc")).toBe("abc TL");
  });

  it("shows a port only when it is not the default", () => {
    expect(serverLabel({ host: "play.projects.gg", port: 25565 })).toBe("play.projects.gg");
    expect(serverLabel({ host: "play.projects.gg", port: 0 })).toBe("play.projects.gg");
    expect(serverLabel({ host: "mc.madenciler.net", port: 25570 })).toBe("mc.madenciler.net:25570");
  });

  it("falls back to the other language and to nothing", () => {
    expect(localized({ tr: "Bakım", en: "Maintenance" }, "en")).toBe("Maintenance");
    expect(localized({ tr: "Bakım", en: " " }, "en")).toBe("Bakım");
    expect(localized({ tr: "", en: "" }, "tr")).toBeNull();
    expect(localized(null, "tr")).toBeNull();
  });
});
