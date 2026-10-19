import { playButtonName } from "@pl/ui";
import { describe, expect, it } from "vitest";

const labels = { play: "Oyna", update: "Güncelle", cancel: "İptal", running: "Oyunda" };

describe("playButtonName", () => {
  const working = { kind: "working", percent: 42.7, label: "Oyun dosyaları indiriliyor" } as const;

  it("leads with cancelling once a press would cancel the launch", () => {
    expect(playButtonName(working, labels, true)).toBe("İptal, Oyun dosyaları indiriliyor %42");
  });

  it("is the progress alone while a press does nothing", () => {
    expect(playButtonName(working, labels, false)).toBe("Oyun dosyaları indiriliyor %42");
  });

  it("offers to cancel a game that opens, once a press would", () => {
    const opening = { kind: "opening", label: "Açılıyor" } as const;
    expect(playButtonName(opening, labels, true)).toBe("İptal, Açılıyor");
    expect(playButtonName(opening, labels, false)).toBe("Açılıyor");
  });

  it("names the other states by their visible label", () => {
    expect(playButtonName({ kind: "ready" }, labels, true)).toBe("Oyna");
    expect(playButtonName({ kind: "update" }, labels, true)).toBe("Güncelle");
    expect(playButtonName({ kind: "running" }, labels, true)).toBe("Oyunda");
    expect(playButtonName({ kind: "unavailable", label: "Hesap yok" }, labels, true)).toBe(
      "Hesap yok",
    );
  });
});
