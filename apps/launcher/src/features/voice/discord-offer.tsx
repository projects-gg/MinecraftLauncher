// Discord's voice keys, offered once (VOICE-KEYS; owner, 2 Nov: "discordu varsa … butonlar
// kopyalansın mı diye ilk sesli sohbete girerken"): the first time the player opens voice chat (the
// first room, or Settings → Sesli sohbet, whichever comes first), when Discord on this computer has
// push-to-talk, mute or deafen keys, a small card lists them in our words ("Bas-konuş: Fare 4",
// "Mikrofonu kapat/aç: Ctrl + Shift + M") with "Hayır" and "Kopyala"; one line names a key that
// cannot be used here. Either answer is final (the launcher remembers it on this computer); nothing
// shows when Discord has none. In a room the card is a toast (the ring card's place), except on the
// crew page of the player's room, where it sits under the rooms (CREW-POLISH: the toast lay over
// the members until answered); on the settings page it sits at the top of the section, where the
// copied keys then show.
import { Warning } from "@phosphor-icons/react";
import { Button, cn, toast } from "@pl/ui";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { enter, leave } from "@/features/social/motion";
import { voiceApi } from "@/lib/api/voice-api";
import type { VoiceDiscordOffer, VoiceSettings, VoiceShortcut } from "@/lib/api/voice-types";
import i18n from "@/lib/i18n";
import { isWindowAway } from "@/lib/window-away";
import { KeyCaps } from "./key-picker";
import { keyParts } from "./rules";
import { useVoiceUi } from "./store";

type Phase = "idle" | "asking" | "open" | "done";

interface DiscordAsk {
  phase: Phase;
  offer: VoiceDiscordOffer | null;
  /** The settings as kept after "Kopyala" (an open settings page takes them). */
  copied: VoiceSettings | null;
  /** Pages showing the card in place (settings, the crew page of the player's room): no toast. */
  inline: number;
}

export const useDiscordAsk = create<DiscordAsk>(() => ({
  phase: "idle",
  offer: null,
  copied: null,
  inline: 0,
}));

const TOAST_ID = "voice-discord";
/** The card is up as a toast. */
let toastUp = false;
/** The toast's next `onDismiss` is ours (answered, or moved in place), not the player's "Hayır".
 * Sonner calls it after its own render, so a flag set only around `dismiss` would be gone. */
let quiet = false;

function closeToast() {
  if (!toastUp) return;
  quiet = true;
  toast.dismiss(TOAST_ID);
}

/**
 * Asks the launcher once per run (it answers null once the player answered, or when Discord has
 * nothing to copy); in a room the card shows as a toast unless a settings page shows it in place.
 */
export async function askDiscord(where: "room" | "settings") {
  if (useDiscordAsk.getState().phase === "idle") {
    useDiscordAsk.setState({ phase: "asking" });
    let offer: VoiceDiscordOffer | null = null;
    try {
      offer = await voiceApi.discordOffer();
    } catch {
      // Nothing to offer this time; the launcher asks again on the next start.
    }
    useDiscordAsk.setState(offer ? { phase: "open", offer } : { phase: "done", offer: null });
  }
  const now = useDiscordAsk.getState();
  if (now.phase === "open" && where === "room" && now.inline === 0) {
    toast.custom(() => <DiscordToast />, {
      id: TOAST_ID,
      duration: Number.POSITIVE_INFINITY,
      classNames: { content: "min-w-0 flex-1", title: "w-full" },
      // Closed by the player: as "Hayır".
      onDismiss: () => {
        toastUp = false;
        if (quiet) {
          quiet = false;
          return;
        }
        void answerDiscord(false);
      },
    });
    toastUp = true;
  }
}

/** "Kopyala" or "Hayır": final either way. */
export async function answerDiscord(copy: boolean): Promise<VoiceSettings | null> {
  if (useDiscordAsk.getState().phase !== "open") return null;
  useDiscordAsk.setState({ phase: "done" });
  closeToast();
  try {
    const kept = await voiceApi.discordAnswer(copy);
    if (copy) useDiscordAsk.setState({ copied: kept });
    return kept;
  } catch {
    toast.error(i18n.t("voice.refused.failed"));
    return null;
  }
}

/** The shortcut's name as the settings call it ("Bas-konuş", "Mikrofonu kapat/aç", "Sağırlaştır"). */
function shortcutName(shortcut: VoiceShortcut, t: TFunction): string {
  if (shortcut === "pushToTalk") return t("voice.settings.modePtt");
  if (shortcut === "mute") return t("voice.settings.muteKey");
  return t("voice.settings.deafenKey");
}

