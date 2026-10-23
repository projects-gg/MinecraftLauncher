// Words for SKIN1's notes (rules.ts → `projectsSkin.note.*`) and sentences with an in-game
// command in them: the command is drawn as one unbreakable piece the player can select, and read
// as Turkish (it is the real command in both languages).
import type { TFunction } from "i18next";
import { dayMonth } from "@/features/social/limits";
import { LINK_COMMAND, type Note, UNLINK_COMMAND } from "./rules";

/** Where the command goes in a sentence (a private-use character no text has). */
export const COMMAND_SLOT = String.fromCharCode(0xe000);

/** A note's sentence; `taken` keeps the command's place as `COMMAND_SLOT`. */
export function noteText(t: TFunction, note: Note): string {
  const key = `projectsSkin.note.${note.key}`;
  switch (note.key) {
    case "limited":
      return t(key, { count: note.count ?? 1 });
    case "blocked":
      return t(key, { date: dayMonth(note.until ?? 0) });
    case "tries":
      return t(key, {
        time: t(`projectsSkin.time.${note.wait?.unit ?? "minutes"}`, {
          count: note.wait?.count ?? 1,
        }),
      });
    case "otherAccount":
      return t(key, { name: note.name ?? "" });
    case "taken":
      return t(key, { command: COMMAND_SLOT });
    default:
      return t(key);
  }
}

/** The command a note's sentence names, if any. */
export const noteCommand = (note: Note) => (note.key === "taken" ? UNLINK_COMMAND : null);

/** A sentence with `COMMAND_SLOT` in it, the command drawn in its place. */
export function CommandText({
  text,
  command = LINK_COMMAND,
}: {
  text: string;
  command?: string | null;
}) {
  const at = command ? text.indexOf(COMMAND_SLOT) : -1;
  if (at < 0 || !command) return <>{text}</>;
  return (
    <>
      {text.slice(0, at)}
      <span
        lang="tr"
        data-selectable
        className="rounded-[4px] bg-white/[0.08] px-[5px] py-px font-semibold whitespace-nowrap text-bone-50"
      >
        {command}
      </span>
      {text.slice(at + COMMAND_SLOT.length)}
    </>
  );
}

/** A note as it reads, with its command drawn in. */
export function NoteText({ t, note }: { t: TFunction; note: Note }) {
  return <CommandText text={noteText(t, note)} command={noteCommand(note)} />;
}
