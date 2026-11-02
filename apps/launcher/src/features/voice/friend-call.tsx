// Voice chat on a friend's row and card (VOICE-L3; voice.md §3.2, §5.1). The card's "Ara" (the main
// way to call), "Odaya davet et" while the player's room may take them ("Aranıyor…" while it
// rings), and after a missed call "Geri ara", or "Aramaya katıl" while that room still goes (the
// card's "Katıl" is their game's). On the row: a headset beside the status while they are in voice
// ("Sesli sohbette" in the launcher; never for an invisible friend: they are offline), and after a
// missed call the status line says "Cevapsız arama" in words (a state, so no gold mark).
import { Headset, Phone, PhonePlus, PhoneX } from "@phosphor-icons/react";
import { Button, IconButton, Tooltip } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { useSocialUi } from "@/features/social/store";
import type { Friend } from "@/lib/api/social-types";
import { answerRing, callFriend, inviteToRoom } from "./actions";
import { callOffer, presenceOf } from "./rules";
import { useVoiceUi, voiceOn } from "./store";

/** Whether the friend is in a voice room now (and voice chat shows at all). */
export function useInVoice(friend: string): boolean {
  const friends = useSocialUi((s) => s.view.friends);
  return useVoiceUi((s) => voiceOn(s.state) && presenceOf(friend, s.state, friends) === "in");
}

/** The friend's missed call, if one waits. */
function useMissed(friend: string) {
  return useVoiceUi((s) =>
    voiceOn(s.state) ? (s.state.missed.find((m) => m.from === friend) ?? null) : null,
  );
}

/** What the friend's card and menu offer (`callOffer`, with the player's crews). */
export function useCallOffer(friend: string, isFriend = true) {
  const crews = useSocialUi((s) => s.view.crews);
  const friends = useSocialUi((s) => s.view.friends);
  return useVoiceUi((s) =>
    callOffer(
      s.state,
      friend,
      isFriend,
      (id) => crews.find((c) => c.id === id)?.members.map((m) => m.id) ?? [],
      friends,
    ),
  );
}

/** The headset before the status line while they are in voice (its words under the pointer). */
export function InVoiceMark({ friend }: { friend: string }) {
  const { t } = useTranslation();
  const inVoice = useInVoice(friend);
  if (!inVoice) return null;
  return (
    <Tooltip content={t("voice.presence.inVoice")}>
      <span className="mr-1 inline-flex align-[-1px]">
        <Headset size={12} weight="bold" aria-hidden className="text-ash-300" />
      </span>
    </Tooltip>
  );
}

/** A missed call from the friend waits (the row says it in its status line). */
export function useMissedCall(friend: string): boolean {
  return useMissed(friend) !== null;
}

/** The status line while a missed call waits: in words, with a call's glyph (no gold: a state). */
export function MissedLine() {
  const { t } = useTranslation();
  return (
    <>
      <PhoneX size={12} weight="bold" aria-hidden className="mr-1 inline align-[-1px]" />
      {t("voice.missed.title")}
    </>
  );
}

/** The chat header's call (a friend's conversation): the card's action as an icon. */
export function ChatCallButton({ friend }: { friend: string }) {
  const { t } = useTranslation();
  const offer = useCallOffer(friend);
  if (!offer) return null;
  const label =
    offer === "call"
      ? t("voice.call.action")
      : offer === "ringing"
        ? t("voice.call.ringing")
        : t("voice.call.inviteToRoom");
  return (
    <IconButton
      label={label}
      icon={offer === "call" ? <Phone size={18} /> : <PhonePlus size={18} />}
      className="shrink-0"
      disabled={offer === "ringing" || offer === "full"}
      onClick={() => void (offer === "call" ? callFriend(friend) : inviteToRoom(friend))}
    />
  );
}

/** The card's voice action (one button, as wide as the card's others). */
export function FriendCallButton({
  friend,
  onDone,
}: {
  friend: Pick<Friend, "id">;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const offer = useCallOffer(friend.id);
  const missed = useMissed(friend.id);
  if (missed?.joinable) {
    return (
      <Button
        variant="secondary"
        className="w-full"
        icon={<Phone size={16} weight="fill" />}
        onClick={() => {
          onDone();
          void answerRing(missed.room);
        }}
      >
        {t("voice.missed.joinCall")}
      </Button>
    );
  }
  if (!offer) return null;
  if (offer === "call") {
    return (
      <Button
        variant="secondary"
        className="w-full"
        icon={<Phone size={16} />}
        onClick={() => {
          onDone();
          void callFriend(friend.id);
        }}
      >
        {missed ? t("voice.missed.callBack") : t("voice.call.action")}
      </Button>
    );
  }
  return (
    <Button
      variant="secondary"
      className="w-full"
      icon={<PhonePlus size={16} />}
      disabled={offer !== "invite"}
      onClick={() => void inviteToRoom(friend.id)}
    >
      {offer === "ringing" ? t("voice.call.ringing") : t("voice.call.inviteToRoom")}
    </Button>
  );
}
