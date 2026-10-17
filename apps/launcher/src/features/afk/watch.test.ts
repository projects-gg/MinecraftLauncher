import { describe, expect, it } from "vitest";
import type { AfkAccount, AfkState } from "@/lib/api";
import { trAfkAlerts } from "@/locales/tr-afk-alerts";
import { type AfkSeen, findStops, seenOf, shortReason, stopNotice } from "./watch";

// The Turkish strings themselves, with i18next's {{name}} interpolation.
const t = ((key: string, options: Record<string, unknown> = {}) => {
  let value: unknown = trAfkAlerts;
  for (const part of key.replace(/^afkAlerts\./, "").split(".")) {
    value = (value as Record<string, unknown>)[part];
  }
  return String(value).replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(options[name]));
}) as never;

const connected: AfkState = { kind: "connected", since: "2026-10-17T02:00:00Z" };
const reconnecting: AfkState = {
  kind: "reconnecting",
  attempt: 3,
  nextAttemptAt: "2026-10-17T02:00:20Z",
};
const banned: AfkState = {
  kind: "error",
  reason: "Sunucudan yasaklandı (You are banned from this server!)",
};
const whitelist: AfkState = { kind: "error", reason: "Beyaz listede değil" };

function account(id: string, state: AfkState): AfkAccount {
  return {
    id,
    nickname: id,
    server: "play.projects.gg",
    autoStart: true,
    mcVersion: "26.3",
    mcVersionRecommended: true,
    state,
  };
}

/** The stops between two reads. */
function step(before: AfkAccount[], after: AfkAccount[]) {
  return findStops(seenOf(before), after).map((stop) => `${stop.name}: ${stop.reason}`);
}

describe("findStops", () => {
  it("says nothing on the first read, even about accounts already stopped", () => {
    expect(findStops(null, [account("KasaBekcisi", banned)])).toEqual([]);
  });

  it("reports an account that steps into an error, with the short reason", () => {
    expect(step([account("KasaBekcisi", connected)], [account("KasaBekcisi", banned)])).toEqual([
      "KasaBekcisi: Sunucudan yasaklandı",
    ]);
  });

  it("reports the end of a reconnect loop (repeated kicks stop the account)", () => {
    expect(step([account("Tarla", reconnecting)], [account("Tarla", whitelist)])).toEqual([
      "Tarla: Beyaz listede değil",
    ]);
  });

  it("says nothing about reconnects and stops the player asked for", () => {
    expect(step([account("Tarla", connected)], [account("Tarla", reconnecting)])).toEqual([]);
    expect(step([account("Tarla", connected)], [account("Tarla", { kind: "stopping" })])).toEqual(
      [],
    );
    expect(
      step([account("Tarla", { kind: "stopping" })], [account("Tarla", { kind: "stopped" })]),
    ).toEqual([]);
    expect(
      step([account("Tarla", { kind: "queued", inSecs: 15 })], [account("Tarla", connected)]),
    ).toEqual([]);
  });

  it("reports an account that stays in error once", () => {
    // The disconnect and the client's exit each set the error; the second one is no news.
    expect(step([account("Kasa", banned)], [account("Kasa", whitelist)])).toEqual([]);
    expect(step([account("Kasa", banned)], [account("Kasa", banned)])).toEqual([]);
  });

  it("reports a new attempt that stops again", () => {
    let seen: AfkSeen | null = null;
    const reads: AfkState[] = [banned, { kind: "starting" }, { kind: "connecting" }, banned];
    const found = reads.map((state) => {
      const accounts = [account("Kasa", state)];
      const stops = findStops(seen, accounts);
      seen = seenOf(accounts);
      return stops.length;
    });
    expect(found).toEqual([0, 0, 0, 1]);
  });

  it("looks at each account on its own", () => {
    const before = [account("A", connected), account("B", banned), account("C", connected)];
    const after = [account("A", banned), account("B", banned), account("C", reconnecting)];
    expect(step(before, after)).toEqual(["A: Sunucudan yasaklandı"]);
  });
});

describe("shortReason", () => {
  it("drops the server's message", () => {
    expect(shortReason("Sunucudan yasaklandı (You are banned from this server!)")).toBe(
      "Sunucudan yasaklandı",
    );
    expect(shortReason("Sunucudan atıldı (Sebep (küfür): 7 gün)")).toBe("Sunucudan atıldı");
  });

  it("keeps a reason without one", () => {
    expect(shortReason("Giriş şifresi yanlış")).toBe("Giriş şifresi yanlış");
    expect(shortReason("(boş)")).toBe("(boş)");
  });
});

describe("stopNotice", () => {
  const stop = (name: string, reason: string) => ({ id: name, name, reason });

  it("names the account and the reason", () => {
    expect(stopNotice([stop("KasaBekcisi", "Sunucudan yasaklandı")], t)).toEqual({
      title: "KasaBekcisi durdu",
      body: "Sunucudan yasaklandı",
    });
  });

  it("joins accounts that stopped together", () => {
    expect(
      stopNotice([stop("A", "Beyaz listede değil"), stop("B", "Beyaz listede değil")], t),
    ).toEqual({ title: "2 AFK hesabı durdu", body: "Beyaz listede değil: A, B" });
    expect(
      stopNotice([stop("A", "Sunucudan yasaklandı"), stop("B", "Giriş süresi doldu")], t),
    ).toEqual({ title: "2 AFK hesabı durdu", body: "A, B" });
  });
});
