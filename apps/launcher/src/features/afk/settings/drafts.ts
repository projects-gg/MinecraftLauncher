// Unsaved edits of an AFK account's settings, desktop or 7/24 Bulut (SHEET-UX): a press outside the
// panel, Esc or its close button closes it and the edits wait here, as the instance settings' and
// AFK+'s do (docs/design.md §5 "Unsaved edits wait where they were made"); saving or "Vazgeç" drops
// them. The card's settings button carries a dot meanwhile. Kept in memory only: a restart starts
// clean.
import { toast } from "@pl/ui";
import { create } from "zustand";

/** The panel as edited, with the stored settings it was edited from. */
export interface HeldDraft<T> {
  draft: T;
  base: T;
}

interface AfkDraftState {
  /** `draftKey(...)` → the edits (absent: nothing unsaved). */
  held: Record<string, HeldDraft<unknown>>;
  hold: (key: string, entry: HeldDraft<unknown> | null) => void;
}

export const useAfkDrafts = create<AfkDraftState>()((set) => ({
  held: {},
  hold: (key, entry) =>
    set((state) => {
      if (entry === null && !(key in state.held)) return state;
      const held = { ...state.held };
      if (entry === null) delete held[key];
      else held[key] = entry;
      return { held };
    }),
}));

/** One account's slot: a desktop account and a cloud account may share an id. */
export function draftKey(kind: "desktop" | "cloud", id: string): string {
  return `${kind}:${id}`;
}

/**
 * A panel closed with edits in it says so once, quietly, with the way back (a closed panel is easy
 * to take for a saved one): "Değişikliklerin kaydedilmedi", "Ayarları aç".
 */
export function remindKept(t: (key: string) => string, key: string, reopen: () => void) {
  if (!(key in useAfkDrafts.getState().held)) return;
  toast(t("afkPanel.kept.title"), {
    description: t("afkPanel.kept.body"),
    action: { label: t("afkPanel.kept.open"), onClick: reopen },
  });
}

/** Whether the account's panel was closed with edits in it (the dot on its settings button). */
export function useHasDraft(kind: "desktop" | "cloud", id: string): boolean {
  return useAfkDrafts((state) => draftKey(kind, id) in state.held);
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/**
 * The held edits over the settings as they are stored now: what the player changed keeps their
 * value, everything else follows what was saved meanwhile (another computer, a copy from another
 * account). Lists (commands, rules, tasks) are edited as wholes.
 */
export function rebaseDraft<T>(draft: T, base: T, current: T): T {
  if (same(base, current)) return draft;
  return merge(draft, base, current) as T;
}

function merge(draft: unknown, base: unknown, current: unknown): unknown {
  if (isRecord(draft) && isRecord(base) && isRecord(current)) {
    const out: Record<string, unknown> = {};
    for (const key of new Set([...Object.keys(current), ...Object.keys(draft)])) {
      out[key] = merge(draft[key], base[key], current[key]);
    }
    return out;
  }
  return same(draft, base) ? current : draft;
}
