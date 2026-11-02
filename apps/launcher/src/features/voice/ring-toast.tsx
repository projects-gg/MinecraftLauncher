// A call or invite ringing for the player (VOICE-L3; voice.md §5.1), in the invite toast's place.
// The caller's name has a line of its own at the card's full width: never cut (a 16-character tag
// fits; a longer display name wraps to a second line; VOICE-INT saw "GeceKu…"), the verb and the
// others in the room as small faces under it, then "Reddet" and "Katıl" as labelled buttons on the
// line below (secondary; the gold glyph on "Katıl": a toast rests right above OYNA, design.md §6).
// It stays while the ring rings and leaves when it ends (taken on another device, declined, the
// room gone); its motion is the toasts' own.
import { Phone, PhoneX } from "@phosphor-icons/react";
import { Button, toast } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { PersonMark } from "@/features/social/person-mark";
import { SocialAvatar, SocialFace } from "@/features/social/social-face";
import type { VoiceRing } from "@/lib/api/voice-types";
import { answerRing, declineRing } from "./actions";
import { namesLine } from "./rules";
import { useFace, useFaces } from "./store";

const ringToastId = (room: string) => `voice-ring:${room}`;

export function RingToast({ toastId, ring }: { toastId: string | number; ring: VoiceRing }) {
  const { t } = useTranslation();
  const from = useFace(ring.from);
  const others = useFaces(ring.with);
  return (
    <div className="flex w-full flex-col gap-2.5 font-normal">
      <div className="flex min-w-0 items-start gap-3">
        <SocialAvatar source={from} size={40} />
        <div className="min-w-0 flex-1">
          {/* What a screen reader hears: who calls, and who else is in the room. */}
          <span className="sr-only">
            {t("voice.ring.calling", { name: from.name })}
            {others.length
              ? `. ${t("voice.ring.others", {
                  names: namesLine(
                    others.map((f) => f.name),
                    t,
                  ),
                })}`
              : ""}
          </span>
          {/* The name alone on its line; a name too long for it breaks onto a second. */}
          <p
            aria-hidden
            data-ring-name=""
            className="text-[14px] leading-[1.3] font-semibold text-bone-50 [overflow-wrap:anywhere]"
          >
            {from.name}
            <PersonMark
              marks={from.marks}
              tooltip={false}
              className="ml-1 inline-flex align-[-2px]"
            />
          </p>
          <p aria-hidden className="mt-0.5 flex items-center gap-1.5 text-[12.5px] text-ash-300">
            {t("voice.ring.verb")}
            {others.length ? (
              <span className="flex items-center gap-1">
                {others.map((face) => (
                  <SocialFace
                    key={face.id}
                    source={face}
                    size={16}
                    className="overflow-hidden rounded-full ring-1 ring-obsidian-900"
                  />
                ))}
              </span>
            ) : null}
          </p>
        </div>
      </div>
      <div className="flex justify-end gap-2">
        <Button
          size="sm"
          variant="secondary"
          icon={<PhoneX size={14} weight="bold" className="text-flare-400" />}
          onClick={() => {
            toast.dismiss(toastId);
            void declineRing(ring.room);
          }}
        >
          {t("voice.ring.decline")}
        </Button>
        <Button
          size="sm"
          variant="secondary"
          icon={<Phone size={14} weight="fill" className="text-gold-300" />}
          onClick={() => {
            toast.dismiss(toastId);
            void answerRing(ring.room);
          }}
        >
          {t("voice.ring.join")}
        </Button>
      </div>
    </div>
  );
}

/** A ring that started: its card, for as long as it rings. */
export function showRingToast(ring: VoiceRing) {
  const left = ring.until * 1000 - Date.now();
  toast.custom((id) => <RingToast toastId={id} ring={ring} />, {
    id: ringToastId(ring.room),
    duration: Math.max(1000, left),
    classNames: { content: "min-w-0 flex-1", title: "w-full" },
  });
}

/** A ring that ended without the card (another device, the caller took it back, timed out). */
export function dismissRingToast(room: string) {
  toast.dismiss(ringToastId(room));
}
