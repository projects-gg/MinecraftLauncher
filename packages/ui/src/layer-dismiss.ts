// What a press outside a layer does (docs/design.md §8, SHEET-UX): every layer (sheet, dialog,
// menu, popover, select) closes as Esc closes it, nested layers one at a time, and the press that
// closes a layer does nothing else. Modal layers get the last part from their scrim and the
// launcher's `#layer-shield`; layers that leave the page live (popovers, menus inside rows) take
// `swallowDismissClick`. A toast is never "outside": its buttons answer the layer's own work.

/** What Radix hands a layer's `onPointerDownOutside` / `onFocusOutside` / `onInteractOutside`. */
export interface OutsideEvent {
  target: EventTarget | null;
  defaultPrevented?: boolean;
  preventDefault(): void;
  detail?: { originalEvent?: Event };
}

/** The press or focus landed on a toast ("Geri al", "Ayrıntıları kopyala"). */
export function onToast(target: EventTarget | null): boolean {
  const element = target as Element | null;
  return Boolean(element?.closest?.("[data-sonner-toaster]"));
}

/** Keeps the layer open when the press or the focus went to a toast; anything else closes it. */
export function keepOpenOnToast(event: OutsideEvent) {
  if (onToast(event.target)) event.preventDefault();
}

/**
 * A sheet's outside handlers (Radix `Dialog.Content`): a press or a focus outside closes it like
 * Esc, except on a toast. Spread onto the content: `<Dialog.Content {...sheetOutside} …>`.
 */
export const sheetOutside = {
  onPointerDownOutside: keepOpenOnToast,
  onFocusOutside: keepOpenOnToast,
} as const;

/** How long an armed swallow waits for its click (a slow click still lands within it). */
const SWALLOW_MS = 1000;

/** The press went down on the trigger of a layer that is open (Radix closes it on that click). */
function onOpenTrigger(target: EventTarget | null): boolean {
  const element = target as Element | null;
  return Boolean(element?.closest?.('[aria-expanded="true"]'));
}

/**
 * For a layer that leaves the page live (`onPointerDownOutside` of a popover, or of a menu that is
 * not modal): the press outside that closes it does nothing else. The click that ends that press
 * is swallowed once, before anything under the pointer hears it; the next press, or a second
 * without a click, disarms it. Not for a press on a toast (the layer stays), on the trigger of an
 * open layer (its own click closes it), or for touch (Radix reports a touch outside only on its
 * click, which has already happened).
 */
export function swallowDismissClick(event: OutsideEvent) {
  if (onToast(event.target)) {
    event.preventDefault();
    return;
  }
  if (event.defaultPrevented || onOpenTrigger(event.target)) return;
  const original = event.detail?.originalEvent as PointerEvent | undefined;
  // Only a press arms it (a focus moving out has no click to come), and never a touch.
  if (original && original.type !== "pointerdown") return;
  if (original?.pointerType === "touch") return;
  const view = (event.target as Node | null)?.ownerDocument?.defaultView ?? globalThis.window;
  if (!view) return;
  const swallow = (click: Event) => {
    click.preventDefault();
    click.stopImmediatePropagation();
    disarm();
  };
  const disarm = () => {
    view.removeEventListener("click", swallow, true);
    view.removeEventListener("pointerdown", disarm, true);
    view.clearTimeout(timer);
  };
  // Capture on the window: the click is stopped before any element (or React's root) hears it.
  // The press under way is past its capture phase, so only the next one disarms.
  view.addEventListener("click", swallow, true);
  view.addEventListener("pointerdown", disarm, true);
  const timer = view.setTimeout(disarm, SWALLOW_MS);
}
