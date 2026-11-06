// A person's share items in their menu in the room panel (SHARE; share.md §3), after a line:
// "Yayını izle" on someone live the player does not watch yet (the menu's way to what "İzle"
// beside the row does, SHARE-LIVE); "Yayını rapor et" on someone who shares (the one the player
// watches, else their first); last, a leader's or moderator's "Paylaşımı durdur" (the room checks
// the role).
import { Eye, Flag, Screencast } from "@phosphor-icons/react";
import { type MenuItem, toast } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { VoicePerson } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import { reportShare, watchShare } from "./actions";
import { unwatched } from "./panel";
import { useShare } from "./store";

export function useUnshareItem(person: VoicePerson): Array<MenuItem | null> {
  const { t } = useTranslation();
  const share = useShare();
  const theirs = share.live.filter((l) => l.person === person.id);
  const first = theirs[0];
  if (!share.on || !first) return [];
  const watched = share.watching.find((w) => w.person === person.id)?.n ?? first.n;
  const next = unwatched(theirs, share.watching);
  const watch: MenuItem | null = next
    ? {
        label: t("share.row.watchItem"),
        icon: <Eye size={16} />,
        onSelect: () => void watchShare(next.person, next.n),
      }
    : null;
  const report: MenuItem = {
    label: t("share.report.action"),
    icon: <Flag size={16} />,
    onSelect: () => void reportShare(person.id, watched),
  };
  const items: Array<MenuItem | null> = [null, ...(watch ? [watch] : []), report];
  // The same powers as "Odadan çıkar": over them in this room.
  if (!person.can.kick) return items;
  return [
    ...items,
    {
      label: t("share.row.unshare"),
      icon: <Screencast size={16} />,
      onSelect: () => {
        for (const live of theirs) {
          void shareApi
            .unshare(person.id, live.n)
            .then((done) => {
              if (!done) toast(t("share.notice.failed"));
            })
            .catch(() => toast(t("share.notice.failed")));
        }
      },
    },
  ];
}
