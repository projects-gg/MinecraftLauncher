// Dragging in the friend list: a friend into another category, or a category to another place.
// The pressed row lifts into a ghost that follows the pointer (a fixed layer, so the list can
// scroll under it near the panel's edges); its place in the list becomes a slot that moves to
// where it would land while the rows around it glide aside. On release the ghost settles into
// the slot on a calm spring and the row is simply there; Escape puts it back. Menus and Alt+↑/↓
// do the same without a pointer (WCAG 2.5.7). Reduced motion: the ghost fades out in place.
import { animate, useMotionValue } from "motion/react";
import {
  type PointerEvent as ReactPointerEvent,
  type RefObject,
  useEffect,
  useRef,
  useState,
} from "react";
import { dragOrder, dropTarget } from "./groups";

/** Pointer travel before a press becomes a drag (a click stays a click). */
const THRESHOLD = 6;
/** How close to the scroller's edge the list starts scrolling, and how fast at most (px/frame). */
const EDGE = 44;
const MAX_SCROLL = 14;
/** Lifting, and settling into the slot: a calm spring, a hair of overshoot. */
const LIFT = { type: "spring", stiffness: 520, damping: 34 } as const;
const SETTLE = { type: "spring", stiffness: 500, damping: 38, mass: 0.9 } as const;

export type DragSpec =
  | { kind: "friend"; id: string; from: string }
  | { kind: "group"; id: string; order: string[] };

interface Box {
  width: number;
  height: number;
  left: number;
  /** The drop has happened; the ghost is settling. */
  settling: boolean;
}

export type ListDrag =
  | ({ kind: "friend"; id: string; from: string; over: string } & Box)
  | ({ kind: "group"; id: string; from: string[]; order: string[] } & Box);

interface Options {
  listRef: RefObject<HTMLElement | null>;
  reduce: boolean;
  onDropFriend: (id: string, group: string) => void;
  onDropGroup: (id: string, order: string[]) => void;
}

