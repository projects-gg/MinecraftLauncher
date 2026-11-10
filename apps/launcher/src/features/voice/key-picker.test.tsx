// @vitest-environment happy-dom
// The voice key picker (VOICE-KEYS): mute and deafen record a combination as it is pressed (Ctrl,
// Shift, Alt show while held and wait for the key; one let go alone is the key itself), push-to-talk
// takes the first key; a combination shows as caps "Ctrl + Shift + M".
import { setAppReducedMotion } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { voiceApi } from "@/lib/api/voice-api";
import type { VoiceKey, VoiceShortcut } from "@/lib/api/voice-types";
import { KeyPicker } from "./key-picker";

beforeAll(() => setAppReducedMotion(true));
afterAll(() => setAppReducedMotion(false));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function picker(shortcut: VoiceShortcut, value: VoiceKey | null = null) {
  vi.spyOn(voiceApi, "checkKey").mockResolvedValue({ usable: true, clash: null });
  const onPick = vi.fn();
  render(
    <KeyPicker
      value={value}
      shortcut={shortcut}
      label="Kısayol"
      onPick={onPick}
      onRefused={() => {}}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /Tuş Seç|Değiştir/ }));
  return onPick;
}

const down = (code: string, key: string, held: Partial<KeyboardEventInit> = {}) =>
  fireEvent.keyDown(window, { code, key, ...held });
const up = (code: string, key: string, held: Partial<KeyboardEventInit> = {}) =>
  fireEvent.keyUp(window, { code, key, ...held });

describe("the voice key picker", () => {
  it("records Ctrl + Shift + M for mute, showing what is held meanwhile", async () => {
    const onPick = picker("mute");
    down("ControlLeft", "Control", { ctrlKey: true });
    down("ShiftLeft", "Shift", { ctrlKey: true, shiftKey: true });
    expect(screen.getByText("Ctrl")).toBeTruthy();
    expect(screen.getByText("Shift")).toBeTruthy();
    expect(onPick).not.toHaveBeenCalled();
    await act(async () => {
      down("KeyM", "M", { ctrlKey: true, shiftKey: true });
    });
    expect(onPick).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "keyboard", code: "KeyM", mods: ["ctrl", "shift"] }),
    );
  });

  it("takes a modifier let go alone as the key itself", async () => {
    const onPick = picker("deafen");
    down("ControlRight", "Control", { ctrlKey: true });
    await act(async () => {
      up("ControlRight", "Control");
    });
    expect(onPick).toHaveBeenCalledWith({
      kind: "keyboard",
      code: "ControlRight",
      label: "Control",
    });
  });

  it("takes push-to-talk's first key, never with modifiers", async () => {
    const onPick = picker("pushToTalk");
    await act(async () => {
      down("ControlLeft", "Control", { ctrlKey: true });
    });
    expect(onPick).toHaveBeenCalledWith({
      kind: "keyboard",
      code: "ControlLeft",
      label: "Control",
    });
  });

  it("shows a combination as caps", () => {
    vi.spyOn(voiceApi, "checkKey").mockResolvedValue({ usable: true, clash: null });
    render(
      <KeyPicker
        value={{ kind: "keyboard", code: "KeyM", label: "M", mods: ["ctrl", "shift"] }}
        shortcut="mute"
        label="Mikrofonu Kapat/Aç"
        onPick={() => {}}
        onRefused={() => {}}
      />,
    );
    expect(
      screen.getByRole("button", { name: "Mikrofonu Kapat/Aç: Ctrl + Shift + M, Değiştir" }),
    ).toBeTruthy();
    expect(screen.getAllByText("+")).toHaveLength(2);
  });
});
