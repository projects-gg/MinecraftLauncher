// "Paylaşımı durdur" in a person's menu in the room panel (SHARE; share.md §3): a leader's or
// moderator's, on someone who shares; the room checks the role. Last in the menu, after a line.
import { Screencast } from "@phosphor-icons/react";
import { type MenuItem, toast } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { VoicePerson } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import { useShare } from "./store";

export function useUnshareItem(person: VoicePerson): Array<MenuItem | null> {
  const { t } = useTranslation();
  const share = useShare();
  const theirs = share.live.filter((l) => l.person === person.id);
  // The same powers as "Odadan çıkar": over them in this room.
  if (!share.on || !theirs.length || !person.can.kick) return [];
  return [
    null,
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
