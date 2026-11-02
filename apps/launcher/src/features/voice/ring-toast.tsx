// A call or invite ringing for the player (VOICE-L3; voice.md §5.1), in the invite toast's place: the
// caller's face, "Kaan_07 arıyor", the others in the room as small faces, then "Reddet" and "Katıl"
// (secondary, the gold glyph on "Katıl": a toast rests right above OYNA, design.md §6). It stays
// while the ring rings and leaves when it ends (taken on another device, declined, the room gone).
// The ring's sound is the launcher's (Rust), so it rings with the page asleep too; a window that is
// away gets the Windows notification with the same two buttons.
import { Phone, PhoneX } from "@phosphor-icons/react";
import { Button, toast } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { MarkedSentence } from "@/features/social/person-mark";
import { SocialAvatar, SocialFace } from "@/features/social/social-face";
import type { VoiceRing } from "@/lib/api/voice-types";
import { answerRing, declineRing } from "./actions";
import { namesLine } from "./rules";
import { useFace, useFaces } from "./store";

const ringToastId = (room: string) => `voice-ring:${room}`;

function RingToast({ toastId, ring }: { toastId: string | number; ring: VoiceRing }) {
  const { t } = useTranslation();
  const from = useFace(ring.from);
  const others = useFaces(ring.with);
  return (
    <div className="flex w-full flex-col gap-3 font-normal">
      <div className="flex items-center gap-3">
        <SocialAvatar source={from} size={40} />
        <div className="min-w-0 flex-1">
          <p className="text-[13.5px] leading-snug font-semibold text-bone-50">
            <MarkedSentence
              text={(who) => t("voice.ring.calling", { name: who })}
              name={from.name}
              marks={from.marks}
            />
          </p>
          {others.length ? (
            <p
              className="mt-1 flex items-center gap-1"
              role="img"
              aria-label={t("voice.ring.others", {
                names: namesLine(
                  others.map((f) => f.name),
                  t,
                ),
              })}
            >
              {others.map((face) => (
                <SocialFace
                  key={face.id}
                  source={face}
                  size={16}
                  className="overflow-hidden rounded-full ring-1 ring-obsidian-900"
                />
              ))}
            </p>
          ) : null}
        </div>
      </div>
      <div className="flex justify-end gap-2">
        <Button
          size="sm"
          variant="secondary"
          icon={<PhoneX size={14} weight="bold" />}
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
