// The room panel's separator on its left edge (SQOL-U3): a drag widens or narrows the panel (the
// pointer held, one width a frame, nothing animating meanwhile), and so do the keys: ← and → by
// 16 px, with Shift by 64, Home and End to the ends, Enter (or a double click) between today's width
// and the last wide one. A focusable vertical separator, as the window splitter pattern (APG): its
// value is the panel's width. Its quiet grip shows under the pointer and with the keyboard's focus;
// Esc during a drag gives the width back.
import { Tooltip } from "@pl/ui";
import { type KeyboardEvent, type PointerEvent, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { formatPercent } from "@/lib/format";
import {
  clampWidth,
  keyWidth,
  layoutFor,
  type PanelPlace,
  toggledWidth,
  usePanelSize,
  type WidthLimits,
} from "./panel-size";

export function PanelResizer({
  place,
  width,
  limits,
  windowWidth,
}: {
  place: PanelPlace;
  /** The panel's width now. */
  width: number;
  limits: WidthLimits;
  windowWidth: number;
}) {
  const { t } = useTranslation();
  const drag = usePanelSize((s) => s.drag);
  const choose = usePanelSize((s) => s.choose);
  const lastWide = usePanelSize((s) => s.lastWide[place]);
  // Where the drag began, and the width it asks for (set once a frame).
  const held = useRef<{ x: number; width: number; pointer: number } | null>(null);
  const asked = useRef<number | null>(null);
  const frame = useRef(0);
  const now = useRef({ place, limits });
  now.current = { place, limits };

  // The drag ends: its width kept (let go), or given back (Esc, a cancelled pointer).
  const stop = useRef((keep: boolean) => {
    if (!held.current) return;
    held.current = null;
    cancelAnimationFrame(frame.current);
    frame.current = 0;
    const final = asked.current;
    asked.current = null;
    const { drag: setDrag, choose: settle } = usePanelSize.getState();
    if (keep && final !== null) settle(now.current.place, final, now.current.limits);
    else setDrag(null);
  }).current;

  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape" || !held.current) return;
      // The panel stays open: Esc only gives the width back.
      event.preventDefault();
      event.stopPropagation();
      stop(false);
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      stop(false);
    };
  }, [stop]);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    // No text selection, no focus jump: the pointer holds the edge.
    event.preventDefault();
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // The pointer went before it could be held: the moves still come while it is over the edge.
    }
    held.current = { x: event.clientX, width, pointer: event.pointerId };
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const from = held.current;
    if (!from || event.pointerId !== from.pointer) return;
    // Left of where it began widens the panel (it stands on the window's right).
    asked.current = clampWidth(from.width + from.x - event.clientX, now.current.limits);
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      if (held.current && asked.current !== null) drag(asked.current);
    });
  };
  const onPointerEnd = (keep: boolean) => (event: PointerEvent<HTMLDivElement>) => {
    if (held.current?.pointer === event.pointerId) stop(keep);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const to =
      event.key === "Enter"
        ? toggledWidth(width, limits, lastWide)
        : keyWidth(event.key, event.shiftKey, width, limits);
    if (to === null) return;
    event.preventDefault();
    choose(place, to, limits);
  };

  const percent = formatPercent((width / Math.max(1, windowWidth)) * 100);
  const columns = layoutFor(width) === "stacked" ? "one" : "two";
  return (
    <Tooltip content={t("roomChat.resize.tooltip")} side="left">
      {/* biome-ignore lint/a11y/useSemanticElements: a focusable splitter (APG), not a rule */}
      <div
        role="separator"
        tabIndex={0}
        aria-orientation="vertical"
        aria-label={t("roomChat.resize.label")}
        aria-valuenow={width}
        aria-valuemin={limits.min}
        aria-valuemax={limits.max}
        aria-valuetext={t(`roomChat.resize.value.${columns}`, { percent })}
        data-panel-resizer=""
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerEnd(true)}
        onPointerCancel={onPointerEnd(false)}
        onLostPointerCapture={onPointerEnd(true)}
        onKeyDown={onKeyDown}
        onDoubleClick={() => choose(place, toggledWidth(width, limits, lastWide), limits)}
        className="group/resize absolute inset-y-0 left-0 z-20 flex w-2 cursor-ew-resize touch-none items-center justify-center outline-none select-none"
      >
        <span
          aria-hidden
          className="h-12 w-[3px] rounded-full bg-white/35 opacity-0 transition-[opacity,background-color] duration-150 group-hover/resize:opacity-100 group-focus-visible/resize:bg-gold-300 group-focus-visible/resize:opacity-100"
        />
      </div>
    </Tooltip>
  );
}
