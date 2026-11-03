// VOICE-ROOMS: a leader or moderator drags a crewmate's face from one room of the crew page to
// another (Discord's and TeamSpeak's move, with the room's "Taşı" rule: leaders anyone, moderators
// members only). Past a few pixels the face lifts and follows the pointer (a fixed layer moved by
// `transform` once a frame: nothing re-renders while it moves), the room under it lights its drop
// plate (opacity), and letting go over another room moves them; Esc, a room that is full or the
// room they are in sends the face back. A click opens the face's menu instead ("Taşı" first, then
// the crewmate's own rows): the keyboard's way.
import { ArrowsLeftRight } from "@phosphor-icons/react";
import { cn, Menu, type MenuItem, useReduceMotion } from "@pl/ui";
import { motion } from "motion/react";
import { type PointerEvent, type Ref, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { isMenuKey } from "@/features/social/menu-key";
import { usePersonItems } from "@/features/social/person-menu";
import { SocialFace } from "@/features/social/social-face";
import type { SocialTeam, TeamMember } from "@/lib/api/social-types";
import type { VoiceCrewRoom } from "@/lib/api/voice-types";
import { moveCrewmate } from "./actions";
import { crewRoomFull, crewRoomName, type VoiceFace } from "./rules";

/** How far the pointer goes before a press becomes a drag (a click stays a click). */
const DRAG_PX = 5;

interface FaceDrag {
  person: string;
  from: string;
  /** The room under the pointer that may take them, or none. */
  target: string | null;
}

interface FaceDragUi {
  drag: FaceDrag | null;
  set: (drag: FaceDrag | null) => void;
  aim: (target: string | null) => void;
}

/** The drag in progress (rows read `target` to light their drop plate). */
export const useFaceDrag = create<FaceDragUi>()((set) => ({
  drag: null,
  set: (drag) => set({ drag }),
  aim: (target) =>
    set((s) => (s.drag === null || s.drag.target === target ? s : { drag: { ...s.drag, target } })),
}));

/** Whether a face dragged now would land in `room` (its row lights its plate). */
export function useDropTarget(room: string): boolean {
  return useFaceDrag((s) => s.drag?.target === room);
}

/** The room a point is over (rows carry `data-voice-room`), or none. */
function roomAt(x: number, y: number): string | null {
  const element = document.elementFromPoint(x, y);
  return element?.closest<HTMLElement>("[data-voice-room]")?.dataset.voiceRoom ?? null;
}

/**
 * A crewmate's face that the player may move: a press drags it, a click (a right click, Enter)
 * opens its menu.
 */
export function DraggableFace({
  team,
  face,
  member,
  room,
  rooms,
  crewMax,
  size = 24,
}: {
  team: SocialTeam;
  face: VoiceFace;
  member: TeamMember;
  room: VoiceCrewRoom;
  /** The crew's rooms as listed (the menu's "Taşı" names the others). */
  rooms: readonly VoiceCrewRoom[];
  crewMax: number;
  size?: number;
}) {
  const { t } = useTranslation();
  const reduce = useReduceMotion();
  const [open, setOpen] = useState(false);
  // Where the face was when it lifted (the layer starts there), while it is dragged.
  const [lift, setLift] = useState<DOMRect | null>(null);
  const press = useRef<{ x: number; y: number; id: number; rect: DOMRect } | null>(null);
  const ghost = useRef<HTMLDivElement | null>(null);
  const frame = useRef(0);
  const dragging = useRef(false);
  const begin = useFaceDrag((s) => s.set);
  const aim = useFaceDrag((s) => s.aim);

  const others = rooms.filter((r) => r.id !== room.id);
  const takes = (id: string | null) => {
    const target = id === null ? undefined : others.find((r) => r.id === id);
    return target !== undefined && !crewRoomFull(target, crewMax) ? target.id : null;
  };
  const items = usePersonItems(member, { crew: team, from: "team" });
  const moveItems: MenuItem[] = others.map((other) => ({
    label: crewRoomName(other, t),
    disabled: crewRoomFull(other, crewMax),
    onSelect: () => void moveCrewmate(team.id, member.id, other.id),
  }));
  // The face's own action first, then the crewmate's rows (the same everywhere, FIX2).
  const menu: Array<MenuItem | null> = [
    {
      label: t("voice.rooms.move"),
      icon: <ArrowsLeftRight size={16} />,
      items: moveItems,
      onSelect: () => {},
    },
  ];
  if (items.length) menu.push(null, ...items);

  const finish = (drop: boolean, x = 0, y = 0) => {
    cancelAnimationFrame(frame.current);
    const target = drop ? takes(roomAt(x, y)) : null;
    dragging.current = false;
    press.current = null;
    begin(null);
    setLift(null);
    if (target) void moveCrewmate(team.id, member.id, target);
  };
  const finishRef = useRef(finish);
  finishRef.current = finish;

  // Esc sends a dragged face back (the press took no focus: the menu opens on the click).
  useEffect(() => {
    if (!lift) return;
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      finishRef.current(false);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [lift]);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  const follow = (x: number, y: number) => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => {
      const layer = ghost.current;
      if (!layer) return;
      layer.style.transform = `translate3d(${x - size / 2}px, ${y - size / 2}px, 0)`;
    });
  };

  const onPointerDown = (event: PointerEvent<HTMLButtonElement>) => {
    // The menu opens on the click (a press may become a drag); Radix would open it on the press.
    event.preventDefault();
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    press.current = {
      x: event.clientX,
      y: event.clientY,
      id: event.pointerId,
      rect: event.currentTarget.getBoundingClientRect(),
    };
  };
  const onPointerMove = (event: PointerEvent<HTMLButtonElement>) => {
    const start = press.current;
    if (!start || start.id !== event.pointerId) return;
    if (!dragging.current) {
      if (Math.hypot(event.clientX - start.x, event.clientY - start.y) < DRAG_PX) return;
      dragging.current = true;
      setLift(start.rect);
      begin({ person: member.id, from: room.id, target: null });
    }
    follow(event.clientX, event.clientY);
    aim(takes(roomAt(event.clientX, event.clientY)));
  };
  const onPointerUp = (event: PointerEvent<HTMLButtonElement>) => {
    const start = press.current;
    if (!start || start.id !== event.pointerId) return;
    if (dragging.current) {
      finish(true, event.clientX, event.clientY);
      return;
    }
    press.current = null;
    setOpen(true);
  };

  return (
    <>
      <Menu
        open={open}
        onOpenChange={setOpen}
        title={face.name}
        items={menu}
        align="start"
        trigger={
          <button
            type="button"
            data-voice-face={member.id}
            aria-label={face.name}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onLostPointerCapture={() => {
              if (dragging.current) finish(false);
            }}
            onContextMenu={(event) => {
              // The face's own menu, not the room's around it.
              event.preventDefault();
              event.stopPropagation();
              setOpen(true);
            }}
            onKeyDown={(event) => {
              // Shift+F10 or the menu key: the face's menu too (the row would open the room's).
              if (!isMenuKey(event)) return;
              event.preventDefault();
              event.stopPropagation();
              setOpen(true);
            }}
            className={cn(
              "relative shrink-0 cursor-grab touch-none rounded-[var(--radius-head)] outline-none select-none focus-visible:ring-2 focus-visible:ring-gold-300 active:cursor-grabbing",
              "transition-opacity duration-150",
              lift && "opacity-35",
            )}
          >
            <SocialFace
              source={face}
              size={size}
              className="overflow-hidden rounded-[var(--radius-head)]"
            />
          </button>
        }
      />
      {lift
        ? createPortal(
            <FaceGhost
              ref={ghost}
              face={face}
              size={size}
              at={lift}
              reduce={reduce}
              label={t("voice.rooms.dragging", { name: face.name })}
            />,
            document.body,
          )
        : null}
    </>
  );
}

