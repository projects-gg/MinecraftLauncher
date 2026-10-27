// @vitest-environment happy-dom
// SHEET-UX: a press outside closes a layer like Esc, never on a toast, and the press that closes a
// layer which leaves the page live (a popover, a row's menu) does nothing else.
import { keepOpenOnToast, type OutsideEvent, onToast, swallowDismissClick } from "@pl/ui";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(() => {
  document.body.innerHTML = "";
  vi.useRealTimers();
});

/** An outside event as Radix hands it to a layer: the target, and the press (or focus) behind it. */
function outside(
  target: Element,
  original: { type: string; pointerType?: string } = { type: "pointerdown", pointerType: "mouse" },
): OutsideEvent & { prevented: boolean } {
  const event = {
    target,
    prevented: false,
    get defaultPrevented() {
      return this.prevented;
    },
    preventDefault() {
      this.prevented = true;
    },
    detail: { originalEvent: original as unknown as Event },
  };
  return event;
}

/** A button on the page under the pointer, counting its clicks. */
function pageButton() {
  const button = document.createElement("button");
  document.body.append(button);
  const clicks = vi.fn();
  button.addEventListener("click", clicks);
  return { button, clicks };
}

function toastButton() {
  const toaster = document.createElement("section");
  toaster.setAttribute("data-sonner-toaster", "");
  const button = document.createElement("button");
  toaster.append(button);
  document.body.append(toaster);
  return button;
}

describe("onToast and keepOpenOnToast", () => {
  it("knows a toast's button and nothing else", () => {
    expect(onToast(toastButton())).toBe(true);
    expect(onToast(pageButton().button)).toBe(false);
    expect(onToast(null)).toBe(false);
  });

  it("keeps the layer open only for a press on a toast", () => {
    const onPage = outside(pageButton().button);
    keepOpenOnToast(onPage);
    expect(onPage.prevented).toBe(false);
    const onToastButton = outside(toastButton());
    keepOpenOnToast(onToastButton);
    expect(onToastButton.prevented).toBe(true);
  });
});

describe("swallowDismissClick", () => {
  it("eats the click that ends the closing press, once", () => {
    const { button, clicks } = pageButton();
    const event = outside(button);
    swallowDismissClick(event);
    expect(event.prevented).toBe(false);
    button.click();
    expect(clicks).not.toHaveBeenCalled();
    // The next click is the player's own.
    button.click();
    expect(clicks).toHaveBeenCalledTimes(1);
  });

  it("lets the click of a new press through", () => {
    const { button, clicks } = pageButton();
    swallowDismissClick(outside(button));
    window.dispatchEvent(new Event("pointerdown"));
    button.click();
    expect(clicks).toHaveBeenCalledTimes(1);
  });

  it("gives up after a second without a click", () => {
    vi.useFakeTimers();
    const { button, clicks } = pageButton();
    swallowDismissClick(outside(button));
    vi.advanceTimersByTime(1000);
    button.click();
    expect(clicks).toHaveBeenCalledTimes(1);
  });

  it("keeps the layer open on a toast and eats nothing", () => {
    const toast = toastButton();
    const clicks = vi.fn();
    toast.addEventListener("click", clicks);
    const event = outside(toast);
    swallowDismissClick(event);
    expect(event.prevented).toBe(true);
    toast.click();
    expect(clicks).toHaveBeenCalledTimes(1);
  });

  it("leaves an open layer's trigger alone: its own click closes the layer", () => {
    const { button, clicks } = pageButton();
    button.setAttribute("aria-expanded", "true");
    swallowDismissClick(outside(button));
    button.click();
    expect(clicks).toHaveBeenCalledTimes(1);
  });

  it("eats nothing after a touch or a focus moving out", () => {
    const { button, clicks } = pageButton();
    swallowDismissClick(outside(button, { type: "pointerdown", pointerType: "touch" }));
    button.click();
    swallowDismissClick(outside(button, { type: "focusin" }));
    button.click();
    expect(clicks).toHaveBeenCalledTimes(2);
  });
});
