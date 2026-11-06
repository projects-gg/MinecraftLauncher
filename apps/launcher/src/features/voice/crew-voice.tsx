// The crew page's "Sesli" (VOICE-L3, VOICE-ROOMS; voice.md §5.1): the crew's rooms, "Genel" always
// first, who is in each and "Katıl"; the player's own room sits on the quiet plate with "Ayrıl".
// "Yeni oda" while the crew may open one more. Written on the crew page's grid (CREW-POLISH): no
// box of its own; the heading and every row's words start where the crew card's do (14 px in from
// the plates' edge), and every button ends where "Ekibi çağır", "Plan yap" and the members' "⋯"
// end (14 px in). A room's faces follow its name, then "+N" when the row is full, then a limit's
// "2/4"; the room's "⋯" (only where the player may change it) sits before its button.
//
// While the crew arranges its rooms (Studio's "Oda yönetimi", `manage`): a right click on a room
// (Shift+F10, the menu key) or its "⋯" (shown under the pointer and the focus) opens the room's
// menu by role; editing and the question before deleting a room with people in it take the row's
// place (one row at a time; the focus comes back to the row, or to the next one when it went). The
// leader and moderators drag rooms into a new order (Alt+↑/↓ from the keyboard) and crewmates'
// faces from one room to another (a face's menu says "Taşı" too). Rows that come and go fade where
// they are; the others and the sections below glide; a dragged row lifts (transform and opacity
// only; fades only with less motion).
import { DotsThree, Headset, Plus } from "@phosphor-icons/react";
import { Button, ContextMenu, cn, IconButton, Menu, OverflowTooltip, Tooltip } from "@pl/ui";
import { AnimatePresence, type DragControls, motion, Reorder, useDragControls } from "motion/react";
import {
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { useCrewRoomTargets } from "@/features/share/live";
import { LiveAppear, LiveTagButton } from "@/features/share/live-card";
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
import { DraggableFace, useDropTarget, useFaceDrag } from "./crew-face-drag";
import { hasRoomPowers, RoomEditForm, RoomRemoveAsk, roomMenuItems } from "./crew-room-menu";
import { useCrewRoomsUi, withPendingOrder } from "./crew-rooms-store";
import { DiscordInRoom } from "./discord-offer";
import {
  type CrewStanding,
  crewRoomFull,
  crewRoomList,
  crewRoomName,
  facesThatFit,
  mayMovePerson,
  moveRoom,
  namesLine,
  type RoomPowers,
  roomPowers,
} from "./rules";
import { useFaces, useVoiceUi, voiceOn } from "./store";

/** Whether a press starts on something of its own (a button, a face) rather than the row. */
const onControl = (target: EventTarget | null) =>
  target instanceof Element && target.closest("button, [data-voice-face], [data-no-drag]") !== null;

/** A room row's "⋯", else its first button: where the keyboard comes back to in a row. */
function focusRow(room: string) {
  const row = document.querySelector<HTMLElement>(`[data-voice-room="${room}"]`);
  const target =
    row?.querySelector<HTMLElement>("[data-room-more]") ??
    row?.querySelector<HTMLElement>("button");
  target?.focus();
}

/** A ref for an element and its width, again whenever it changes (nothing is watched in between);
 * null while there is none or it is hidden. A callback ref: it follows an element that comes later. */
function useWidthOf(): [(node: HTMLElement | null) => void, number | null] {
  const [width, setWidth] = useState<number | null>(null);
  const watch = useRef<ResizeObserver | null>(null);
  const ref = useCallback((node: HTMLElement | null) => {
    watch.current?.disconnect();
    watch.current = null;
    if (!node) {
      setWidth(null);
      return;
    }
    // The box's own width: a lifted row's scale does not change what fits.
    const read = () => setWidth(node.clientWidth || null);
    read();
    if (typeof ResizeObserver === "undefined") return;
    watch.current = new ResizeObserver(read);
    watch.current.observe(node);
  }, []);
  return [ref, width];
}

/** The flex gap between the last face (or "+N") and a limit's count. */
const COUNT_GAP = 4;

/** A room's live card opens under its "YAYINDA", as a crew tag's card does under the tag. */
const LIVE_CARD = { side: "bottom", align: "start", sideOffset: 6 } as const;

function Faces({
  team,
  room,
  rooms,
  crewMax,
  standing,
  manage,
  count,
}: {
  team: SocialTeam;
  room: VoiceCrewRoom;
  rooms: readonly VoiceCrewRoom[];
  crewMax: number;
  standing: CrewStanding;
  manage: boolean;
  /** A limit's "2/4", right after the faces (CREW-POLISH: at the row's end it floated alone). */
  count?: ReactNode;
}) {
  const { t } = useTranslation();
  const faces = useFaces(room.people);
  const [box, width] = useWidthOf();
  const [tail, tailWidth] = useWidthOf();
  // The faces take the row's free width less the count's, so "2/4" always fits after them.
  const free =
    width === null ? null : Math.max(0, width - (count ? (tailWidth ?? 0) + COUNT_GAP : 0));
  const shown = faces.slice(0, facesThatFit(faces.length, free));
  const more = faces.length - shown.length;
  // There when empty too: it takes the row's free width (the buttons keep their place).
  return (
    <span ref={box} className="flex min-w-0 flex-1 items-center gap-1">
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
          // A flex box: a picture and a letter face sit on one line (inline, a picture rode its
          // baseline 3 px above the letter's tile).
          <span key={face.id} role="img" aria-label={face.name} className="flex shrink-0">
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
          className="shrink-0 pl-1 text-[12.5px] font-medium text-ash-300 tabular-nums"
          role="img"
          aria-label={namesLine(
            faces.slice(shown.length).map((f) => f.name),
            t,
          )}
        >
          +{more}
        </span>
      ) : null}
      {count ? (
        <span ref={tail} className="flex shrink-0 pl-1">
          {count}
        </span>
      ) : null}
    </span>
  );
}

