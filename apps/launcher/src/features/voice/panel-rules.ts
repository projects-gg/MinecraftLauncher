// The room panel's header, marks and footer, in plain rules (SQOL-U3): who an unnamed room's title
// names when its line is short, a crew room's title and the line under it, where renaming and
// closing the room live, the footer's labels and a connection's words.
import type { TFunction } from "i18next";
import type { VoiceQuality, VoiceRoomView } from "@/lib/api/voice-types";
import { namesLine, roomTitle } from "./rules";

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
 * The panel's title: a crew room's is the crew's name (the room is the chip lit under it, so the
 * title never repeats it); any other room's is `roomTitle`.
 */
export function panelTitle(
  room: Pick<VoiceRoomView, "kind" | "name" | "people" | "general">,
  crewName: string | null | undefined,
  nameOf: (id: string) => string,
  t: TFunction,
  calling?: string | null,
): string {
  if (room.kind === "crew" && crewName) return crewName;
  return roomTitle(room, nameOf, t, calling);
}

/**
 * The quiet line under the title: a call says it is one; a crew's chips and a chat's people line
 * count the people, so the header then says only a full room; else how many are in the room.
 */
export function headerLine(
  room: Pick<VoiceRoomView, "kind" | "people">,
  where: { stage: boolean; counted: boolean; full: boolean },
  t: TFunction,
): string | null {
  if (where.stage) return t("voice.call.kind");
  if (room.kind === "crew" || where.counted) return where.full ? t("voice.refused.roomFull") : null;
  return t("voice.room.count", { count: room.people.length + 1 });
}

/**
 * The pencil beside the title: a friend room's name means something once three are in it (two are
 * a call, named by who is in it), as roles do; a crew's title is the crew's name, its rooms renamed
 * from the menu.
 */
export function renameBesideTitle(
  room: Pick<VoiceRoomView, "canRename" | "kind" | "people">,
): boolean {
  return room.canRename && room.kind !== "crew" && room.people.length >= 2;
}

/** "Adı Değiştir" in the header's menu: a crew's own rooms ("Genel" is never renamed). */
export function renameInMenu(room: Pick<VoiceRoomView, "canRename" | "kind" | "general">): boolean {
  return room.canRename && room.kind === "crew" && !room.general;
}

/**
 * The footer's microphone and headphones, each labelled by what a press does from the state it is
 * in: "Mikrofonu Aç" while the microphone is off, deafened or `closed` (a moderator, a missing
 * push-to-talk key), else "Mikrofonu Kapat"; "Sağırlaştırmayı Kaldır" while deafened, else
 * "Sağırlaştır".
 */
export function footerActions(
  me: Pick<VoiceRoomView["me"], "mute" | "deaf">,
  closed: boolean,
  t: TFunction,
): { mic: string; deaf: string } {
  return {
    mic: me.mute || me.deaf || closed ? t("voice.room.unmuteMic") : t("voice.room.muteMic"),
    deaf: me.deaf ? t("voice.room.deafenOff") : t("voice.room.deafenOn"),
  };
}

/** A connection's words, for its bars' tooltip and screen readers ("Bağlantı: Zayıf"). */
export function connectionWords(quality: VoiceQuality, t: TFunction): string {
  return t(`roomChat.connection.${quality}`);
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
