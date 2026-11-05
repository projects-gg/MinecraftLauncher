// "Paylaşımı durdur" in a person's menu in the room panel (SHARE; share.md §3): a leader's or
// moderator's, on someone who shares; the room checks the role. Last in the menu, after a line.
// "Yayını rapor et" on someone who shares: the one the player watches, else their first.
import { Flag, Screencast } from "@phosphor-icons/react";
import { type MenuItem, toast } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { VoicePerson } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import { reportShare } from "./actions";
import { useShare } from "./store";

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
  // The same powers as "Odadan çıkar": over them in this room.
  if (!person.can.kick) return [null, report];
  return [
    null,
    report,
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