type RowMode = "view" | "edit" | "remove";

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
  /** The room, its form or the question before deleting it (one row at a time, the list's). */
  mode: RowMode;
  onMode: (mode: RowMode) => void;
  /** The row is being dragged into a new place. */
  lifted?: boolean;
  /** Starts a drag of the row (the leader's and moderators'). */
  dragControls?: DragControls;
}

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
  mode,
  onMode,
  lifted = false,
  dragControls,
}: RowProps) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const drop = useDropTarget(room.id);
  // A face dragged out of this row keeps the pointer's hover: only the room it would go to lights.
  const faceDrag = useFaceDrag((s) => s.drag !== null);
  const [open, setOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);
  const nameRef = useRef<HTMLSpanElement>(null);
  // A form that takes the row's place keeps the focus the closing menu would give back.
  const keepFocus = useRef(false);
  // The form closed with the focus in it (or nowhere): the focus comes back to the row's "⋯".
  const refocus = useRef(false);
  const shown = crewRoomName(room, t);
  const later = (next: RowMode) => {
    keepFocus.current = true;
    window.setTimeout(() => onMode(next), 0);
  };
  // The room after this one (else before it, else "Genel") takes the focus when this one goes.
  const neighbour = () => {
    const at = order.indexOf(room.id);
    return order[at + 1] ?? order[at - 1] ?? rooms[0]?.id ?? null;
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
        if (room.people.length > 0) {
          later("remove");
          return;
        }
        removeEmptyCrewRoom(team.id, room.id, shown);
        const next = neighbour();
        keepFocus.current = true;
        if (next) window.setTimeout(() => focusRow(next), 0);
      },
    },
    t,
  );
  const onCloseAutoFocus = (event: Event) => {
    if (!keepFocus.current) return;
    keepFocus.current = false;
    event.preventDefault();
  };
  const close = () => {
    const active = document.activeElement;
    refocus.current =
      active === null || active === document.body || (rowRef.current?.contains(active) ?? false);
    onMode("view");
  };
  const removed = (gone: boolean) => {
    if (!gone) {
      close();
      return;
    }
    onMode("view");
    const next = neighbour();
    if (next) focusRow(next);
  };
  useEffect(() => {
    if (mode !== "view" || !refocus.current) return;
    refocus.current = false;
    focusRow(room.id);
  }, [mode, room.id]);
  // Rights taken away meanwhile (a role changed, the switch went off) close the form.
  useEffect(() => {
    if ((mode === "edit" && !powers.edit) || (mode === "remove" && !powers.remove)) onMode("view");
  }, [mode, powers.edit, powers.remove, onMode]);

  const more = hasRoomPowers(powers);
  const limited = room.max !== null && room.max < crewMax;
  const full = !here && crewRoomFull(room, crewMax);
  const live = useCrewRoomTargets(team.id, room, crewMax);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (mode !== "view") return;
    if (openMenuFromKey(event)) return;
    // Not with Ctrl: Windows reports AltGr as Ctrl+Alt (Turkish Q types with it).
    if (!powers.order || !event.altKey || event.ctrlKey) return;
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    const next = moveRoom(order, room.id, event.key === "ArrowUp" ? -1 : 1);
    if (!next) return;
    // The row's node moves in the page and the browser drops its focus: it comes back.
    const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void orderCrewRooms(team.id, next);
    window.requestAnimationFrame(() => {
      if (active?.isConnected && document.activeElement !== active) active.focus();
    });
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
        ref={rowRef}
        data-voice-room={room.id}
        onKeyDown={onKeyDown}
        onPointerDown={onPointerDown}
        className={cn(
          "group/room relative min-h-[44px] rounded-[var(--radius-panel)] px-4 py-1.5 transition-colors duration-200",
          // A closing form fades inside the row, not over the next one (a lifted row casts its shadow).
          !lifted && "overflow-hidden",
          here || mode !== "view"
            ? "bg-white/[0.06]"
            : faceDrag
              ? undefined
              : "hover:bg-white/[0.045] data-[state=open]:bg-white/[0.06] has-[[data-state=open]]:bg-white/[0.06]",
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
              <RoomEditForm crew={team.id} crewMax={crewMax} room={room} onDone={close} />
            </motion.div>
          ) : mode === "remove" ? (
            <motion.div
              key="remove"
              role="group"
              aria-label={shown}
              className="relative"
              {...rise(reduce, 3, 0.2)}
            >
              <RoomRemoveAsk crew={team.id} room={room} onDone={removed} />
            </motion.div>
          ) : (
            <motion.div
              key="view"
              className="relative flex min-h-8 items-center gap-2.5"
              {...rise(reduce, 3, 0.2)}
            >
              {/* A long name takes up to half the row (the faces make way), cut beyond it; the
                  faces follow it (CREW-POLISH: a fixed column left them far from short names). */}
              <OverflowTooltip text={shown} of={nameRef}>
                <span
                  ref={nameRef}
                  className="max-w-[50%] shrink-0 truncate text-[14px] font-semibold text-bone-50"
                >
                  {shown}
                </span>
              </OverflowTooltip>
              {/* SHARE-LIVE: someone shares in it (seen from outside too): its card under the tag. */}
              <LiveAppear show={live.length > 0}>
                <LiveTagButton targets={live} placement={LIVE_CARD} />
              </LiveAppear>
              <Faces
                team={team}
                room={room}
                rooms={rooms}
                crewMax={crewMax}
                standing={standing}
                manage={manage}
                count={
                  limited ? (
                    <span
                      className="font-display text-[12px] font-semibold text-ash-400 tabular-nums"
                      role="img"
                      aria-label={t("voice.rooms.countLabel", {
                        count: room.people.length,
                        max: room.max,
                      })}
                    >
                      {t("voice.rooms.count", { count: room.people.length, max: room.max })}
                    </span>
                  ) : null
                }
              />
              {/* The room's "⋯" before its button: the button keeps the page's one right edge,
                  where "Ekibi çağır" and "Plan yap" end, whether or not a row has a menu. */}
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
                      data-room-more
                      label={t("voice.rooms.more", { room: shown })}
                      icon={<DotsThree size={18} weight="bold" />}
                      className={cn(
                        "transition-opacity duration-150",
                        open
                          ? "opacity-100"
                          : faceDrag
                            ? "opacity-0"
                            : "opacity-0 group-hover/room:opacity-100 group-focus-within/room:opacity-100 focus-visible:opacity-100",
                      )}
                    />
                  }
                />
              ) : null}
              {here ? (
                <Button size="sm" variant="secondary" onClick={() => void leaveRoom()}>
                  {t("voice.crew.leave")}
                </Button>
              ) : (
                // A full room says so where it acts (the service would refuse the join).
                <Tooltip content={t("voice.refused.roomFull")} disabled={!full}>
                  {/* A press on the closed button does not drag the row. */}
                  <span className="inline-flex" data-no-drag>
                    <Button
                      size="sm"
                      variant="secondary"
                      icon={<Headset size={15} />}
                      disabled={full}
                      onClick={() => void joinCrewRoom(team.id, room.general ? null : room.id)}
                    >
                      {t("voice.crew.join")}
                    </Button>
                  </span>
                </Tooltip>
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
  // The one row showing its form or its question (here, so the rows below glide as it opens).
  const [editing, setEditing] = useState<{ room: string; mode: "edit" | "remove" } | null>(null);

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
      mode={editing?.room === room.id ? editing.mode : "view"}
      onMode={(next) => setEditing(next === "view" ? null : { room: room.id, mode: next })}
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

  // One list ("Genel" first, never dragged): a screen reader hears all the rooms together.
  const generalRow = <li>{row(general)}</li>;
  return (
    <section className="mt-6" aria-labelledby={`voice-${team.id}`}>
      {/* A small gap under the heading: "Yeni oda" no longer sits on the first room's plate. */}
      <div className="mb-1.5 flex min-h-8 items-center gap-2 px-4">
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
      </div>
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
          className="flex flex-col gap-1"
        >
          {generalRow}
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
        // 4 px between rooms: the player's room plate and a hovered neighbour read as two.
        <ul className="flex flex-col gap-1">
          {generalRow}
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
      {/* Discord's keys, asked once (VOICE-KEYS), under the rooms while the player is in one of
          them: as a toast it lay over the members until answered (CREW-POLISH). */}
      <DiscordInRoom active={here !== null} />
    </section>
  );
}
