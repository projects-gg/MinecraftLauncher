import { playButtonName, playPress } from "@pl/ui";
import { describe, expect, it } from "vitest";

describe("playPress", () => {
  const press = { cancellable: true, armed: true, cancelShown: true, pointer: true };

  it("starts from the gold faces", () => {
    expect(playPress("ready", press)).toBe("play");
    expect(playPress("update", { ...press, armed: false })).toBe("play");
  });

  it("cancels a launch whose İptal is on the button", () => {
    expect(playPress("working", press)).toBe("cancel");
    expect(playPress("opening", press)).toBe("cancel");
  });

  it("shows İptal first under a pointer that rested where it pressed OYNA", () => {
    // The button says "%42" or "Açılıyor": the press shows "İptal" and does nothing more.
    expect(playPress("working", { ...press, cancelShown: false })).toBe("showCancel");
    expect(playPress("opening", { ...press, cancelShown: false })).toBe("showCancel");
  });

  it("cancels from the keyboard, whose name for the button leads with İptal", () => {
    expect(playPress("working", { ...press, cancelShown: false, pointer: false })).toBe("cancel");
  });

  it("leaves the moment after OYNA to the press guard", () => {
    // Not armed: the guard ignores the press (the rest of the double click).
    expect(playPress("working", { ...press, armed: false, cancelShown: false })).toBe("cancel");
  });

  it("does nothing while the game runs or cannot be stopped", () => {
    expect(playPress("running", press)).toBe("nothing");
    expect(playPress("unavailable", press)).toBe("nothing");
    expect(playPress("opening", { ...press, cancellable: false })).toBe("nothing");
  });
});

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
