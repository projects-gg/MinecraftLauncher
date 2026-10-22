// The composer's field takes the focus for an edit and keeps it (FIX2). An edit starts from a
// message's "…" menu, its right-click menu or ↑; the field takes the focus in the next frame with
// the caret at the end of its text. A menu then closes: Radix hands the focus back to the menu's
// trigger once its exit animation has ended (~120 ms later), when the player chose with keys, and
// that took the field's focus away again (the owner's "Düzenle'ye girince yazı kutusu seçilmiyor").
// The menus now leave a focus that moved on alone (`returnFocusAfterKeys`, @pl/ui); this hold is the
// second guard: for a moment the field takes the focus back from anything that takes it without
// the player, and lets go at the player's first press or key.

/** How long a closing menu or layer may still move the focus (ms). */
export const HOLD_MS = 800;

export interface Hold {
  /** The player pressed or typed since the hold began: their focus is theirs. */
  acted: boolean;
  startedAt: number;
}

/** Whether a focus that went elsewhere goes back to the field. */
export function takesBack(hold: Hold, nowMs: number, toField: boolean): boolean {
  return !hold.acted && !toField && nowMs - hold.startedAt < HOLD_MS;
}

/** The field focused, the caret after its last character. */
export function placeCaret(element: HTMLTextAreaElement | HTMLInputElement): void {
  element.focus({ preventScroll: true });
  const end = element.value.length;
  element.setSelectionRange(end, end);
}

/**
 * Focuses `element` in the next frame (once the text it is given is in it), the caret at the
 * end, and holds the focus there for `HOLD_MS`. Returns the function that stops the hold (and the
 * focus, when its frame has not come yet).
 */
export function holdFocus(element: HTMLTextAreaElement | HTMLInputElement): () => void {
  const hold: Hold = { acted: false, startedAt: performance.now() };
  let placed = false;
  let timer: number | undefined;
  let frame = window.requestAnimationFrame(() => {
    placed = true;
    placeCaret(element);
  });
  const acted = () => {
    hold.acted = true;
  };
  const moved = (event: FocusEvent) => {
    if (!placed || !takesBack(hold, performance.now(), event.target === element)) return;
    window.cancelAnimationFrame(frame);
    frame = window.requestAnimationFrame(() => {
      if (!hold.acted && element.isConnected && document.activeElement !== element) {
        placeCaret(element);
      }
    });
  };
  const stop = () => {
    window.clearTimeout(timer);
    window.removeEventListener("pointerdown", acted, true);
    window.removeEventListener("keydown", acted, true);
    document.removeEventListener("focusin", moved, true);
  };
  window.addEventListener("pointerdown", acted, true);
  window.addEventListener("keydown", acted, true);
  document.addEventListener("focusin", moved, true);
  timer = window.setTimeout(stop, HOLD_MS);
  return () => {
    if (!placed) window.cancelAnimationFrame(frame);
    stop();
  };
}
