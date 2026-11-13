// The call with one friend on the room panel (VOICE-CALL; the owner: "discord gibi güzel şeyler"):
// instead of a list of two, both faces large side by side, the friend first, each with the speaking
// ring and the ring that follows the voice (it reaches furthest here), the name and what they
// turned off under it, and the call's time under the two. While the player's call rings the
// friend's face waits faded with "Aranıyor…" ("Başka bir odada" when they are in another room:
// §3.2); the one way out is the footer's button, which says "İptal" then. A press or a right click
// on the friend opens their menu, as a row's does. SHARE-LIVE: a friend who shares has "YAYINDA" and
// "İzle" under their name (the stage glides to its new middle). Faces at 72 px (a multiple of 8: the pixel heads
// stay square), their names clear of the ring that follows the voice.
//
// SQOL-U3: with a chat under it the stage is compact (faces at 48 px, a band at the panel's top),
// and the friend's face says "konuşuyor" to screen readers while they speak. In a wide panel's
// people column the two faces stand one over the other.
import { cn, Menu, useMotionPrefs } from "@pl/ui";
import { motion } from "motion/react";
import { type ReactNode, type RefObject, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useRoomTargets } from "@/features/share/live";
import { LiveAppear, LiveTagButton } from "@/features/share/live-card";
import { WatchButton } from "@/features/share/panel";
import { glide } from "@/features/social/motion";
import { usePersonItems } from "@/features/social/person-menu";
import { useSocialUi } from "@/features/social/store";
import type { VoicePerson, VoiceRoomView, VoiceState } from "@/lib/api/voice-types";
import { CallTime, callStart } from "./call-time";
import { SpeakingFace } from "./faces";
import { presenceOf, roleWord, type VoiceFace } from "./rules";
import { useSpeakingWord } from "./speaking-word";
import { useFace, useVoiceUi } from "./store";

const FACE = 72;
/** With a chat under the stage (SQOL-U3): still a multiple of 8. */
const COMPACT_FACE = 48;

/** The panel shows the call this way: a friend room with one friend in it, or one being called. */
export function isOneToOne(state: Pick<VoiceState, "room" | "calling">): boolean {
  const room = state.room;
  if (room?.kind !== "friend" || room.ringing.length) return false;
  return state.calling ? room.people.length === 0 : room.people.length === 1;
}

function Seat({
  face,
  line,
  marks,
  dim = false,
  live,
  compact = false,
  children,
}: {
  face: VoiceFace;
  /** Under the name: "Sen", "Aranıyor…", a role. */
  line?: string | null;
  marks?: ReactNode;
  dim?: boolean;
  /** SHARE-LIVE: "YAYINDA" under the name (the seat is too narrow beside it). */
  live?: ReactNode;
  /** SQOL-U3: a chat under the stage: smaller faces, closer lines. */
  compact?: boolean;
  /** The face as a button (the friend's menu), or plain. */
  children?: (face: ReactNode) => ReactNode;
}) {
  const art = (
    <SpeakingFace
      face={face}
      size={compact ? COMPACT_FACE : FACE}
      dim={dim}
      speaking={dim ? false : undefined}
    />
  );
  return (
    <div
      className={cn(
        "flex min-w-0 flex-col items-center text-center",
        compact ? "w-[116px] gap-2.5" : "w-[132px] gap-3.5",
      )}
    >
      {children ? children(art) : art}
      <div className="flex min-w-0 max-w-full flex-col items-center">
        <span className="flex max-w-full items-center gap-1.5">
          <span
            className={cn(
              "truncate leading-[1.3] font-semibold text-bone-50",
              compact ? "text-[13px]" : "text-[14px]",
            )}
          >
            {face.name}
          </span>
          {marks}
        </span>
        {line ? <span className="text-[12px] leading-[1.35] text-ash-400">{line}</span> : null}
        {live}
      </div>
    </div>
  );
}

/** A seat's card beside the panel, level with the stage (as a row's card sits beside its row). */
const SEAT_CARD = { side: "left", align: "center", sideOffset: 14 } as const;

