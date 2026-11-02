// A call or invite ringing for the player (VOICE-L3; voice.md §5.1), in the invite toast's place and
// form (one row): the caller's face, "Kaan_07 arıyor" with the others in the room as small faces
// under it, then "Reddet" (a call's red glyph, its word the tooltip and the name, as the invite
// toast's ×) and "Katıl" (secondary, the gold glyph: a toast rests right above OYNA, design.md §6).
// One row keeps it as low as the other toasts, clear of the news column's links. It stays while
// the ring rings and leaves when it ends (taken on another device, declined, the room gone).
import { Phone, PhoneX } from "@phosphor-icons/react";
import { Button, IconButton, toast } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { PersonMark } from "@/features/social/person-mark";
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
    <div className="flex w-full items-center gap-3 font-normal">
      <SocialAvatar source={from} size={40} />
      <div className="min-w-0 flex-1">
        {/* A long name gives way, never the verb ("GeceKurdu_ is call…" said nothing). Both
            languages put the name first. */}
        <p className="text-[13.5px] leading-snug font-semibold text-bone-50">
          <span className="sr-only">{t("voice.ring.calling", { name: from.name })}</span>
          <span aria-hidden className="flex min-w-0 items-center gap-1">
            <span className="min-w-0 truncate">{from.name}</span>
            <PersonMark marks={from.marks} tooltip={false} className="shrink-0" />
            <span className="shrink-0">{t("voice.ring.verb")}</span>
          </span>
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
      <IconButton
        label={t("voice.ring.decline")}
        icon={<PhoneX size={17} weight="bold" className="text-flare-400" />}
        size="sm"
        onClick={() => {
          toast.dismiss(toastId);
          void declineRing(ring.room);
        }}
      />
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
