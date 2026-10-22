import { afterEach, describe, expect, it, vi } from "vitest";
import { editEscape, leaveOnEscape } from "./edit-escape";

const press = (key: string, extra: { isComposing?: boolean; keyCode?: number } = {}) => ({
  key,
  isComposing: false,
  keyCode: key === "Escape" ? 27 : 13,
  ...extra,
});

/** A keydown as the window sees it first, with what the listener did to it. */
function keydown(
  key: string,
  target: unknown,
  extra: { isComposing?: boolean; keyCode?: number } = {},
) {
  const event = new Event("keydown", { cancelable: true });
  Object.defineProperties(event, {
    key: { value: key },
    isComposing: { value: extra.isComposing ?? false },
    keyCode: { value: extra.keyCode ?? (key === "Escape" ? 27 : 13) },
    target: { value: target },
  });
  const stopped = vi.spyOn(event, "stopPropagation");
  return { event, stopped };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("Esc in an edit", () => {
  it("leaves the edit, never the panel around it", () => {
    expect(editEscape(press("Escape"), true)).toBe("leave");
  });

  it("is the IME's while it composes", () => {
    expect(editEscape(press("Escape", { isComposing: true }), true)).toBe("composing");
    // WebKit: the Esc that ended a composition comes after it, with the IME's key code.
    expect(editEscape(press("Escape", { keyCode: 229 }), true)).toBe("composing");
  });

  it("keeps its old meaning outside the edit, and other keys are not ours", () => {
    expect(editEscape(press("Escape"), false)).toBe("pass");
    expect(editEscape(press("Enter"), true)).toBe("pass");
    expect(editEscape(press("Process", { isComposing: true, keyCode: 229 }), true)).toBe("pass");
  });

  it("is heard on the window first and goes no further", () => {
    const view = new EventTarget();
    vi.stubGlobal("window", view);
    const field = new EventTarget();
    const leave = vi.fn();
    const stop = leaveOnEscape((target) => target === field, leave);

    const esc = keydown("Escape", field);
    view.dispatchEvent(esc.event);
    expect(leave).toHaveBeenCalledTimes(1);
    expect(esc.stopped).toHaveBeenCalled();
    expect(esc.event.defaultPrevented).toBe(true);

    // Composing: neither the edit nor the panel takes it.
    const ime = keydown("Escape", field, { isComposing: true });
    view.dispatchEvent(ime.event);
    expect(leave).toHaveBeenCalledTimes(1);
    expect(ime.stopped).toHaveBeenCalled();
    expect(ime.event.defaultPrevented).toBe(false);

    // Elsewhere in the panel: the panel's own Esc.
    const elsewhere = keydown("Escape", new EventTarget());
    view.dispatchEvent(elsewhere.event);
    expect(leave).toHaveBeenCalledTimes(1);
    expect(elsewhere.stopped).not.toHaveBeenCalled();
    expect(elsewhere.event.defaultPrevented).toBe(false);

    // The edit is over: Esc is the panel's again.
    stop();
    const after = keydown("Escape", field);
    view.dispatchEvent(after.event);
    expect(leave).toHaveBeenCalledTimes(1);
    expect(after.stopped).not.toHaveBeenCalled();
  });
});