export function useListDrag({ listRef, reduce, onDropFriend, onDropGroup }: Options) {
  const [drag, setDrag] = useState<ListDrag | null>(null);
  const current = useRef<ListDrag | null>(null);
  const y = useMotionValue(0);
  const scale = useMotionValue(1);
  const opacity = useMotionValue(1);
  const pending = useRef<{ spec: DragSpec; x: number; y: number; element: HTMLElement } | null>(
    null,
  );
  const pointer = useRef({ y: 0, direction: 1 as 1 | -1 });
  const grab = useRef(0);
  const frame = useRef<number | null>(null);
  const suppressClick = useRef(false);
  // The latest callbacks and preferences, for the window listeners set up once.
  const live = useRef({ reduce, onDropFriend, onDropGroup });
  live.current = { reduce, onDropFriend, onDropGroup };

  useEffect(() => {
    const set = (next: ListDrag | null) => {
      current.current = next;
      setDrag(next);
    };
    const list = () => listRef.current;

    const hitTest = () => {
      const element = list();
      const d = current.current;
      if (!element || !d || d.settling) return;
      const top = element.getBoundingClientRect().top;
      const headers = [...element.querySelectorAll<HTMLElement>("[data-group-header]")];
      if (d.kind === "friend") {
        const target =
          dropTarget(
            pointer.current.y - top,
            headers.map((header) => ({
              id: header.dataset.groupHeader ?? "",
              top: header.offsetTop,
            })),
          ) ?? headers[0]?.dataset.groupHeader;
        if (target && target !== d.over) set({ ...d, over: target });
        return;
      }
      const blocks = new Map<string, { top: number; bottom: number }>();
      headers.forEach((header, index) => {
        const id = header.dataset.groupHeader ?? "";
        const next = headers[index + 1];
        blocks.set(id, {
          top: header.offsetTop,
          bottom: next ? next.offsetTop : element.scrollHeight,
        });
      });
      // The middle of the lifted header, in the list's own coordinates.
      const middle = pointer.current.y - grab.current + d.height / 2 - top;
      const order = dragOrder(d.order, d.id, middle, pointer.current.direction, blocks);
      if (order.join("\n") !== d.order.join("\n")) set({ ...d, order });
    };

    const scroller = () => list()?.closest<HTMLElement>(".overflow-y-auto") ?? null;

    const autoScroll = () => {
      const box = scroller();
      if (!box) return;
      const rect = box.getBoundingClientRect();
      const at = pointer.current.y;
      const speed = (depth: number) => Math.ceil((Math.min(depth, EDGE) / EDGE) * MAX_SCROLL);
      if (at < rect.top + EDGE) box.scrollTop -= speed(rect.top + EDGE - at);
      else if (at > rect.bottom - EDGE) box.scrollTop += speed(at - (rect.bottom - EDGE));
    };

    const loop = () => {
      frame.current = null;
      const d = current.current;
      if (!d || d.settling) return;
      autoScroll();
      hitTest();
      frame.current = requestAnimationFrame(loop);
    };

    const begin = (event: PointerEvent) => {
      const press = pending.current;
      if (!press) return;
      pending.current = null;
      const rect = press.element.getBoundingClientRect();
      grab.current = press.y - rect.top;
      y.set(event.clientY - grab.current);
      opacity.set(1);
      scale.set(1);
      if (!live.current.reduce) void animate(scale, 1.02, LIFT);
      suppressClick.current = true;
      document.body.style.userSelect = "none";
      document.body.style.cursor = "grabbing";
      const box = { width: rect.width, height: rect.height, left: rect.left, settling: false };
      set(
        press.spec.kind === "friend"
          ? { ...press.spec, over: press.spec.from, ...box }
          : {
              kind: "group",
              id: press.spec.id,
              from: press.spec.order,
              order: press.spec.order,
              ...box,
            },
      );
      frame.current = requestAnimationFrame(loop);
    };

    const settle = () => {
      const element = list();
      const d = current.current;
      if (!d) return;
      const done = () => {
        set(null);
        document.body.style.userSelect = "";
        document.body.style.cursor = "";
      };
      if (live.current.reduce) {
        void animate(opacity, 0, { duration: 0.12 }).then(done);
        return;
      }
      const slot = element?.querySelector<HTMLElement>("[data-drag-slot]");
      if (!element || !slot) {
        void animate(opacity, 0, { duration: 0.14 }).then(done);
        return;
      }
      // The slot's place once the rows have glided (its layout box, not where it is mid-glide).
      const target = element.getBoundingClientRect().top + slot.offsetTop;
      if (slot.dataset.folded === "1") {
        // Into a folded category: the ghost sinks into its header.
        void animate(y, target - d.height * 0.6, SETTLE);
        void animate(scale, 0.96, { duration: 0.18 });
        void animate(opacity, 0, { duration: 0.18 }).then(done);
        return;
      }
      void animate(scale, 1, SETTLE);
      void animate(y, target, SETTLE).then(done);
    };

    const finish = (commit: boolean) => {
      const d = current.current;
      if (!d || d.settling) return;
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      frame.current = null;
      let next: ListDrag = d;
      if (d.kind === "friend") {
        if (!commit) next = { ...d, over: d.from };
        else if (d.over !== d.from) live.current.onDropFriend(d.id, d.over);
      } else if (!commit) {
        next = { ...d, order: d.from };
      } else if (d.order.join("\n") !== d.from.join("\n")) {
        live.current.onDropGroup(d.id, d.order);
      }
      set({ ...next, settling: true });
      // Two frames: the slot is in its final place in the layout by then.
      requestAnimationFrame(() => requestAnimationFrame(settle));
    };

    const onMove = (event: PointerEvent) => {
      if (!current.current) {
        const press = pending.current;
        if (!press) return;
        if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < THRESHOLD) return;
        pointer.current.y = event.clientY;
        begin(event);
        return;
      }
      if (current.current.settling) return;
      event.preventDefault();
      const delta = event.clientY - pointer.current.y;
      if (delta !== 0) pointer.current.direction = delta > 0 ? 1 : -1;
      pointer.current.y = event.clientY;
      y.set(event.clientY - grab.current);
    };

    const onUp = () => {
      pending.current = null;
      if (current.current) finish(true);
      // A click right after a drag belongs to the drag; nothing else may take it.
      window.setTimeout(() => {
        suppressClick.current = false;
      }, 0);
    };

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && current.current && !current.current.settling) {
        event.preventDefault();
        event.stopPropagation();
        finish(false);
      }
    };

    const onClick = (event: MouseEvent) => {
      if (!suppressClick.current) return;
      suppressClick.current = false;
      event.preventDefault();
      event.stopPropagation();
    };

    window.addEventListener("pointermove", onMove, { passive: false });
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    // Before the sheet's own Escape (which would close it).
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("click", onClick, true);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("click", onClick, true);
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
    };
  }, [listRef, y, scale, opacity]);

  /**
   * On a row's or a grip's `onPointerDown`: a press that may become a drag. `element` is what
   * lifts (the pressed element itself when not given).
   */
  const press = (event: ReactPointerEvent<HTMLElement>, spec: DragSpec, element?: HTMLElement) => {
    if (event.button !== 0 || current.current) return;
    if ((event.target as HTMLElement).closest("[data-no-drag]")) return;
    pending.current = {
      spec,
      x: event.clientX,
      y: event.clientY,
      element: element ?? event.currentTarget,
    };
  };

  return { drag, press, ghost: { y, scale, opacity } };
}
