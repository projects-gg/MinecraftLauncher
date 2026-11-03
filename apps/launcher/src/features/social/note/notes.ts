// SOC-NOTE §3.5: the status note, its pure rules (notes.test.ts) and the hooks every surface asks.
// A note is one line of at most 40 characters (code points, as the friends service counts them
// after chat's cleaning), never a link or a phone number (the service checks again, its word list
// too). It lasts an hour, until the end of the player's day ("Bugün", local midnight, sent as an
// absolute time) or until taken off ("Silinmesin"). Friends see it while the player is online and
// visible; it shows only while SOC-NEXT's switch and the service's are on, and goes by itself at
// its end: one timer for the soonest end on screen (`useNoteClockDriver`), never a poll.
import { useEffect } from "react";
import { create } from "zustand";
import type { Friend, StatusNote } from "@/lib/api/social-types";
import { looksLikeLink, looksLikePhone } from "../chat/text";
import { useSocNext } from "../next/flag";
import { cleanPlanNote } from "../plan-note";
import { useSocialUi } from "../store";

/** Characters in a note (code points). */
export const NOTE_MAX = 40;
/** The counter shows from here on, near the limit. */
export const NOTE_COUNT_FROM = 30;

export const NOTE_LENGTHS = ["hour", "today", "keep"] as const;
export type NoteLength = (typeof NOTE_LENGTHS)[number];

export type NoteProblem = "link" | "phone" | "long";

/** The note as it goes: chat's cleaning on one line (a pasted line break reads as a space). */
export function cleanNote(value: string): string {
  return cleanPlanNote(value);
}

/** Characters as the service counts them. */
export function noteLength(value: string): number {
  return [...cleanNote(value)].length;
}

/** Why the note cannot go as it is, or null when it may (an empty one takes it off). */
export function noteProblem(value: string): NoteProblem | null {
  const note = cleanNote(value);
  if (!note) return null;
  if ([...note].length > NOTE_MAX) return "long";
  if (looksLikeLink(note)) return "link";
  if (looksLikePhone(note)) return "phone";
  return null;
}

/** When a note of this length ends (Unix seconds; 0: until taken off). "Bugün": local midnight. */
export function noteUntil(length: NoteLength, nowMs: number): number {
  if (length === "keep") return 0;
  if (length === "hour") return Math.floor(nowMs / 1000) + 3_600;
  const midnight = new Date(nowMs);
  midnight.setHours(24, 0, 0, 0);
  return Math.floor(midnight.getTime() / 1000);
}

/** The length the dialog opens on: as the note was set, as near as its end tells ("Bugün" new). */
export function lengthOf(note: StatusNote | undefined, nowMs: number): NoteLength {
  if (!note) return "today";
  if (note.until === 0) return "keep";
  return note.until * 1000 - nowMs <= 3_660_000 && note.until !== noteUntil("today", nowMs)
    ? "hour"
    : "today";
}

/** Whether a note shows at `nowMs`. */
export function noteShows(note: StatusNote | undefined, nowMs: number): note is StatusNote {
  return note !== undefined && (note.until === 0 || note.until * 1000 > nowMs);
}

/** The soonest end among notes that still show (Unix ms), or null when none ends. */
export function nextNoteEnd(
  notes: ReadonlyArray<StatusNote | undefined>,
  nowMs: number,
): number | null {
  let next: number | null = null;
  for (const note of notes) {
    if (!note || note.until === 0) continue;
    const end = note.until * 1000;
    if (end > nowMs && (next === null || end < next)) next = end;
  }
  return next;
}

/** Status notes show: SOC-NEXT's switch (signed config) and the service's (Studio). */
export function useNotesOn(): boolean {
  const flag = useSocNext();
  const on = useSocialUi((s) => s.view.chat?.socNote?.notes === true);
  return flag && on;
}

/** The clock notes are read against: one value for every surface, moved by SocNoteHost alone. */
const useNoteTick = create<{ now: number }>()(() => ({ now: Date.now() }));

/**
 * SocNoteHost's one timer: the clock moves at the soonest end among the notes on screen (and when
 * the window comes back, a computer that slept wakes the timeout late), never on a poll.
 */
export function useNoteClockDriver() {
  // The clock's own moves count too: after one end the next one is scheduled.
  const now = useNoteTick((s) => s.now);
  const own = useSocialUi((s) => s.view.chat?.socNote?.note);
  const friends = useSocialUi((s) => s.view.friends);
  const soonest =
    nextNoteEnd([own, ...friends.map((f: Friend) => f.note)], Math.max(now, Date.now())) ?? 0;
  useEffect(() => {
    if (!soonest) return;
    const wait = Math.max(0, soonest - Date.now()) + 250;
    const id = window.setTimeout(() => useNoteTick.setState({ now: Date.now() }), wait);
    return () => window.clearTimeout(id);
  }, [soonest]);
  useEffect(() => {
    const again = () => useNoteTick.setState({ now: Date.now() });
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, []);
}

/** The time notes are read against (a render follows the clock's moves). */
export function useNoteNow(): number {
  const now = useNoteTick((s) => s.now);
  return Math.max(now, Date.now());
}

/** A friend's note while it shows (and notes are on), else undefined. */
export function useShownNote(
  friend: Pick<Friend, "note" | "state"> | undefined,
): StatusNote | undefined {
  const on = useNotesOn();
  const now = useNoteNow();
  if (!on || !friend || friend.state === "offline") return undefined;
  return noteShows(friend.note, now) ? friend.note : undefined;
}

/** The player's own note while it shows (and notes are on), else undefined. */
export function useOwnNote(): StatusNote | undefined {
  const on = useNotesOn();
  const now = useNoteNow();
  const note = useSocialUi((s) => s.view.chat?.socNote?.note);
  return on && noteShows(note, now) ? note : undefined;
}
