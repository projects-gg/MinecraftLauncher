// SOC-NOTE §3.5: where a friend's status note shows: on their card under where they are (here), and
// in place of the line under their name while they are not playing (friend-row.tsx,
// team-member.tsx); the player's own takes the status picker's words (status-menu.tsx). The
// friend's words in bone, a quiet quote mark before them on the card; a timed note's end in its
// tooltip ("17:30'a kadar"). Nothing animates but the line swapping in place (the rows' swap).
import { Quotes } from "@phosphor-icons/react";
import { Tooltip } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { StatusNote } from "@/lib/api/social-types";
import { untilText } from "../crew-notify";
import { useNoteNow } from "./notes";

/** "17:30'a kadar" for a timed note, nothing for one that stays. */
export function useNoteEnd(note: StatusNote | undefined): string | null {
  const { t } = useTranslation();
  const now = useNoteNow();
  if (!note || note.until === 0) return null;
  return untilText(t, note.until * 1000, now);
}

/** A friend's note on their card, under where they are. */
export function CardNote({ note }: { note: StatusNote | undefined }) {
  const { t } = useTranslation();
  const end = useNoteEnd(note);
  if (!note) return null;
  return (
    <Tooltip content={end ?? ""} disabled={!end}>
      <p className="mt-1.5 flex min-w-0 items-start gap-1.5 text-[13px] leading-snug text-bone-100">
        <Quotes size={12} weight="fill" className="mt-[3px] shrink-0 text-ash-400" aria-hidden />
        <span className="min-w-0 break-words" data-selectable>
          <span className="sr-only">{t("socNote.note.title")}: </span>
          {note.text}
          {end ? <span className="sr-only">, {end}</span> : null}
        </span>
      </p>
    </Tooltip>
  );
}
