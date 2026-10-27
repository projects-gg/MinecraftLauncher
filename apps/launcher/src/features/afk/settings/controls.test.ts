// SHEET-UX: one Backspace or Delete beside or inside a reply's placeholder takes the whole of it.
import type { KeyboardEvent } from "react";
import { describe, expect, it, vi } from "vitest";
import { cloudTokens, deleteWholeToken, desktopTokens } from "./controls";

describe("token sets", () => {
  const cloud = cloudTokens("Yazanın adı", (n) => `${n}. parça`);
  const desktop = desktopTokens("Yazanın adı");

  it("show a reply's placeholders by name and store them back as they were", () => {
    const stored = "/msg {u} {1} davet listene eklendi.";
    const shown = cloud.toShown(stored);
    expect(shown).toBe("/msg {Yazanın adı} {1. parça} davet listene eklendi.");
    expect(cloud.toStored(shown)).toBe(stored);
    expect(desktop.toShown("Merhaba $u, sonra")).toBe("Merhaba {Yazanın adı}, sonra");
    expect(desktop.toStored("Merhaba {Yazanın adı}, sonra")).toBe("Merhaba $u, sonra");
  });

  it("leave other braces and a broken name as plain words", () => {
    expect(cloud.toStored("{Yazanın ad} {x} {10}")).toBe("{Yazanın ad} {x} {10}");
    expect(cloud.toShown("{x} {0} {10}")).toBe("{x} {0} {10}");
    expect("Selam {Yazanın adı}!".split(cloud.shown)).toEqual(["Selam ", "{Yazanın adı}", "!"]);
  });
});

const CLOUD = /(\{(?:u|[1-9])\})/;
const DESKTOP = /(\$u)/;

/** The key press as the field sees it, the caret at `caret` (or a selection to `end`). */
function press(key: string, value: string, caret: number, end = caret, mods = {}) {
  const input = {
    value,
    selectionStart: caret,
    selectionEnd: end,
    setSelectionRange: vi.fn(),
  };
  const event = {
    key,
    ctrlKey: false,
    altKey: false,
    metaKey: false,
    ...mods,
    currentTarget: input,
    preventDefault: vi.fn(),
  } as unknown as KeyboardEvent<HTMLInputElement>;
  const onText = vi.fn();
  deleteWholeToken(event, value.includes("$u") ? DESKTOP : CLOUD, onText);
  return {
    onText,
    prevented: (event.preventDefault as ReturnType<typeof vi.fn>).mock.calls.length,
  };
}

describe("deleteWholeToken", () => {
  vi.stubGlobal("requestAnimationFrame", (run: () => void) => {
    run();
    return 0;
  });

  it("takes a placeholder whole with Backspace right after it or inside it", () => {
    expect(press("Backspace", "Selam {u}!", 9).onText).toHaveBeenCalledWith("Selam !");
    expect(press("Backspace", "Selam {u}!", 8).onText).toHaveBeenCalledWith("Selam !");
    expect(press("Backspace", "/msg {u} {1} hey", 12).onText).toHaveBeenCalledWith("/msg {u}  hey");
  });

  it("takes it whole with Delete right before it or inside it", () => {
    expect(press("Delete", "Selam {u}!", 6).onText).toHaveBeenCalledWith("Selam !");
    expect(press("Delete", "Merhaba $u, sonra", 8).onText).toHaveBeenCalledWith("Merhaba , sonra");
  });

  it("leaves other presses to the field", () => {
    // Before the token with Backspace, after it with Delete, a selection, a shortcut, a plain key.
    expect(press("Backspace", "Selam {u}!", 6).prevented).toBe(0);
    expect(press("Delete", "Selam {u}!", 9).prevented).toBe(0);
    expect(press("Backspace", "Selam {u}!", 6, 9).prevented).toBe(0);
    expect(press("Backspace", "Selam {u}!", 9, 9, { ctrlKey: true }).prevented).toBe(0);
    expect(press("a", "Selam {u}!", 9).prevented).toBe(0);
    expect(press("Backspace", "Selam {x}!", 9).prevented).toBe(0);
  });
});
