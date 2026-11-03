// SOC-NEXT's rows in a person's menu (menus.ts → personMenu, person-menu.tsx words the rest): a
// friend's nickname ("Takma ad ver", "Takma adı değiştir", "Takma adı kaldır") and ignoring a
// friend or crewmate ("Yok say", "Yok saymayı bırak"). Ignoring also turns their voice off for the
// player ("Sustur (yalnız sen)", remembered with the voice settings); stopping turns it back on
// only when ignoring had turned it off.
import { Eye, EyeSlash, PencilSimple, PencilSimpleSlash } from "@phosphor-icons/react";
import { type MenuItem, toast } from "@pl/ui";
import type { TFunction } from "i18next";
import { announce } from "@/features/accounts/use-accounts";
import { useVoiceUi } from "@/features/voice/store";
import { voiceApi } from "@/lib/api/voice-api";
import type { PersonAction } from "../menus";
import { socNextOn } from "./flag";
import { askNickname, removeNickname } from "./nickname-dialog";
import { useSocNextPrefs } from "./prefs";

/** What a person's menu offers of SOC-NEXT (undefined: nothing). */
export function socNextPersonInput(
  id: string,
  friend: boolean,
  crewmate: boolean,
): { nickname?: "none" | "set"; ignore?: boolean } {
  if (!socNextOn()) return {};
  const prefs = useSocNextPrefs.getState();
  return {
    nickname: friend ? (prefs.nicknames[id] ? "set" : "none") : undefined,
    ignore: friend || crewmate ? prefs.ignored.includes(id) : undefined,
  };
}

/** Ignores someone, or stops: shown at once, they are never told. */
export function setIgnoring(person: { id: string; name: string }, on: boolean, t: TFunction) {
  const prefs = useSocNextPrefs.getState();
  prefs.setIgnored(person.id, on);
  const inRoom = useVoiceUi.getState().state.room?.people.find((p) => p.id === person.id);
  if (on && !inRoom?.mutedForMe) {
    prefs.setIgnoreMuted(person.id, true);
    void voiceApi.setMutedForMe(person.id, true).catch(() => {});
  } else if (!on && prefs.ignoreMuted.includes(person.id)) {
    prefs.setIgnoreMuted(person.id, false);
    void voiceApi.setMutedForMe(person.id, false).catch(() => {});
  }
  const words = on
    ? t("socNext.ignore.done", { name: person.name })
    : t("socNext.ignore.stopped", { name: person.name });
  announce(words);
  toast(words);
}

type SocNextAction = Extract<PersonAction, "nickname" | "removeNickname" | "ignore" | "unignore">;

/** One of SOC-NEXT's rows, worded, with its icon and what it does. */
export function socNextItem(
  action: SocNextAction,
  person: { id: string; name: string; realName?: string; avatar?: string; avatarUrl?: string },
  t: TFunction,
): MenuItem {
  switch (action) {
    case "nickname": {
      const has = Boolean(useSocNextPrefs.getState().nicknames[person.id]);
      return {
        label: has ? t("socNext.nick.change") : t("socNext.nick.set"),
        icon: <PencilSimple size={16} />,
        onSelect: () =>
          askNickname({
            id: person.id,
            realName: person.realName ?? person.name,
            avatar: person.avatar,
            avatarUrl: person.avatarUrl,
          }),
      };
    }
    case "removeNickname":
      return {
        label: t("socNext.nick.remove"),
        icon: <PencilSimpleSlash size={16} />,
        onSelect: () => removeNickname(person.id, t),
      };
    case "ignore":
      return {
        label: t("socNext.ignore.action"),
        icon: <EyeSlash size={16} />,
        onSelect: () => setIgnoring(person, true, t),
      };
    case "unignore":
      return {
        label: t("socNext.ignore.stop"),
        icon: <Eye size={16} />,
        onSelect: () => setIgnoring(person, false, t),
      };
  }
}
