// Voice chat on a friend's row and card (VOICE-L3; voice.md §3.2, §5.1). The card's "Sesli arama
// başlat" (VOICE-CALL: in the place of "Discord'dan mesaj at", the owner's words; Discord stays in
// the person menu), "Odaya davet et" while the player's room may take them ("Aranıyor…" while it
// rings), and after a missed call "Geri ara", or "Aramaya katıl" while that room still goes (the
// card's "Katıl" is their game's). While voice chat is on the card always has its voice button: a
// friend who cannot be called now gets it faded with one plain reason under the pointer (never
// which setting of theirs: offline, invisible and voice chat off read the same). A call asks first
// (call-confirm.tsx). On the row: a headset beside the status while they are in voice ("Sesli
// sohbette" in the launcher; never for an invisible friend: they are offline), and after a missed
// call the status line says "Cevapsız arama" in words (a state, so no gold mark).
import { Headset, Phone, PhonePlus, PhoneX } from "@phosphor-icons/react";
import { Button, cn, IconButton, Tooltip } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { useSocialUi } from "@/features/social/store";
import type { Friend } from "@/lib/api/social-types";
import { answerRing, inviteToRoom } from "./actions";
import { askToCall } from "./call-confirm";
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
      (id) => crews.some((c) => c.members.some((m) => m.id === id && m.state !== "offline")),
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

/**
 * A missed call beside the name, when the status line holds their game ("Projects'te" with its
 * "Katıl", which must not read as answering the call): a quiet call glyph, its words on hover.
 */
export function MissedMark() {
  const { t } = useTranslation();
  return (
    <Tooltip content={t("voice.missed.title")}>
      <span className="flex shrink-0">
        <PhoneX
          size={12}
          weight="bold"
          aria-label={t("voice.missed.title")}
          className="text-bone-100"
        />
      </span>
    </Tooltip>
  );
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
      onClick={() => (offer === "call" ? askToCall(friend) : void inviteToRoom(friend))}
    />
  );
}

/**
 * Why a friend cannot be called now, in one plain sentence: they share the player's room already,
 * or anything else (offline, invisible, their voice chat off: none of it is told).
 */
function useWaitReason(friend: string): string {
  const { t } = useTranslation();
  const together = useVoiceUi((s) => Boolean(s.state.room?.people.some((p) => p.id === friend)));
  return together ? t("voice.call.together") : t("voice.call.unreachable");
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
  const on = useVoiceUi((s) => voiceOn(s.state));
  const offer = useCallOffer(friend.id);
  const missed = useMissed(friend.id);
  const reason = useWaitReason(friend.id);
  if (!on) return null;
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
  if (!offer) {
    // Here, faded, with its reason (focusable: the keyboard reads the reason too).
    return (
      <Tooltip content={reason}>
        <Button
          variant="secondary"
          className="w-full cursor-default opacity-45"
          icon={<Phone size={16} />}
          aria-disabled
        >
          {t("voice.call.action")}
        </Button>
      </Tooltip>
    );
  }
  if (offer === "call") {
    return (
      <Button
        variant="secondary"
        className="w-full"
        icon={<Phone size={16} />}
        onClick={() => {
          onDone();
          askToCall(friend.id);
        }}
      >
        {missed ? t("voice.missed.callBack") : t("voice.call.action")}
      </Button>
    );
  }
  const full = offer === "full";
  return (
    <Tooltip content={t("voice.refused.roomFull")} disabled={!full}>
      <span className={cn("flex w-full", full && "cursor-default")}>
        <Button
          variant="secondary"
          className="w-full"
          icon={<PhonePlus size={16} />}
          disabled={offer !== "invite"}
          onClick={() => void inviteToRoom(friend.id)}
        >
          {offer === "ringing" ? t("voice.call.ringing") : t("voice.call.inviteToRoom")}
        </Button>
      </span>
    </Tooltip>
  );
}
