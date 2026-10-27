import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import type { AfkState } from "@/lib/api";
import { tr } from "@/locales/tr";
import { trAfkConnect } from "@/locales/tr-afk-connect";
import { trAfkReconnect } from "@/locales/tr-afk-reconnect";
import { primaryAction } from "./primary-action";
import { failedText, nextChangeMs, reconnectSecs, stateSignal, stateText } from "./state-line";

// The Turkish strings with the AFK connect texts, looked up like i18next does.
const strings: Record<string, unknown> = {
  ...tr,
  afkConnect: trAfkConnect,
  afkReconnect: trAfkReconnect,
};
const t = ((key: string, values?: Record<string, unknown>) => {
  let value: unknown = strings;
  for (const part of key.split(".")) value = (value as Record<string, unknown> | undefined)?.[part];
  if (typeof value !== "string") throw new Error(`no text for ${key}`);
  return value.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(values?.[name]));
}) as TFunction;

describe("stateSignal", () => {
  it("lights the bars in the game and crosses them when the connection dropped", () => {
    expect(stateSignal({ kind: "connected", since: "2026-10-18T10:00:00Z" })).toBe("online");
    expect(stateSignal({ kind: "error", reason: "Sunucu kapandı" })).toBe("offline");
  });

  it("sweeps only while a connection is being made", () => {
    expect(stateSignal({ kind: "starting" })).toBe("pending");
    expect(stateSignal({ kind: "connecting" })).toBe("pending");
    expect(stateSignal({ kind: "queued", inSecs: 15 })).toBe("idle");
    expect(
      stateSignal({ kind: "reconnecting", attempt: 2, nextAttemptAt: "2026-10-18T10:00:20Z" }),
    ).toBe("idle");
    expect(stateSignal({ kind: "stopping" })).toBe("idle");
    expect(stateSignal({ kind: "stopped" })).toBe("idle");
  });

  it("sweeps while the client is fetched and crosses when it could not be had", () => {
    expect(stateSignal({ kind: "preparing", step: "downloading", percent: 40 })).toBe("pending");
    expect(stateSignal({ kind: "clientFailed", problem: "offline" })).toBe("offline");
  });
});

describe("failedText", () => {
  it("says what happened to the client, without a code", () => {
    expect(failedText({ kind: "clientFailed", problem: "notPublished" }, t)).toBe(
      "AFK istemcisi henüz hazır değil",
    );
    expect(failedText({ kind: "clientFailed", problem: "offline" }, t)).toBe(
      "AFK istemcisi indirilemedi",
    );
  });

  it("puts the drop before its reason unless the reason says it all", () => {
    expect(failedText({ kind: "error", reason: "Sunucudan atıldı" }, t)).toBe(
      "Bağlantı kesildi: Sunucudan atıldı",
    );
    expect(
      failedText({ kind: "error", reason: "AFK istemcisi başlatılamadı", standalone: true }, t),
    ).toBe("AFK istemcisi başlatılamadı");
    expect(failedText({ kind: "stopped" }, t)).toBeNull();
  });
});

describe("a stopped account that starts with the launcher", () => {
  it("says it connects when the launcher opens, only while stopped", () => {
    const at = Date.parse("2026-10-25T20:00:00Z");
    expect(stateText({ kind: "stopped" }, at, t)).toBe("Durduruldu");
    expect(stateText({ kind: "stopped" }, at, t, true)).toBe(
      "Durduruldu, launcher yeniden açılınca bağlanır",
    );
    expect(stateText({ kind: "connecting" }, at, t, true)).toBe("Bağlanıyor");
  });
});

describe("primaryAction", () => {
  it("connects an idle account and stops a running one", () => {
    expect(primaryAction({ kind: "stopped" })).toBe("connect");
    expect(primaryAction({ kind: "error", reason: "Beyaz listede değil" })).toBe("connect");
    expect(primaryAction({ kind: "connected", since: "2026-10-18T10:00:00Z" })).toBe("stop");
    expect(primaryAction({ kind: "queued", inSecs: 15 })).toBe("stop");
  });

  it("cancels while the client is fetched and tries again after it could not be had", () => {
    expect(primaryAction({ kind: "preparing", step: "checking" })).toBe("cancel");
    expect(primaryAction({ kind: "clientFailed", problem: "broken" })).toBe("retry");
  });
});

describe("reconnect countdown", () => {
  const at = Date.parse("2026-10-19T20:00:20Z");
  const next = new Date(at).toISOString();
  const waiting: AfkState = { kind: "reconnecting", attempt: 2, nextAttemptAt: next };

  it("counts the whole seconds left down to 1", () => {
    expect(reconnectSecs(next, at - 4300)).toBe(5);
    expect(reconnectSecs(next, at - 4000)).toBe(4);
    expect(reconnectSecs(next, at - 1)).toBe(1);
    expect(reconnectSecs(next, at + 500)).toBe(1);
  });

  it("renders again on each whole second and not in between", () => {
    expect(nextChangeMs(waiting, at - 4300)).toBe(300);
    expect(nextChangeMs(waiting, at - 4000)).toBe(1000);
    expect(nextChangeMs(waiting, at - 900)).toBeNull();
  });

  it("reads the clock with the state it shows", () => {
    // E2E3: "15 sn" for ~100 ms between two reconnects.
    const third: AfkState = {
      kind: "reconnecting",
      attempt: 3,
      nextAttemptAt: new Date(at + 11_000).toISOString(),
    };
    expect(stateText(third, at, t)).toBe("Yeniden bağlanacak, 11 sn");
  });

  it("never guesses a number before the backend knows when", () => {
    const pending: AfkState = { kind: "reconnecting", attempt: 2, nextAttemptAt: null };
    expect(stateText(pending, at, t)).toBe("Yeniden bağlanıyor");
    expect(nextChangeMs(pending, at)).toBeNull();
  });
});

describe("uptime", () => {
  const since = "2026-10-19T20:00:00Z";
  const start = Date.parse(since);
  const connected: AfkState = { kind: "connected", since };

  it("renders on each second of the first minute and on each minute after it", () => {
    // 3.2 s shows "3 sn"; "4 sn" from 3.5 s, where the rounded seconds change.
    expect(nextChangeMs(connected, start + 3200)).toBe(300);
    expect(nextChangeMs(connected, start + 3500)).toBe(1000);
    // 59.6 s already shows "1 dk"; "2 dk" from 119.5 s.
    expect(nextChangeMs(connected, start + 59_600)).toBe(59_900);
    expect(nextChangeMs(connected, start + 125_000)).toBe(54_500);
  });

  it("does not tick for the other states", () => {
    expect(nextChangeMs({ kind: "connecting" }, start)).toBeNull();
    expect(nextChangeMs({ kind: "queued", inSecs: 15 }, start)).toBeNull();
  });
});
