// A person's share items in their menu in the room panel (SHARE; share.md §3). First, over a line:
// "Yayını izle" on someone live the player does not watch yet (the menu's way to what "İzle"
// beside the row does, SHARE-LIVE). Last, after a line: "Yayını rapor et" on someone who shares
// (the one the player watches, else their first), and a leader's or moderator's "Paylaşımı
// durdur" (the room checks the role).
import { Eye, Flag, Screencast } from "@phosphor-icons/react";
import { type MenuItem, toast } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { VoicePerson } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import { reportShare, watchShare } from "./actions";
import { unwatched } from "./panel";
import { useShare } from "./store";

/** "Yayını izle" and the line under it, at the top of the menu (nothing to watch: nothing). */
export function useWatchItem(person: VoicePerson): Array<MenuItem | null> {
  const { t } = useTranslation();
  const share = useShare();
  const next = share.on
    ? unwatched(
        share.live.filter((l) => l.person === person.id),
        share.watching,
      )
    : null;
  if (!next) return [];
  return [
    {
      label: t("share.row.watchItem"),
      icon: <Eye size={16} />,
      onSelect: () => void watchShare(next.person, next.n),
    },
    null,
  ];
}

export function useUnshareItem(person: VoicePerson): Array<MenuItem | null> {
  const { t } = useTranslation();
  const share = useShare();
  const theirs = share.live.filter((l) => l.person === person.id);
  const first = theirs[0];
  if (!share.on || !first) return [];
  const watched = share.watching.find((w) => w.person === person.id)?.n ?? first.n;
  const report: MenuItem = {
    label: t("share.report.action"),
    icon: <Flag size={16} />,
    onSelect: () => void reportShare(person.id, watched),
  };
  const items: Array<MenuItem | null> = [null, report];
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
