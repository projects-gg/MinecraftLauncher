// Who wrote in the room chat: as the lists know them now (the player, friends, crewmates), else as
// the room names them (a friend's friend's card), else as they were when they wrote (someone who
// has left keeps their name and face), else "Biri".
import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import type { ChatPerson } from "@/features/social/chat/sender-name";
import { useSocialUi } from "@/features/social/store";
import type { SocialSnapshot } from "@/lib/api/social-types";
import type { VoiceProfile } from "@/lib/api/voice-types";
import { useVoiceUi } from "../store";
import { useRoomChat } from "./store";

function fromLists(
  id: string,
  view: Pick<SocialSnapshot, "me" | "friends" | "crews">,
  profiles: readonly VoiceProfile[] | undefined,
): ChatPerson | undefined {
  if (view.me && view.me.id === id) return view.me;
  const friend = view.friends.find((f) => f.id === id);
  if (friend) return friend;
  for (const crew of view.crews) {
    const member = crew.members.find((m) => m.id === id);
    if (member) return member;
  }
  const profile = profiles?.find((p) => p.id === id);
  return profile
    ? {
        name: profile.name,
        avatar: profile.avatar,
        ...(profile.avatarUrl ? { avatarUrl: profile.avatarUrl } : {}),
        ...(profile.marks ? { marks: profile.marks } : {}),
      }
    : undefined;
}

/** The sender as known now, read outside React (a message that just came keeps it). */
export function personNow(id: string): ChatPerson | undefined {
  return fromLists(id, useSocialUi.getState().view, useVoiceUi.getState().state.profiles);
}

/** Who wrote what, for the room chat's rows. */
export function useRoomPeople(): (id: string) => ChatPerson {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me);
  const friends = useSocialUi((s) => s.view.friends);
  const crews = useSocialUi((s) => s.view.crews);
  const profiles = useVoiceUi((s) => s.state.profiles);
  const kept = useRoomChat((s) => s.people);
  return useCallback(
    (id: string) =>
      fromLists(id, { me, friends, crews }, profiles) ??
      kept[id] ?? { name: t("chat.list.someone"), avatar: "discord" },
    [me, friends, crews, profiles, kept, t],
  );
}
