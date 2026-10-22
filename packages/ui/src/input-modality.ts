/**
 * Whether the player's latest input was a pointer press or a key (POLISH6).
 *
 * Radix focuses a menu by script when it opens, and Chromium then treats every focus moved by
 * script as keyboard focus: after a plain click on a "…" button, the button lit up with its gold
 * ring and popped its tooltip when the menu closed, and the tabs of a sheet opened from the menu
 * kept a ring after every click. Focus moved by script after pointer use therefore goes without
 * the ring (`focusPanel`), and a menu closed after pointer use leaves the focus where it is
 * (`returnFocusAfterKeys`). After keys everything stays as it was: keyboard players see where
 * they are.
 */
let pointerLast = false;

if (typeof window !== "undefined") {
  window.addEventListener(
    "pointerdown",
    () => {
      pointerLast = true;
    },
    true,
  );
  window.addEventListener(
    "keydown",
    () => {
      pointerLast = false;
    },
    true,
  );
}

/** Whether the latest input was a pointer press (mouse, pen, touch) rather than a key. */
export function lastInputWasPointer(): boolean {
  return pointerLast;
}

/**
 * Focus for a layer's panel (or the element it starts on) when it opens: without the focus ring
 * after pointer use, so the next clicks inside do not light up rings either.
 */
export function focusPanel(element: HTMLElement): void {
  element.focus(
    pointerLast
      ? ({ preventScroll: true, focusVisible: false } as FocusOptions)
      : { preventScroll: true },
  );
}

/**
 * A menu's `onCloseAutoFocus`: the focus goes back to the trigger only after keyboard use (a
 * mouse player would get the trigger's ring and tooltip after a plain click), and never when the
 * chosen item moved it on by itself: a field it focused (a chat message's "Düzenle", FIX2) or a
 * layer it opened keeps it.
 */
export function returnFocusAfterKeys(event: Event): void {
  const active = typeof document === "undefined" ? null : document.activeElement;
  if (pointerLast || focusMovedOn(active, event.currentTarget ?? event.target)) {
    event.preventDefault();
  }
}

/**
 * Whether the focus moved on while a menu closed: something outside the menu has it, not the
 * page itself (the focus falls to the page when the menu's focused row goes away).
 */
export function focusMovedOn(active: Element | null, menu: unknown): boolean {
  if (!active) return false;
  const page = active.ownerDocument;
  if (page && (active === page.body || active === page.documentElement)) return false;
  const inMenu =
    menu !== null &&
    typeof menu === "object" &&
    "contains" in menu &&
    typeof menu.contains === "function" &&
    Boolean(menu.contains(active));
  return !inMenu;
}

/**
 * `onMouseDown` of a Radix tab: Radix focuses the tab by script on the press, which Chromium
 * draws with the keyboard's focus ring, so every click on a tab left a gold box around it. The tab
 * takes the focus first, quietly; Radix then finds it focused. Arrow keys still show the ring.
 */
export function focusTabQuietly(event: {
  button: number;
  ctrlKey: boolean;
  currentTarget: HTMLElement;
}): void {
  if (event.button === 0 && !event.ctrlKey) {
    event.currentTarget.focus({ preventScroll: true, focusVisible: false } as FocusOptions);
  }
}
