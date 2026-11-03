// The crew page's "Sesli" (VOICE-L3, VOICE-ROOMS; voice.md §5.1): the crew's rooms, "Genel" always
// first, who is in each and "Katıl"; the player's own room sits on the quiet plate with "Ayrıl".
// "Yeni oda" while the crew may open one more. Written as the page's other sections are: no box of
// its own; the heading and every row's words start on one line with the members' faces (8 px in
// from the plates' edge), and the buttons end where "Plan yap" and the members' "Katıl" end (8 px
// in, then the place of a row's "⋯").
//
// While the crew arranges its rooms (Studio's "Oda yönetimi", `manage`): a right click on a room
// (Shift+F10, the menu key) or its "⋯" (shown under the pointer and the focus) opens the room's
// menu by role; editing and the question before deleting a room with people in it take the row's
// place. The leader and moderators drag rooms into a new order (Alt+↑/↓ from the keyboard) and
// crewmates' faces from one room to another (a face's menu says "Taşı" too). Rows that come and go
// fade where they are; the others glide; a dragged row lifts (transform and opacity only; fades
// only with less motion).
import { DotsThree, Headset, Plus } from "@phosphor-icons/react";
import { Button, ContextMenu, cn, IconButton, Menu } from "@pl/ui";
import { AnimatePresence, type DragControls, motion, Reorder, useDragControls } from "motion/react";
import {
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { openMenuFromKey } from "@/features/social/menu-key";
import { enter, glide, leave, rise, useSocialMotion } from "@/features/social/motion";
import { SocialFace } from "@/features/social/social-face";
import type { SocialTeam } from "@/lib/api/social-types";
import type { VoiceCrewRoom } from "@/lib/api/voice-types";
import {
  joinCrewRoom,
  leaveRoom,
  openCrewRoom,
  orderCrewRooms,
  removeEmptyCrewRoom,
} from "./actions";
import { DraggableFace, useDropTarget } from "./crew-face-drag";
import { hasRoomPowers, RoomEditForm, RoomRemoveAsk, roomMenuItems } from "./crew-room-menu";
import { useCrewRoomsUi, withPendingOrder } from "./crew-rooms-store";
import {
  type CrewStanding,
  crewRoomList,
  crewRoomName,
  mayMovePerson,
  moveRoom,
  namesLine,
  type RoomPowers,
  roomPowers,
} from "./rules";
import { useFaces, useVoiceUi, voiceOn } from "./store";

/** Faces a row shows before "+N". */
const FACES = 5;

/** Whether a press starts on something of its own (a button, a face) rather than the row. */
const onControl = (target: EventTarget | null) =>
  target instanceof Element && target.closest("button, [data-voice-face]") !== null;

function Faces({
  team,
  room,
  rooms,
  crewMax,
  standing,
  manage,
}: {
  team: SocialTeam;
  room: VoiceCrewRoom;
  rooms: readonly VoiceCrewRoom[];
  crewMax: number;
  standing: CrewStanding;
  manage: boolean;
}) {
  const { t } = useTranslation();
  const faces = useFaces(room.people);
  if (!faces.length) return null;
  const shown = faces.slice(0, FACES);
  const more = faces.length - shown.length;
  return (
    <span className="flex min-w-0 items-center gap-1">
      {shown.map((face) => {
        const member = team.members.find((m) => m.id === face.id);
        // A face the player may move is a button (a press drags it, a click opens its menu).
        return member && mayMovePerson(standing, member, manage) ? (
          <DraggableFace
            key={face.id}
            team={team}
            face={face}
            member={member}
            room={room}
            rooms={rooms}
            crewMax={crewMax}
          />
        ) : (
          <span key={face.id} role="img" aria-label={face.name} className="shrink-0">
            <SocialFace
              source={face}
              size={24}
              className="overflow-hidden rounded-[var(--radius-head)]"
            />
          </span>
        );
      })}
      {more > 0 ? (
        <span
          className="pl-1 text-[12.5px] font-medium text-ash-300 tabular-nums"
          role="img"
          aria-label={namesLine(
            faces.slice(FACES).map((f) => f.name),
            t,
          )}
        >
          +{more}
        </span>
      ) : null}
    </span>
  );
}

interface RowProps {
  team: SocialTeam;
  room: VoiceCrewRoom;
  rooms: readonly VoiceCrewRoom[];
  here: boolean;
  powers: RoomPowers;
  standing: CrewStanding;
  manage: boolean;
  crewMax: number;
  /** The rooms' order ("Genel" aside), for the menu and Alt+↑/↓. */
  order: readonly string[];
  /** The row is being dragged into a new place. */
  lifted?: boolean;
  /** Starts a drag of the row (the leader's and moderators'). */
  dragControls?: DragControls;
}

type RowMode = "view" | "edit" | "remove";

function RoomRow({
  team,
  room,
  rooms,
  here,
  powers,
  standing,
  manage,
  crewMax,
  order,
  lifted = false,
  dragControls,
}: RowProps) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const drop = useDropTarget(room.id);
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<RowMode>("view");
  // A form that takes the row's place keeps the focus the closing menu would give back.
  const keepFocus = useRef(false);
  const shown = crewRoomName(room, t);
  const later = (next: RowMode) => {
    keepFocus.current = true;
    window.setTimeout(() => setMode(next), 0);
  };
  const items = roomMenuItems(
    {
      crew: team.id,
      room,
      here,
      powers,
      order,
      onEdit: () => later("edit"),
      onRemove: () => {
        if (room.people.length > 0) later("remove");
        else removeEmptyCrewRoom(team.id, room.id, shown);
      },
    },
    t,
  );
  const onCloseAutoFocus = (event: Event) => {
    if (!keepFocus.current) return;
    keepFocus.current = false;
    event.preventDefault();
  };
  // Rights taken away meanwhile (a role changed, the switch went off) close the form.
  useEffect(() => {
    if (mode === "edit" && !powers.edit) setMode("view");
    if (mode === "remove" && !powers.remove) setMode("view");
  }, [mode, powers.edit, powers.remove]);

  const more = hasRoomPowers(powers);
  const limited = room.max !== null && room.max < crewMax;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (mode !== "view") return;
    if (openMenuFromKey(event)) return;
    // Not with Ctrl: Windows reports AltGr as Ctrl+Alt (Turkish Q types with it).
    if (!powers.order || !event.altKey || event.ctrlKey) return;
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    const next = moveRoom(order, room.id, event.key === "ArrowUp" ? -1 : 1);
    if (next) void orderCrewRooms(team.id, next);
  };
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (!dragControls || mode !== "view" || event.button !== 0 || onControl(event.target)) return;
    dragControls.start(event);
  };

  return (
    <ContextMenu
      items={items}
      title={shown}
      disabled={mode !== "view"}
      onCloseAutoFocus={onCloseAutoFocus}
    >
      {/* biome-ignore lint/a11y/noStaticElementInteractions: the row's keys reach it from its buttons */}
      <div
        data-voice-room={room.id}
        onKeyDown={onKeyDown}
        onPointerDown={onPointerDown}
        aria-keyshortcuts={powers.order ? "Alt+ArrowUp Alt+ArrowDown" : undefined}
        className={cn(
          "group/room relative min-h-[44px] rounded-[var(--radius-panel)] py-1.5 pr-2 pl-2 transition-colors duration-200",
          here || mode !== "view"
            ? "bg-white/[0.06]"
            : "hover:bg-white/[0.045] has-[[data-state=open]]:bg-white/[0.06]",
          dragControls && mode === "view" && "cursor-grab touch-none active:cursor-grabbing",
        )}
      >
        {/* Lifted while dragged into a new place: its backing and shadow fade in. */}
        <span
          aria-hidden
          className={cn(
            "pointer-events-none absolute inset-0 rounded-[inherit] bg-obsidian-800/95 shadow-[0_14px_32px_rgb(34_6_6/0.5)] ring-1 ring-white/10 transition-opacity duration-150",
            lifted ? "opacity-100" : "opacity-0",
          )}
        />
        {/* A crewmate's face dragged over: the room that would take them lights up. */}
        <span
          aria-hidden
          className={cn(
            "pointer-events-none absolute inset-0 rounded-[inherit] bg-white/[0.07] ring-1 ring-bone-100/25 transition-opacity duration-150",
            drop ? "opacity-100" : "opacity-0",
          )}
        />
        <AnimatePresence mode="popLayout" initial={false}>
          {mode === "edit" ? (
            <motion.div key="edit" className="relative" {...rise(reduce, 3, 0.2)}>
              <RoomEditForm
                crew={team.id}
                crewMax={crewMax}
                room={room}
                onDone={() => setMode("view")}
              />
            </motion.div>
          ) : mode === "remove" ? (
            <motion.div key="remove" className="relative" {...rise(reduce, 3, 0.2)}>
              <RoomRemoveAsk crew={team.id} room={room} onDone={() => setMode("view")} />
            </motion.div>
          ) : (
            <motion.div
              key="view"
              className="relative flex min-h-8 items-center gap-2"
              {...rise(reduce, 3, 0.2)}
            >
              <span className="w-[112px] shrink-0 truncate text-[14px] font-semibold text-bone-50">
                {shown}
              </span>
              <span className="flex min-w-0 flex-1 items-center gap-2">
                <Faces
                  team={team}
                  room={room}
                  rooms={rooms}
                  crewMax={crewMax}
                  standing={standing}
                  manage={manage}
                />
                {limited ? (
                  <span
                    className="shrink-0 text-[12px] font-medium text-ash-400 tabular-nums"
                    role="img"
                    aria-label={t("voice.rooms.countLabel", {
                      count: room.people.length,
                      max: room.max,
                    })}
                  >
                    {t("voice.rooms.count", { count: room.people.length, max: room.max })}
                  </span>
                ) : null}
              </span>
              {here ? (
                <Button size="sm" variant="secondary" onClick={() => void leaveRoom()}>
                  {t("voice.crew.leave")}
                </Button>
              ) : (
                <Button
                  size="sm"
                  variant="secondary"
                  icon={<Headset size={15} />}
                  onClick={() => void joinCrewRoom(team.id, room.general ? null : room.id)}
                >
                  {t("voice.crew.join")}
                </Button>
              )}
              {more ? (
                <Menu
                  open={open}
                  onOpenChange={setOpen}
                  items={items}
                  title={shown}
                  besideWhenNoRoom
                  onCloseAutoFocus={onCloseAutoFocus}
                  trigger={
                    <IconButton
                      size="sm"
                      label={t("voice.rooms.more")}
                      icon={<DotsThree size={18} weight="bold" />}
                      className={cn(
                        "transition-opacity duration-150",
                        open
                          ? "opacity-100"
                          : "opacity-0 group-hover/room:opacity-100 group-focus-within/room:opacity-100 focus-visible:opacity-100",
                      )}
                    />
                  }
                />
              ) : (
                // The place of a row's "⋯", as the members' rows keep it.
                <span aria-hidden className="size-8 shrink-0" />
              )}
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </ContextMenu>
  );
}

/** One of the crew's rooms that the leader and moderators drag into a new place. */
function OrderedRoom({
  id,
  onDragEnd,
  children,
}: {
  id: string;
  onDragEnd: () => void;
  children: (controls: DragControls, lifted: boolean) => ReactNode;
}) {
  const controls = useDragControls();
  const calm = useCalm();
  const { reduce, layout } = useSocialMotion();
  const [lifted, setLifted] = useState(false);
  return (
    <Reorder.Item
      as="li"
      value={id}
      layout="position"
      drag="y"
      dragListener={false}
      dragControls={controls}
      dragElastic={0.08}
      onDragStart={() => setLifted(true)}
      onDragEnd={() => {
        setLifted(false);
        onDragEnd();
      }}
      // Performance mode and less motion: the others take their new places at once.
      transition={{ layout: layout ? glide : { duration: 0 } }}
      className="relative"
      whileDrag={calm({ scale: 1.015 })}
      initial={{ opacity: 0, y: reduce ? 0 : 4 }}
      animate={{ opacity: 1, y: 0, transition: enter(0.24) }}
      exit={{ opacity: 0, transition: leave(0.2) }}
    >
      {children(controls, lifted)}
    </Reorder.Item>
  );
}

export function CrewVoice({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const on = useVoiceUi((s) => voiceOn(s.state));
  const entry = useVoiceUi((s) => s.state.crews.find((c) => c.crew === team.id));
  const max = useVoiceUi((s) => s.state.crewRoomsMax);
  const here = useVoiceUi((s) => (s.state.room?.crew === team.id ? s.state.room.id : null));
  const hidden = useCrewRoomsUi((s) => s.hidden);
  const pending = useCrewRoomsUi((s) => s.order[team.id]);
  const setPending = useCrewRoomsUi((s) => s.setOrder);
  // While a row is dragged its order lives here; the order the player gave takes over after.
  const [dragOrder, setDragOrder] = useState<string[] | null>(null);
  const dragging = useRef(false);
  const latest = useRef<string[] | null>(null);

  const rooms = crewRoomList(team.id, entry, hidden);
  const [general, ...rest] = rooms;
  const saved = rest.map((room) => room.id);
  const savedKey = saved.join();
  // The order the player gave goes once the service's list says it too.
  useEffect(() => {
    if (pending && pending.join() === savedKey) setPending(team.id, null);
  }, [pending, savedKey, setPending, team.id]);
  const others = withPendingOrder(rest, dragOrder ?? pending);
  const order = others.map((room) => room.id);

  if (!on || !general) return null;
  const manage = entry?.manage ?? false;
  const crewMax = entry?.crewMax ?? 15;
  const standing: CrewStanding = { role: team.role, moderator: team.moderator };
  const canAdd = entry ? entry.canAdd : max > 1;
  const reorders = others.some((room) => roomPowers(standing, room, manage).order);

  const row = (room: VoiceCrewRoom, extra: Partial<RowProps> = {}) => (
    <RoomRow
      team={team}
      room={room}
      rooms={rooms}
      here={room.id === here}
      powers={roomPowers(standing, room, manage)}
      standing={standing}
      manage={manage}
      crewMax={crewMax}
      order={order}
      {...extra}
    />
  );

  const endDrag = () => {
    const next = dragging.current ? latest.current : null;
    dragging.current = false;
    latest.current = null;
    setDragOrder(null);
    if (next && next.join() !== (pending ?? saved).join()) void orderCrewRooms(team.id, next);
  };

  return (
    <section className="mt-5" aria-labelledby={`voice-${team.id}`}>
      <div className="mb-1.5 flex min-h-8 items-center gap-2 pr-2 pl-2">
        <h3
          id={`voice-${team.id}`}
          className="min-w-0 flex-1 text-[13px] font-semibold text-ash-300"
        >
          {t("voice.crew.title")}
        </h3>
        {canAdd ? (
          <Button
            size="sm"
            variant="secondary"
            icon={<Plus size={15} />}
            onClick={() => void openCrewRoom(team.id)}
          >
            {t("voice.crew.newRoom")}
          </Button>
        ) : null}
        <span aria-hidden className="size-8 shrink-0" />
      </div>
      <ul className="flex flex-col gap-0.5">
        <li>{row(general)}</li>
      </ul>
      {reorders ? (
        <Reorder.Group
          as="ul"
          axis="y"
          values={order}
          onReorder={(next: string[]) => {
            dragging.current = true;
            latest.current = next;
            setDragOrder(next);
          }}
          className="mt-0.5 flex flex-col gap-0.5"
        >
          <AnimatePresence initial={false}>
            {others.map((room) => (
              <OrderedRoom key={room.id} id={room.id} onDragEnd={endDrag}>
                {(controls, lifted) =>
                  row(room, {
                    lifted,
                    dragControls: roomPowers(standing, room, manage).order ? controls : undefined,
                  })
                }
              </OrderedRoom>
            ))}
          </AnimatePresence>
        </Reorder.Group>
      ) : (
        <ul className="mt-0.5 flex flex-col gap-0.5">
          <AnimatePresence initial={false} mode="popLayout">
            {others.map((room) => (
              <motion.li
                key={room.id}
                layout={layout ? "position" : false}
                transition={{ layout: glide }}
                initial={{ opacity: 0, y: reduce ? 0 : 4 }}
                animate={{ opacity: 1, y: 0, transition: enter(0.24) }}
                exit={{ opacity: 0, transition: leave(0.2) }}
              >
                {row(room)}
              </motion.li>
            ))}
          </AnimatePresence>
        </ul>
      )}
    </section>
  );
}