/** "Bas-konuş kısayolu burada kullanılamıyor." (two at most: with all three there is no card). */
export function skippedLine(skipped: readonly VoiceShortcut[], t: TFunction): string | null {
  if (!skipped.length) return null;
  const names = skipped.map((s) => shortcutName(s, t));
  const joined =
    names.length === 1
      ? names[0]
      : t("voice.discord.and", { first: names.slice(0, -1).join(", "), last: names.at(-1) });
  return t("voice.discord.skipped", { names: joined, count: names.length });
}

function OfferCard({
  offer,
  onAnswer,
  className,
}: {
  offer: VoiceDiscordOffer;
  onAnswer: (copy: boolean) => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const rows = (
    [
      ["pushToTalk", offer.pushToTalk],
      ["mute", offer.mute],
      ["deafen", offer.deafen],
    ] as const
  ).filter(([, key]) => key !== null);
  const skipped = skippedLine(offer.skipped, t);
  return (
    <div className={cn("flex w-full flex-col gap-3 font-normal", className)}>
      <p className="text-[14px] leading-[1.3] font-semibold text-bone-50">
        {t("voice.discord.title")}
      </p>
      <dl className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-2">
        {rows.map(([shortcut, key]) =>
          key ? (
            <div key={shortcut} className="contents">
              <dt className="truncate text-[12.5px] text-ash-300">{shortcutName(shortcut, t)}</dt>
              <dd className="justify-self-end">
                <KeyCaps parts={keyParts(key, t)} small />
              </dd>
            </div>
          ) : null,
        )}
      </dl>
      {skipped ? (
        <p className="flex items-start gap-1.5 text-[12px] leading-[1.4] text-bone-100">
          <Warning
            size={12}
            weight="fill"
            aria-hidden
            className="mt-[2px] shrink-0 text-gold-400"
          />
          {skipped}
        </p>
      ) : null}
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="secondary" onClick={() => onAnswer(false)}>
          {t("voice.discord.no")}
        </Button>
        <Button size="sm" variant="secondary" onClick={() => onAnswer(true)}>
          {t("voice.discord.copy")}
        </Button>
      </div>
    </div>
  );
}

/** The card as a toast (in a room): "Kopyala" says it is done, the page is not in sight. The
 * answer closes the toast itself (as answered, not as "Hayır"). */
export function DiscordToast() {
  const offer = useDiscordAsk((s) => s.offer);
  if (!offer) return null;
  return (
    <OfferCard
      offer={offer}
      onAnswer={(copy) => {
        void answerDiscord(copy).then((kept) => {
          if (kept && copy) toast(i18n.t("voice.discord.copied"));
        });
      }}
    />
  );
}

/** The card in place at the top of Settings → Sesli sohbet (the copied keys show in the rows), and
 * under the crew page's rooms. It fades in and out (opacity); what is under it glides to its place
 * (the page's layout). `className` sets its margins (the settings' by default); `say`: "Kopyala"
 * says it is done, as the toast does (a page without the key rows). */
export function DiscordInline({
  className = "mt-3 mb-1",
  say = false,
}: {
  className?: string;
  say?: boolean;
}) {
  const phase = useDiscordAsk((s) => s.phase);
  const offer = useDiscordAsk((s) => s.offer);
  const shown = phase === "open" && offer !== null;
  return (
    <AnimatePresence initial={false}>
      {shown && offer ? (
        <motion.section
          key="discord"
          aria-label={i18n.t("voice.discord.title")}
          className={cn(
            "rounded-[var(--radius-panel)] bg-white/[0.035] p-4 ring-1 ring-white/8",
            className,
          )}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.22) }}
          exit={{ opacity: 0, transition: leave(0.16) }}
        >
          <OfferCard
            offer={offer}
            onAnswer={(copy) => {
              void answerDiscord(copy).then((kept) => {
                if (say && kept && copy) toast(i18n.t("voice.discord.copied"));
              });
            }}
          />
        </motion.section>
      ) : null}
    </AnimatePresence>
  );
}

/**
 * On the crew page while the player is in one of its rooms (CREW-POLISH): the card under the rooms,
 * in the page's flow, where the toast lay over the members. A toast already up moves here; leaving
 * the page or the room unanswered hands it back to the toast (in a room, with the window in front),
 * after the render, so another page that shows it in place counts first.
 */
export function DiscordInRoom({ active }: { active: boolean }) {
  useEffect(() => {
    if (!active) return;
    useDiscordAsk.setState((s) => ({ inline: s.inline + 1 }));
    closeToast();
    return () => {
      useDiscordAsk.setState((s) => ({ inline: s.inline - 1 }));
      window.setTimeout(() => {
        const open = useDiscordAsk.getState().phase === "open";
        if (open && useVoiceUi.getState().state.room && !isWindowAway()) void askDiscord("room");
      }, 0);
    };
  }, [active]);
  return active ? <DiscordInline className="mt-3" say /> : null;
}
