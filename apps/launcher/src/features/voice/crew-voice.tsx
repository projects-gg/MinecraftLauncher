// The crew page's "Sesli" (VOICE-L3; voice.md §5.1): the crew's live rooms, who is in each and
// "Katıl"; with nobody in any, "Genel" alone with "Katıl" (no explanation). The player's own room
// sits on the quiet plate with "Ayrıl". A small "Yeni oda" while the crew may open one more. Rows
// that come and go fade where they are; the others glide.
import { Plus } from "@phosphor-icons/react";
import { Button, cn } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { enter, glide, leave, useSocialMotion } from "@/features/social/motion";
import { SocialFace } from "@/features/social/social-face";
import type { SocialTeam } from "@/lib/api/social-types";
import type { VoiceCrewRoom } from "@/lib/api/voice-types";
import { joinCrewRoom, leaveRoom, openCrewRoom } from "./actions";
import { crewRoomName, namesLine } from "./rules";
import { useFaces, useVoiceUi, voiceOn } from "./store";

/** Faces a row shows before "+N". */
const FACES = 5;

function RoomRow({ crew, room, here }: { crew: string; room: VoiceCrewRoom; here: boolean }) {
  const { t } = useTranslation();
  const faces = useFaces(room.people);
  const shown = faces.slice(0, FACES);
  const more = faces.length - shown.length;
  return (
    <div
      className={cn(
        "flex min-h-[44px] items-center gap-3 rounded-md py-1.5 pr-1.5 pl-3 transition-colors duration-200",
        here && "bg-white/[0.06]",
      )}
    >
      <span className="w-[88px] shrink-0 truncate text-[13.5px] font-semibold text-bone-50">
        {crewRoomName(room, t)}
      </span>
      <span className="flex min-w-0 flex-1 items-center">
        {faces.length ? (
          <span
            role="img"
            aria-label={namesLine(
              faces.map((f) => f.name),
              t,
            )}
            className="flex min-w-0 items-center gap-1"
          >
            {shown.map((face) => (
              <SocialFace
                key={face.id}
                source={face}
                size={24}
                className="overflow-hidden rounded-[var(--radius-head)]"
              />
            ))}
            {more > 0 ? (
              <span className="pl-1 text-[12.5px] font-medium text-ash-300 tabular-nums">
                +{more}
              </span>
            ) : null}
          </span>
        ) : null}
      </span>
      {here ? (
        <Button size="sm" variant="ghost" onClick={() => void leaveRoom()}>
          {t("voice.crew.leave")}
        </Button>
      ) : (
        <Button size="sm" variant="secondary" onClick={() => void joinCrewRoom(crew, room.id)}>
          {t("voice.crew.join")}
        </Button>
      )}
    </div>
  );
}

export function CrewVoice({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const on = useVoiceUi((s) => voiceOn(s.state));
  const entry = useVoiceUi((s) => s.state.crews.find((c) => c.crew === team.id));
  const max = useVoiceUi((s) => s.state.crewRoomsMax);
  const here = useVoiceUi((s) => (s.state.room?.crew === team.id ? s.state.room.id : null));
  if (!on) return null;
  // No live room: "Genel" alone, its id the crew's own (the service's `g` + crew id).
  const rooms: VoiceCrewRoom[] = entry?.rooms.length
    ? entry.rooms
    : [{ id: `g${team.id}`, name: "", general: true, people: [] }];
  const canAdd = entry ? entry.canAdd : max > 1;
  return (
    <section className="mt-5" aria-labelledby={`voice-${team.id}`}>
      <div className="mb-1.5 flex h-8 items-center gap-2">
        <h3 id={`voice-${team.id}`} className="flex-1 text-[13px] font-semibold text-ash-300">
          {t("voice.crew.title")}
        </h3>
        {canAdd ? (
          <Button
            size="sm"
            variant="ghost"
            icon={<Plus size={14} weight="bold" />}
            onClick={() => void openCrewRoom(team.id)}
          >
            {t("voice.crew.newRoom")}
          </Button>
        ) : null}
      </div>
      <ul className="rounded-[var(--radius-panel)] bg-obsidian-950/40 p-1 ring-1 ring-white/8">
        <AnimatePresence initial={false} mode="popLayout">
          {rooms.map((room) => (
            <motion.li
              key={room.id}
              layout={layout ? "position" : false}
              transition={{ layout: glide }}
              initial={{ opacity: 0, y: reduce ? 0 : 4 }}
              animate={{ opacity: 1, y: 0, transition: enter(0.24) }}
              exit={{ opacity: 0, transition: leave(0.2) }}
            >
              <RoomRow crew={team.id} room={room} here={room.id === here} />
            </motion.li>
          ))}
        </AnimatePresence>
      </ul>
    </section>
  );
}