/** The friend, whose face opens their menu (their volume first) on a press or a right click. */
function FriendSeat({
  person,
  marks,
  stage,
  compact,
}: {
  person: VoicePerson;
  marks: ReactNode;
  /** The stage: the live card sits beside the panel, level with it. */
  stage: RefObject<HTMLElement | null>;
  compact: boolean;
}) {
  const { t } = useTranslation();
  const face = useFace(person.id);
  const [open, setOpen] = useState(false);
  const live = useRoomTargets(person.id);
  const speaking = useSpeakingWord(person.id);
  const ref = { id: face.id, name: face.name, avatar: face.avatar, avatarUrl: face.avatarUrl };
  const items = usePersonItems(ref, { from: "list" });
  return (
    <Seat
      face={face}
      line={roleWord(person.role, t)}
      marks={marks}
      compact={compact}
      live={
        // SHARE-LIVE: "YAYINDA" under the name, and "İzle" under it: both the friend's.
        <LiveAppear show={live.length > 0}>
          <span className="mt-1.5 flex flex-col items-center gap-2.5">
            <LiveTagButton targets={live} placement={{ ...SEAT_CARD, anchor: stage }} />
            <WatchButton person={person.id} name={face.name} />
          </span>
        </LiveAppear>
      }
    >
      {(art) => (
        <Menu
          items={items}
          align="center"
          open={open}
          onOpenChange={setOpen}
          trigger={
            <button
              type="button"
              aria-label={speaking ? `${face.name}, ${t("roomChat.speaking")}` : face.name}
              onContextMenu={(event) => {
                event.preventDefault();
                setOpen(true);
              }}
              onKeyDown={(event) => {
                if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
                  event.preventDefault();
                  setOpen(true);
                }
              }}
              className="cursor-pointer rounded-[calc(var(--radius-head)+4px)] outline-none transition-[transform,scale] duration-150 ease-out focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97]"
            >
              {art}
            </button>
          }
        />
      )}
    </Seat>
  );
}

/** The friend being called: faded, "Aranıyor…" (or "Başka bir odada"). */
function CallingSeat({ friend, compact }: { friend: string; compact: boolean }) {
  const { t } = useTranslation();
  const face = useFace(friend);
  const friends = useSocialUi((s) => s.view.friends);
  const busy = useVoiceUi((s) => presenceOf(friend, s.state, friends) === "in");
  return (
    <Seat
      face={face}
      dim
      compact={compact}
      line={busy ? t("voice.call.elsewhere") : t("voice.room.ringing")}
    />
  );
}

export function CallStage({
  room,
  marksOf,
  compact = false,
  stacked = false,
}: {
  room: VoiceRoomView;
  /** What someone turned off, as the panel's rows draw it. */
  marksOf: (
    person: Pick<VoicePerson, "mute" | "deaf" | "mutedByModerator" | "quality">,
  ) => ReactNode;
  /** SQOL-U3: a chat under the stage: a band at the panel's top instead of its middle. */
  compact?: boolean;
  /** SQOL-U3: a wide panel's people column: the faces one over the other, in its middle. */
  stacked?: boolean;
}) {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me);
  const calling = useVoiceUi((s) => s.state.calling);
  const since = useVoiceUi((s) => callStart(s.state));
  const friend = room.people[0];
  const stageRef = useRef<HTMLElement>(null);
  // What comes and goes under a seat ("YAYINDA", "İzle") moves the stage's middle: it glides there.
  const { layout } = useMotionPrefs();
  const moves = { layout: layout ? ("position" as const) : false, transition: { layout: glide } };
  const own = useRoomTargets(room.me.id);
  const meFace: VoiceFace = {
    id: room.me.id,
    name: me?.name ?? t("voice.room.you"),
    avatar: me?.avatar,
    avatarUrl: me?.avatarUrl,
    friend: false,
  };
  return (
    <section
      ref={stageRef}
      aria-label={t("voice.title")}
      // In the middle of the panel, a little above it (the eye's middle); over a chat, a band at
      // its top.
      className={cn(
        "flex flex-col items-center px-2",
        compact ? "pt-3 pb-2" : "flex-1 justify-center pt-4 pb-14",
      )}
      data-voice-stage=""
    >
      <motion.div
        {...moves}
        className={cn(
          "flex justify-center",
          stacked ? "flex-col items-center gap-7" : "items-start",
          !stacked && (compact ? "gap-4" : "gap-6"),
        )}
      >
        {friend ? (
          <FriendSeat person={friend} marks={marksOf(friend)} stage={stageRef} compact={compact} />
        ) : calling ? (
          <CallingSeat friend={calling.friend} compact={compact} />
        ) : null}
        <Seat
          face={meFace}
          compact={compact}
          line={t("voice.room.you")}
          marks={marksOf(room.me)}
          live={
            <LiveAppear show={own.length > 0}>
              <span className="mt-1.5 flex">
                <LiveTagButton targets={own} placement={{ ...SEAT_CARD, anchor: stageRef }} />
              </span>
            </LiveAppear>
          }
        />
      </motion.div>
      {/* The call's time; its room is kept while the call still rings, so nothing moves on an answer. */}
      <motion.div
        {...moves}
        className={cn("flex items-center", compact ? "mt-1.5 h-6" : "mt-5 h-8")}
      >
        {since !== null ? (
          <CallTime
            since={since}
            className={cn("font-semibold text-ash-300", compact ? "text-[13px]" : "text-[15px]")}
          />
        ) : null}
      </motion.div>
    </section>
  );
}
