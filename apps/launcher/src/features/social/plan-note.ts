// A crew plan's short note (FIX2: "Bedwars, sesli gelin"), checked as the player types the way the
// friends service checks it (services/social: chat's cleaning and checks): one line of at most 80
// characters (code points), never a link or a phone number; projects.gg may be written. The
// service checks again, its word list too, which it never names.
import { cleanChatText, looksLikeLink, looksLikePhone } from "./chat/text";

/** Characters in a note. */
export const PLAN_NOTE_MAX = 80;
/** The counter shows from here on, near the limit. */
export const PLAN_NOTE_COUNT_FROM = 64;

export type NoteProblem = "link" | "phone" | "long";

/** The note as it goes: chat's cleaning on one line (a pasted line break reads as a space). */
export function cleanPlanNote(value: string): string {
  return cleanChatText(value.replace(/\r\n?|[\n\u0085\u2028\u2029]/gu, " "));
}

/** Characters as the service counts them. */
export function planNoteLength(value: string): number {
  return [...cleanPlanNote(value)].length;
}

/** Why the note cannot go as it is, or null when it may (an empty one is no note). */
export function planNoteProblem(value: string): NoteProblem | null {
  const note = cleanPlanNote(value);
  if (!note) return null;
  if ([...note].length > PLAN_NOTE_MAX) return "long";
  if (looksLikeLink(note)) return "link";
  if (looksLikePhone(note)) return "phone";
  return null;
}
