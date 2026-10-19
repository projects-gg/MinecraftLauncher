import { describe, expect, it } from "vitest";
import type { ClientProblem, FailureReason } from "@/bindings";
import { enAfkConnect } from "@/locales/en-afk-connect";
import { tr } from "@/locales/tr";
import { trAfkConnect } from "@/locales/tr-afk-connect";
import { clientState, downloadPercent, failureText, shownInPlace } from "./afk-client";

/** The Turkish strings with the AFK connect texts, looked up like i18next does (no fallback). */
const strings: Record<string, unknown> = { ...tr, afkConnect: trAfkConnect };
const t = (key: string) => {
  let value: unknown = strings;
  for (const part of key.split(".")) value = (value as Record<string, unknown> | undefined)?.[part];
  if (typeof value !== "string") throw new Error(`no text for ${key}`);
  return value;
};

const PROBLEMS: ClientProblem[] = ["notPublished", "offline", "diskFull", "blocked", "broken"];

describe("clientState", () => {
  it("leaves the state to the session without a client step", () => {
    expect(clientState(null, "stopped")).toBeNull();
    expect(clientState(undefined, "error")).toBeNull();
  });

  it("shows the client while an idle account waits for it", () => {
    expect(clientState({ step: "checking" }, "stopped")).toEqual({
      kind: "preparing",
      step: "checking",
    });
    expect(clientState({ step: "downloading", done: 25, total: 100 }, "stopped")).toEqual({
      kind: "preparing",
      step: "downloading",
      percent: 25,
    });
    // An earlier session error gives way to the new attempt.
    expect(clientState({ step: "installing" }, "error")).toEqual({
      kind: "preparing",
      step: "installing",
    });
  });

  it("turns a failure into the account's state", () => {
    expect(clientState({ step: "failed", problem: "offline" }, "stopped")).toEqual({
      kind: "clientFailed",
      problem: "offline",
    });
  });

  it("lets a session that runs speak", () => {
    expect(clientState({ step: "failed", problem: "offline" }, "connecting")).toBeNull();
    expect(clientState({ step: "checking" }, "starting")).toBeNull();
  });
});

describe("downloadPercent", () => {
  it("has no percent before the size is known", () => {
    expect(downloadPercent(10, 0)).toBeUndefined();
  });

  it("stays within 0 and 100", () => {
    expect(downloadPercent(50, 200)).toBe(25);
    expect(downloadPercent(300, 200)).toBe(100);
  });
});

describe("failureText", () => {
  it("never shows the backend's technical message for a launch failure", () => {
    const reasons: FailureReason[] = [
      {
        kind: "launchFailed",
        message: "I/O error at C:\\afk\\ProjectsKonsol.ini: Access is denied.",
      },
      { kind: "clientMissing" },
      { kind: "secretsUnavailable" },
      { kind: "unexpectedExit", code: -1073741819 },
    ];
    const texts = reasons.map((reason) => failureText(reason, t));
    expect(texts).toEqual([
      { reason: "AFK istemcisi başlatılamadı", standalone: true },
      { reason: "AFK istemcisi bulunamadı", standalone: true },
      { reason: "Giriş komutları okunamadı", standalone: true },
      { reason: "AFK istemcisi beklenmedik şekilde kapandı", standalone: true },
    ]);
    for (const { reason } of texts) expect(reason).not.toMatch(/error|denied|-\d|\\/i);
  });

  it("keeps the server's own words in brackets", () => {
    expect(
      failureText({ kind: "refused", cause: "banned", message: "You are banned!" }, t),
    ).toEqual({ reason: "Sunucudan yasaklandı (You are banned!)", standalone: false });
    expect(failureText({ kind: "kicked", message: null }, t)).toEqual({
      reason: "Sunucudan atıldı",
      standalone: false,
    });
  });

  it("does not say the drop twice", () => {
    // The state line would read "Bağlantı kesildi: Bağlantı kesildi".
    expect(failureText({ kind: "disconnected", message: null }, t)).toEqual({
      reason: "Bağlantı kesildi",
      standalone: true,
    });
    expect(failureText(null, t)).toEqual({ reason: "Bağlantı kesildi", standalone: true });
    expect(failureText({ kind: "failed", message: "Sunucuya bağlanılamadı" }, t)).toEqual({
      reason: "Bağlanamadı (Sunucuya bağlanılamadı)",
      standalone: true,
    });
  });
});

describe("shownInPlace", () => {
  it("keeps the client's failures and cancels out of toasts", () => {
    expect(shownInPlace({ code: "AFK-ISTEMCI", message: "x", details: "y" })).toBe(true);
    expect(shownInPlace({ code: "IPTAL", message: "x", details: "y" })).toBe(true);
    expect(shownInPlace({ code: "AFK-HESAP-YOK", message: "x", details: "y" })).toBe(false);
    expect(shownInPlace(new Error("x"))).toBe(false);
    expect(shownInPlace(null)).toBe(false);
  });
});

describe("AFK connect texts", () => {
  it("word every problem in both languages, what happened and what to do", () => {
    for (const problem of PROBLEMS) {
      for (const texts of [trAfkConnect, enAfkConnect]) {
        expect(texts.problem[problem].title.length).toBeGreaterThan(0);
        expect(texts.problem[problem].hint).toMatch(/\.$/);
      }
    }
  });

  it("show no codes or technical words", () => {
    const all = JSON.stringify([trAfkConnect, enAfkConnect]);
    expect(all).not.toMatch(/AFK-[A-Z]|IPTAL|ClientNotInstalled|not installed|error/);
  });

  it("have the same keys in English", () => {
    const keys = (value: unknown, prefix = ""): string[] =>
      value && typeof value === "object"
        ? Object.entries(value).flatMap(([key, child]) => keys(child, `${prefix}${key}.`))
        : [prefix];
    expect(keys(enAfkConnect).sort()).toEqual(keys(trAfkConnect).sort());
  });
});
