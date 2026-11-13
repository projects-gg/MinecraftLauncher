// The room panel's header and marks, in plain rules (SQOL-U3): who an unnamed room's title names
// when its line is short, and what the header's room menu holds.
import type { TFunction } from "i18next";
import type { VoiceRoomView } from "@/lib/api/voice-types";
import { namesLine } from "./rules";

/**
 * The ways to say who is in a room, longest first: every name (two and "ve N kişi daha" past
 * three, as `namesLine`), then fewer names and how many more, then the first name and "…". Every
 * one ends at a name's end or says how many more: a line never stops inside a name by choice.
 */
export function namesLines(names: readonly string[], t: TFunction): string[] {
  if (!names.length) return [];
  const lines = [namesLine(names, t)];
  const shown = names.length <= 3 ? names.length : 2;
  for (let k = shown - 1; k >= 1; k--) {
    lines.push(
      t("voice.names.more", { names: names.slice(0, k).join(", "), count: names.length - k }),
    );
  }
  if (names.length > 1) lines.push(`${names[0]}…`);
  return lines;
}

/**
 * The longest way to say who is in a room that `fits` its line; the shortest when none does (the
 * line's own ellipsis then cuts it, only on a very narrow line).
 */
export function fitNames(
  names: readonly string[],
  fits: (line: string) => boolean,
  t: TFunction,
): string {
  const lines = namesLines(names, t);
  return lines.find(fits) ?? lines.at(-1) ?? "";
}

/**
 * The names an unnamed friend room's title is made of (`roomTitle`'s own rule), or null when the
 * title is a name of its own ("Genel", "Oda 2", the leader's words, the friend being called).
 */
export function titleNames(
  room: Pick<VoiceRoomView, "name" | "people" | "general">,
  nameOf: (id: string) => string,
): string[] | null {
  if (room.general || room.name || !room.people.length) return null;
  return room.people.map((p) => nameOf(p.id));
}

/**
 * "Odayı Kapat" in the header's menu: alone, closing is leaving ("Ayrıl" says it), and a call of
 * two has nothing to close for the other (leaving is hanging up).
 */
export function closeInMenu(
  room: Pick<VoiceRoomView, "canClose" | "people">,
  stage: boolean,
): boolean {
  return room.canClose && room.people.length > 0 && !stage;
}
