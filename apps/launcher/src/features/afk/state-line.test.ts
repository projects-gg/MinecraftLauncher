import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import { tr } from "@/locales/tr";
import { trAfkConnect } from "@/locales/tr-afk-connect";
import { primaryAction } from "./primary-action";
import { failedText, stateSignal } from "./state-line";

// The Turkish strings with the AFK connect texts, looked up like i18next does.
const strings: Record<string, unknown> = { ...tr, afkConnect: trAfkConnect };
const t = ((key: string) => {
  let value: unknown = strings;
  for (const part of key.split(".")) value = (value as Record<string, unknown> | undefined)?.[part];
  if (typeof value !== "string") throw new Error(`no text for ${key}`);
  return value;
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
