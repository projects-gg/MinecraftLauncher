// A control that takes the place of the one just pressed ("İptal" after "Bağla" or OYNA, a job's
// ×, OYNA again after "İptal") must not take the second press of a double click (CLAUDE.md,
// docs/design.md §6). It takes presses once the pointer has left it, or `ARM_MS` after it took
// its place even while the pointer rests on it: a player who does not move the mouse must still
// be able to press it (E2E2: "İptal" stayed dead under a resting pointer). A click the platform
// counts as the second of a double click (`detail` 2 or more) never counts when no press of its
// own landed on the control first, however long the player's double-click time is.
// Tests: apps/launcher/src/shell/press-guard.test.ts.
import { useEffect, useRef, useState } from "react";

/**
 * How long a control that took another's place ignores presses while the pointer rests on it:
 * longer than a double click (Windows allows 500 ms by default) and short enough that a player
 * who means to press it hardly waits.
 */
export const ARM_MS = 800;

export interface PressGuard {
  /** What the control does now; a new role is a new control in the old one's place. */
  role: string;
  /** When it took this role (`performance.now()`); `null`: it was there from the start. */
  since: number | null;
  /** Takes presses: from the start, `ARM_MS` after it took its role, or once the pointer left. */
  armed: boolean;
  /** A press counted in this role, so a double click on it now is the player's own. */
  counted: boolean;
}

/** A control as it mounts; `since`: when it took the place of another (none: it was there). */
export function guardStart(role: string, since: number | null, now: number): PressGuard {
  return { role, since, armed: since === null || now - since >= ARM_MS, counted: false };
}

/** The control's role changed: it ignores presses again for a moment. */
export function guardSwap(guard: PressGuard, role: string, now: number): PressGuard {
  return guard.role === role ? guard : { role, since: now, armed: false, counted: false };
}

/** The pointer left the control: whatever it presses next is a press of its own. */
export function guardLeave(guard: PressGuard): PressGuard {
  return guard.armed ? guard : { ...guard, armed: true };
}

/** `ARM_MS` after the control took its role it takes presses, wherever the pointer is. */
export function guardElapsed(guard: PressGuard, now: number): PressGuard {
  if (guard.armed || guard.since === null || now - guard.since < ARM_MS) return guard;
  return { ...guard, armed: true };
}

/**
 * Whether a press counts now, and the guard after it. `detail` is the platform's click count:
 * 0 from the keyboard, 1 for a click of its own, 2 or more for the later presses of a double
 * (or triple) click.
 */
export function guardPress(
  guard: PressGuard,
  now: number,
  detail: number,
): { counts: boolean; guard: PressGuard } {
  const current = guardElapsed(guard, now);
  // The rest of a double click whose first press landed before this control was here.
  const continues = detail >= 2 && current.since !== null && !current.counted;
  const counts = current.armed && !continues;
  return { counts, guard: counts && !current.counted ? { ...current, counted: true } : current };
}

/**
 * `usePressGuard(role)` for a control whose role changes in place (OYNA and "İptal", an AFK
 * account's "Bağla", "İptal", "Durdur"); `since` for one that mounts in another's place (a job's
 * bar with its ×: when the job started). `take(event)` first in the click handler; spread
 * `handlers` on the control. One timer per role change, none at rest (OPT1).
 */
export function usePressGuard(role: string, options: { since?: number } = {}) {
  const [guard, setGuard] = useState(() =>
    guardStart(role, options.since ?? null, performance.now()),
  );
  // A new role is a new control in the old one's place: set while rendering, so its first frame
  // already ignores presses.
  let current = guard;
  if (guard.role !== role) {
    current = guardSwap(guard, role, performance.now());
    setGuard(current);
  }
  const latest = useRef(current);
  latest.current = current;
  const { armed, since } = current;
  useEffect(() => {
    if (armed || since === null) return;
    const id = window.setTimeout(
      () => setGuard((g) => guardElapsed(g, performance.now())),
      Math.max(0, since + ARM_MS - performance.now()) + 1,
    );
    return () => window.clearTimeout(id);
  }, [armed, since]);
  return {
    /** Presses count now (`aria-disabled` while they do not). */
    armed,
    /** When the control took its current role (`null`: it was there from the start). */
    since,
    /** Call first in the click handler: `false` when this press must not count. */
    take(event: { detail: number }): boolean {
      const result = guardPress(latest.current, performance.now(), event.detail);
      if (result.guard !== latest.current) {
        latest.current = result.guard;
        setGuard(result.guard);
      }
      return result.counts;
    },
    handlers: {
      onPointerLeave(event: { pointerType: string }) {
        // A finger leaves with every tap it lifts: only a mouse or a pen moving away counts.
        if (event.pointerType !== "touch") setGuard((g) => guardLeave(g));
      },
      onKeyDown(event: { repeat: boolean; key: string; preventDefault: () => void }) {
        // A held Enter repeats its press: it is one press, like a held mouse button.
        if (event.repeat && (event.key === "Enter" || event.key === " ")) event.preventDefault();
      },
    },
  };
}
