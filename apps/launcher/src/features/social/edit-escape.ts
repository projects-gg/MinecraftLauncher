// Esc in a field that edits something in the friends panel (a chat message, a plan's note, a
// category's name) leaves that edit and nothing else: the panel stays open (FIX2; the owner's Esc
// in an edit closed the whole panel). Radix's layers (the panel, its menus) hear Esc on the
// document in the capture phase, before any React handler runs; a listener on the window's capture
// phase comes first. An IME's Esc only ends its composition: the edit and the panel stay.

/** What an Esc press does to an edit: leave it, leave it to the IME, or not ours at all. */
export type EditEscape = "leave" | "composing" | "pass";

/** The parts of a key press the choice reads. */
export type EscapeKey = Pick<KeyboardEvent, "key" | "isComposing" | "keyCode">;

/** `inEdit`: the press went to the edit's field or a control of the edit beside it. */
export function editEscape(event: EscapeKey, inEdit: boolean): EditEscape {
  if (event.key !== "Escape" || !inEdit) return "pass";
  // 229: the IME took the key (WebKit sends the Esc that ends a composition after it ended).
  if (event.isComposing || event.keyCode === 229) return "composing";
  return "leave";
}

/**
 * Until the returned function is called, Esc on an element `inEdit` accepts calls `leave`, and no
 * layer around it (the panel, a menu) hears the key; an IME's Esc is the IME's alone.
 */
export function leaveOnEscape(
  inEdit: (target: EventTarget | null) => boolean,
  leave: () => void,
): () => void {
  const onKey = (event: KeyboardEvent) => {
    const action = editEscape(event, inEdit(event.target));
    if (action === "pass") return;
    event.stopPropagation();
    if (action === "composing") return;
    event.preventDefault();
    leave();
  };
  const capture = { capture: true };
  window.addEventListener("keydown", onKey, capture);
  return () => window.removeEventListener("keydown", onKey, capture);
}

/** Whether `target` is inside `area` (an edit's form: its field and the controls beside it). */
export function inside(area: Element | null | undefined, target: EventTarget | null): boolean {
  return !!area && target instanceof Node && area.contains(target);
}