/** The face that follows the pointer: lifted a touch, its backing and shadow faded in. */
function FaceGhost({
  ref,
  face,
  size,
  at,
  reduce,
  label,
}: {
  ref: Ref<HTMLDivElement>;
  face: VoiceFace;
  size: number;
  at: DOMRect;
  reduce: boolean;
  label: string;
}) {
  return (
    <div
      ref={ref}
      role="status"
      aria-label={label}
      className="pointer-events-none fixed top-0 left-0 z-[70] will-change-transform"
      style={{ transform: `translate3d(${at.left}px, ${at.top}px, 0)` }}
    >
      <motion.span
        className="relative block"
        initial={{ opacity: 0, transform: reduce ? "none" : "scale(1)" }}
        animate={{ opacity: 1, transform: reduce ? "none" : "scale(1.12)" }}
        transition={{ duration: 0.14, ease: [0.16, 1, 0.3, 1] }}
      >
        <span
          aria-hidden
          className="absolute -inset-1 rounded-[var(--radius-head)] bg-obsidian-950/60 shadow-[0_10px_24px_rgb(34_6_6/0.55)]"
        />
        <SocialFace
          source={face}
          size={size}
          className="relative overflow-hidden rounded-[var(--radius-head)] ring-2 ring-bone-100/70"
        />
      </motion.span>
    </div>
  );
}
