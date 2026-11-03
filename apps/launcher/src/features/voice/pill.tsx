// The room in the title bar (VOICE-L3; voice.md §5.1), left of the friends pill, only while the
// player is in a room or calling: up to three faces with their speaking rings (and, on a face, what
// that person turned off), the room's name, and "Mikrofon" and "Ayrıl" as round buttons inside the
// pill (round inside round), the headphones off before them while the player is deafened
// (VOICE-KEYS). A click on the faces
// or the name opens the room panel. While the player's call rings it says "Aranıyor…" with "İptal";
// a call nobody took stays a moment as "Cevap yok" with "Tekrar ara", a connection that did not
// come back as "Bağlanamadı" with "Tekrar dene" (8 s, held while the pointer or the focus is on it).
// The pill glides to its new width (transform, on the drawer's spring; Performance mode: in place);
// words that change cross-fade in place. Nothing moves while nothing changes.
//
// VOICE-CALL: the microphone's meter before its button (its bars move with the player's voice while
// the window is in front: meter.tsx), and in a call with one friend the call's time after the name
// (call-time.tsx). Both keep their room, so nothing beside them shifts.
import {
  Headset,
  Microphone,
  MicrophoneSlash,
  Phone,
  PhoneDisconnect,
  WarningCircle,
} from "@phosphor-icons/react";
import { cn, Tooltip, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { enter, leave, swap } from "@/features/social/motion";
import { callFriend, leaveRoom, retryRoom, toggleDeaf, toggleMute } from "./actions";
import { CallTime, callStart } from "./call-time";
import { QualityBars, SpeakingFace } from "./faces";
import { HeadphonesSlash } from "./icons";
import { VoiceMeter } from "./meter";
import { meterThreshold, useVoicePrefs } from "./prefs";
import { faceMark, pillPeople, roomTitle } from "./rules";
import { useFace, useFaces, useVoiceUi, voiceOn } from "./store";

/** How long the pill keeps an ended stay ("Cevap yok", "Bağlanamadı"). */
export const ENDED_MS = 8000;

const glide = { type: "spring", stiffness: 480, damping: 40 } as const;

export function VoicePill() {
  const shown = useVoiceUi(
    (s) =>
      voiceOn(s.state) && (s.state.room !== null || s.state.calling !== null || s.ended !== null),
  );
  return <AnimatePresence initial={false}>{shown ? <Pill key="voice" /> : null}</AnimatePresence>;
}

/** A round action inside the pill: 22 px, 3 px in from its edge. */
function RoundButton({
  label,
  onClick,
  tone = "quiet",
  disabled = false,
  children,
}: {
  label: string;
  onClick: () => void;
  tone?: "quiet" | "off" | "leave";
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <Tooltip content={label}>
      <button
        type="button"
        aria-label={label}
        aria-disabled={disabled || undefined}
        onClick={disabled ? undefined : onClick}
        className={cn(
          "relative flex size-[22px] cursor-pointer items-center justify-center rounded-full outline-none",
          "transition-[background-color,color,transform,scale] duration-150 ease-out active:scale-[0.92]",
          "focus-visible:ring-2 focus-visible:ring-gold-300",
          // The title bar is 34 px tall: the press reaches it all, not only the drawn circle.
          "after:absolute after:-inset-x-0.5 after:-inset-y-1.5",
          tone === "quiet" && "text-ash-300 hover:bg-white/10 hover:text-bone-50",
          tone === "off" && "text-flare-400 hover:bg-flare-400/14",
          tone === "leave" && "text-ash-300 hover:bg-flare-400/16 hover:text-flare-300",
          disabled && "cursor-default opacity-60 hover:bg-transparent",
        )}
      >
        {children}
      </button>
    </Tooltip>
  );
}

/** A worded action inside the pill ("İptal", "Tekrar ara", "Tekrar dene"). */
function WordButton({
  children,
  onClick,
  tone = "quiet",
  label,
  icon,
}: {
  children: string;
  onClick: () => void;
  tone?: "quiet" | "cancel";
  label?: string;
  /** A call's glyph on "Tekrar ara": "ara" is also "search", the glyph says which. */
  icon?: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className={cn(
        "relative flex h-[22px] cursor-pointer items-center gap-1 rounded-full px-2.5 text-[12px] font-semibold whitespace-nowrap outline-none",
        "transition-[background-color,color,transform,scale] duration-150 ease-out active:scale-[0.95]",
        "focus-visible:ring-2 focus-visible:ring-gold-300 after:absolute after:inset-x-0 after:-inset-y-1.5",
        tone === "quiet" && "bg-white/10 text-bone-100 hover:bg-white/16 hover:text-bone-50",
        tone === "cancel" && "bg-white/8 text-bone-100 hover:bg-flare-400/16 hover:text-flare-300",
        icon ? "pl-2" : null,
      )}
    >
      {icon}
      {children}
    </button>
  );
}

function Pill() {
  const { t } = useTranslation();
  const { reduce, layout } = useMotionPrefs();
  const state = useVoiceUi((s) => s.state);
  const ended = useVoiceUi((s) => s.ended);
  const setEnded = useVoiceUi((s) => s.setEnded);
  const openPanel = useVoiceUi((s) => s.openPanel);
  const trigger = useRef<HTMLButtonElement>(null);
  const [held, setHeld] = useState(false);

  // An ended stay leaves by itself unless the player is on it.
  useEffect(() => {
    if (!ended || held) return;
    const timer = window.setTimeout(() => setEnded(null), ENDED_MS);
    return () => window.clearTimeout(timer);
  }, [ended, held, setEnded]);

  const room = state.room;
  const calling = state.calling;
  const since = callStart(state);
  const threshold = useVoicePrefs((s) => meterThreshold(s.settings));
  const people = room ? pillPeople(room) : [];
  const faces = useFaces(people.map((p) => p.id));
  const others = useFaces(room?.people.map((p) => p.id) ?? []);
  const callee = useFace(calling?.friend ?? (ended?.kind === "noAnswer" ? ended.friend : ""));
  const nameOf = (id: string) => others.find((f) => f.id === id)?.name ?? callee.name;

  // What the pill says, keyed so a change cross-fades.
  const mode = calling
    ? "calling"
    : room
      ? room.phase
      : ended?.kind === "noAnswer"
        ? "noAnswer"
        : "failed";
  const title = room ? roomTitle(room, nameOf, t, calling?.friend) : "";
  const words =
    mode === "calling"
      ? t("voice.call.ringingName", { name: callee.name })
      : mode === "joining"
        ? t("voice.room.joining")
        : mode === "reconnecting"
          ? t("voice.room.reconnecting")
          : mode === "noAnswer"
            ? t("voice.call.noAnswer")
            : mode === "failed"
              ? t("voice.room.failed")
              : title;
  const quality = room?.me.quality ?? "good";
  const weak = quality !== "good" && mode === "live";
  const me = room?.me;
  // The microphone's buttons (and its meter) show in the room itself, not while a call rings.
  const metered = Boolean(me) && mode !== "calling" && mode !== "noAnswer" && mode !== "failed";
  const micOff = Boolean(me && (me.mute || me.deaf || me.mutedByModerator));
  const micLabel = me?.mutedByModerator
    ? t("voice.mutedByMod")
    : micOff
      ? t("voice.room.unmuteMic")
      : t("voice.room.muteMic");
  const open = () => openPanel({ from: trigger.current });

  return (
    <motion.div
      layout={layout}
      transition={{ layout: glide }}
      // Rounded through `style` so the glide keeps its corners round.
      style={{ borderRadius: 14 }}
      className="flex h-8 items-center gap-1 bg-obsidian-950/45 py-[3px] pr-[3px] pl-[3px] ring-1 ring-white/8 backdrop-blur-md"
      initial={{ opacity: 0, scale: reduce ? 1 : 0.94 }}
      animate={{ opacity: 1, scale: 1, transition: enter(0.24) }}
      exit={{ opacity: 0, scale: reduce ? 1 : 0.96, transition: leave(0.2) }}
      onPointerEnter={() => setHeld(true)}
      onPointerLeave={() => setHeld(false)}
      onFocus={() => setHeld(true)}
      onBlur={() => setHeld(false)}
    >
      <motion.button
        ref={trigger}
        layout={layout ? "position" : false}
        type="button"
        aria-haspopup="dialog"
        aria-label={
          weak
            ? `${t("voice.pill.label", { room: words })}, ${t("voice.room.weak")}`
            : t("voice.pill.label", { room: words })
        }
        disabled={!room}
        onClick={open}
        className={cn(
          "group relative flex h-[22px] min-w-0 items-center gap-2 rounded-full pr-2 outline-none",
          "focus-visible:ring-2 focus-visible:ring-gold-300",
          room ? "cursor-pointer" : "cursor-default",
        )}
      >
        <span className="flex items-center gap-[7px] pl-px">
          {mode === "failed" ? (
            <WarningCircle size={18} weight="fill" aria-hidden className="text-flare-400" />
          ) : mode === "calling" || mode === "noAnswer" ? (
            <SpeakingFace
              face={callee}
              size={20}
              round
              dim={mode === "noAnswer"}
              speaking={false}
            />
          ) : faces.length ? (
            faces.map((face) => {
              // What they turned off, on their face (nothing while both are on).
              const person = people.find((p) => p.id === face.id);
              return (
                <SpeakingFace
                  key={face.id}
                  face={face}
                  size={20}
                  round
                  mark={person ? faceMark(person) : null}
                />
              );
            })
          ) : (
            // Alone in the room: the feature's mark, no stand-in face.
            <span className="flex size-5 items-center justify-center rounded-full bg-white/8">
              <Headset size={13} weight="bold" aria-hidden className="text-ash-300" />
            </span>
          )}
        </span>
        {/* A little narrower beside the meter (VOICE-CALL): the pill keeps its width at 1024. */}
        <span
          className={cn(
            "relative grid min-w-0",
            metered ? "max-w-[clamp(72px,9vw,150px)]" : "max-w-[clamp(84px,11vw,160px)]",
          )}
        >
          <AnimatePresence initial={false} mode="popLayout">
            <motion.span
              key={`${mode}:${mode === "live" ? words : ""}`}
              className={cn(
                "truncate text-[12.5px] leading-[18px] font-medium [grid-area:1/1]",
                mode === "live"
                  ? "text-bone-50 transition-colors group-hover:text-bone-50"
                  : "text-ash-300",
              )}
              {...swap(reduce, 0.18)}
            >
              {words}
            </motion.span>
          </AnimatePresence>
        </span>
        {/* A call with one friend: how long it has been on (VOICE-CALL). */}
        {mode === "live" && since !== null ? (
          <CallTime since={since} className="shrink-0 text-[12px] leading-[18px] text-ash-300" />
        ) : null}
        {weak ? (
          <Tooltip content={t("voice.room.weak")}>
            <span className="flex shrink-0">
              <QualityBars quality={quality} />
            </span>
          </Tooltip>
        ) : null}
      </motion.button>

      <motion.span layout={layout ? "position" : false} className="flex items-center gap-0.5">
        {mode === "calling" ? (
          <WordButton
            tone="cancel"
            label={t("voice.call.cancelLabel")}
            onClick={() => void leaveRoom()}
          >
            {t("voice.call.cancel")}
          </WordButton>
        ) : mode === "noAnswer" && ended?.kind === "noAnswer" ? (
          <WordButton
            icon={<Phone size={12} weight="fill" aria-hidden />}
            onClick={() => void callFriend(ended.friend)}
          >
            {t("voice.call.again")}
          </WordButton>
        ) : mode === "failed" ? (
          <WordButton onClick={() => void retryRoom()}>{t("voice.room.retry")}</WordButton>
        ) : me ? (
          <>
            {/* Deafened, both are off and both say so (Discord's way): the headphones undeafen, the
                microphone comes back as it was. */}
            <AnimatePresence initial={false}>
              {me.deaf ? (
                <motion.span
                  key="deaf"
                  className="flex"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1, transition: enter(0.18) }}
                  exit={{ opacity: 0, transition: leave(0.12) }}
                >
                  <RoundButton
                    label={t("voice.room.deafenOff")}
                    tone="off"
                    onClick={() => void toggleDeaf()}
                  >
                    <HeadphonesSlash size={15} />
                  </RoundButton>
                </motion.span>
              ) : null}
            </AnimatePresence>
            {/* What the microphone hears (VOICE-CALL); still while it is off. */}
            <VoiceMeter threshold={threshold} className={cn("mx-1", micOff && "opacity-60")} />
            <RoundButton
              label={micLabel}
              tone={micOff ? "off" : "quiet"}
              disabled={me.mutedByModerator}
              onClick={() => void toggleMute()}
            >
              {micOff ? (
                <MicrophoneSlash size={15} weight="bold" aria-hidden />
              ) : (
                <Microphone size={15} weight="bold" aria-hidden />
              )}
            </RoundButton>
            <RoundButton
              label={t("voice.room.leave")}
              tone="leave"
              onClick={() => void leaveRoom()}
            >
              <PhoneDisconnect size={15} weight="bold" aria-hidden />
            </RoundButton>
          </>
        ) : null}
      </motion.span>
    </motion.div>
  );
}
