// The call with one friend on the room panel (VOICE-CALL; the owner: "discord gibi güzel şeyler"):
// instead of a list of two, both faces large side by side, the friend first, each with the speaking
// ring and the ring that follows the voice (it reaches furthest here), the name and what they
// turned off under it, and the call's time under the two. While the player's call rings the
// friend's face waits faded with "Aranıyor…" ("Başka bir odada" when they are in another room:
// §3.2); the one way out is the footer's button, which says "İptal" then. A press or a right click
// on the friend opens their menu, as a row's does. Faces at 72 px (a multiple of 8: the pixel heads
// stay square), their names clear of the ring that follows the voice.
import { Menu } from "@pl/ui";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { usePersonItems } from "@/features/social/person-menu";
import { useSocialUi } from "@/features/social/store";
import type { VoicePerson, VoiceRoomView, VoiceState } from "@/lib/api/voice-types";
import { CallTime, callStart } from "./call-time";
import { SpeakingFace } from "./faces";
import { presenceOf, roleWord, type VoiceFace } from "./rules";
import { useFace, useVoiceUi } from "./store";

const FACE = 72;

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
  children,
}: {
  face: VoiceFace;
  /** Under the name: "Sen", "Aranıyor…", a role. */
  line?: string | null;
  marks?: ReactNode;
  dim?: boolean;
  /** The face as a button (the friend's menu), or plain. */
  children?: (face: ReactNode) => ReactNode;
}) {
  const art = <SpeakingFace face={face} size={FACE} dim={dim} speaking={dim ? false : undefined} />;
  return (
    <div className="flex w-[132px] min-w-0 flex-col items-center gap-3.5 text-center">
      {children ? children(art) : art}
      <div className="flex min-w-0 max-w-full flex-col items-center">
        <span className="flex max-w-full items-center gap-1.5">
          <span className="truncate text-[14px] leading-[1.3] font-semibold text-bone-50">
            {face.name}
          </span>
          {marks}
        </span>
        {line ? <span className="text-[12px] leading-[1.35] text-ash-400">{line}</span> : null}
      </div>
    </div>
  );
}

/** The friend, whose face opens their menu (their volume first) on a press or a right click. */
function FriendSeat({ person, marks }: { person: VoicePerson; marks: ReactNode }) {
  const { t } = useTranslation();
  const face = useFace(person.id);
  const [open, setOpen] = useState(false);
  const ref = { id: face.id, name: face.name, avatar: face.avatar, avatarUrl: face.avatarUrl };
  const items = usePersonItems(ref, { from: "list" });
  return (
    <Seat face={face} line={roleWord(person.role, t)} marks={marks}>
      {(art) => (
        <Menu
          items={items}
          align="center"
          open={open}
          onOpenChange={setOpen}
          trigger={
            <button
              type="button"
              aria-label={face.name}
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
function CallingSeat({ friend }: { friend: string }) {
  const { t } = useTranslation();
  const face = useFace(friend);
  const friends = useSocialUi((s) => s.view.friends);
  const busy = useVoiceUi((s) => presenceOf(friend, s.state, friends) === "in");
  return <Seat face={face} dim line={busy ? t("voice.call.elsewhere") : t("voice.room.ringing")} />;
}

export function CallStage({
  room,
  marksOf,
}: {
  room: VoiceRoomView;
  /** What someone turned off, as the panel's rows draw it. */
  marksOf: (
    person: Pick<VoicePerson, "mute" | "deaf" | "mutedByModerator" | "quality">,
  ) => ReactNode;
}) {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me);
  const calling = useVoiceUi((s) => s.state.calling);
  const since = useVoiceUi((s) => callStart(s.state));
  const friend = room.people[0];
  const meFace: VoiceFace = {
    id: room.me.id,
    name: me?.name ?? t("voice.room.you"),
    avatar: me?.avatar,
    avatarUrl: me?.avatarUrl,
    friend: false,
  };
  return (
    <section
      aria-label={t("voice.title")}
      // In the middle of the panel, a little above it (the eye's middle).
      className="flex flex-1 flex-col items-center justify-center px-2 pt-4 pb-14"
      data-voice-stage=""
    >
      <div className="flex items-start justify-center gap-6">
        {friend ? (
          <FriendSeat person={friend} marks={marksOf(friend)} />
        ) : calling ? (
          <CallingSeat friend={calling.friend} />
        ) : null}
        <Seat face={meFace} line={t("voice.room.you")} marks={marksOf(room.me)} />
      </div>
      {/* The call's time; its room is kept while the call still rings, so nothing moves on an answer. */}
      <div className="mt-5 flex h-8 items-center">
        {since !== null ? (
          <CallTime since={since} className="text-[15px] font-semibold text-ash-300" />
        ) : null}
      </div>
    </section>
  );
}
