// A console line in the game's looks (CONSOLE-COLOR). Each part shows in its colour and flags. A
// part with a hover shows the game's tooltip for the mouse pointer, a tap or the keyboard. A part
// with a click is a button the host answers: a command's card, the command line, a copy. Only
// plain text nodes; a colour only from a checked code; nothing runs on its own.
import { ArrowSquareOut, TerminalWindow } from "@phosphor-icons/react";
import {
  type CSSProperties,
  forwardRef,
  type KeyboardEvent,
  memo,
  type PointerEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { type TipAnchor, tooltipPlace } from "../inventory/float";
import { type TooltipLine, tooltipLines } from "../inventory/text";
import { INVENTORY_TEXTS, type InventoryLang } from "../inventory/texts";
import { TooltipLines } from "../inventory/tooltip";
import type { InvItem } from "../inventory/types";
import "../inventory/inventory.css";
import "./console.css";
import type { ConsoleGround } from "./ground";
import {
  type ConsoleClick,
  type ConsoleEvent,
  type ConsoleHover,
  type ConsoleRun,
  hoverEntityLines,
  hoverTextLines,
  lineRuns,
  safeEvents,
} from "./looks";
import { CONSOLE_TEXTS } from "./texts";

export interface ConsoleTextProps {
  text: string;
  /** The machine's parts (`st`); absent or not the contract's: the line as today. */
  st?: unknown;
  /** The machine's events (`ev`). */
  ev?: unknown;
  lang: InventoryLang;
  /**
   * A part's click. Without it no click is offered (a console that cannot send). The host decides:
   * a command or a link waits for the player's word, a suggestion fills the command line, a copy
   * copies.
   */
  onClick?: (click: ConsoleClick) => void;
  /** A hover's item as the inventory shows it (the host's model); without it an item shows none. */
  itemOf?: (raw: unknown) => InvItem | null;
  /** The line without looks (today's words, a private line's name standing out). */
  children?: ReactNode;
  /** What it is drawn on: the console (default) or a plate on it (a chosen row, a button). */
  ground?: ConsoleGround;
}

/** The game's GUI pixel in CSS pixels for the console's tooltips. */
const UNIT = 2;

function decoration(run: ConsoleRun, hint: boolean): string | undefined {
  const lines = [];
  if (run.underlined || hint) lines.push("underline");
  if (run.strikethrough) lines.push("line-through");
  return lines.length ? lines.join(" ") : undefined;
}

function runStyle(run: ConsoleRun, hint = false): CSSProperties | undefined {
  const style: CSSProperties = {};
  if (run.color) style.color = run.color;
  if (run.bold) style.fontWeight = 700;
  if (run.italic) style.fontStyle = "italic";
  const line = decoration(run, hint);
  if (line) style.textDecorationLine = line;
  // A server's underline thin and faint: never read as a part that does something.
  if (run.underlined) {
    style.textDecorationThickness = "1px";
    style.textDecorationColor = "color-mix(in srgb, currentColor 45%, transparent)";
  }
  return Object.keys(style).length ? style : undefined;
}

/** What a command and a link carry: a small line icon, so the card they open never surprises. */
/**
 * The small line icon of what a click does, a command or a link: a console part's, and a dialog
 * button's (DialogCard) in its label's colour.
 */
export function ActIcon({ a, color }: { a: string; color?: string }) {
  if (a === "run_command") {
    return (
      <TerminalWindow
        className="pl-con-act-icon"
        size={12}
        weight="bold"
        color={color}
        aria-hidden
      />
    );
  }
  if (a === "open_url") {
    return (
      <ArrowSquareOut
        className="pl-con-act-icon"
        size={12}
        weight="bold"
        color={color}
        aria-hidden
      />
    );
  }
  return null;
}

function linesOf(
  hover: ConsoleHover | undefined,
  lang: InventoryLang,
  itemOf: ConsoleTextProps["itemOf"],
): TooltipLine[] | null {
  if (!hover) return null;
  if ("text" in hover) return hoverTextLines(hover);
  if ("entity" in hover) {
    const lines = hoverEntityLines(hover, lang, CONSOLE_TEXTS[lang].entityType);
    return lines.length ? lines : null;
  }
  const item = itemOf?.(hover.item);
  return item ? tooltipLines(item, lang, INVENTORY_TEXTS[lang]) : null;
}

interface TipHandle {
  place: (anchor: TipAnchor) => void;
}

/** The game's tooltip over the page (never inside the line's own markup), placed as the game does. */
const GameTip = forwardRef<TipHandle, { lines: TooltipLine[]; anchor: TipAnchor }>(function GameTip(
  { lines, anchor },
  ref,
) {
  const node = useRef<HTMLDivElement>(null);
  const place = useCallback((at: TipAnchor) => {
    const el = node.current;
    if (!el) return;
    const size = el.getBoundingClientRect();
    const { x, y } = tooltipPlace(
      at,
      { width: size.width - 6 * UNIT, height: size.height - 6 * UNIT },
      {
        width: document.documentElement.clientWidth,
        height: document.documentElement.clientHeight,
      },
      UNIT,
    );
    el.style.transform = `translate(${Math.round(x - 3 * UNIT)}px, ${Math.round(y - 3 * UNIT)}px)`;
  }, []);
  useImperativeHandle(ref, () => ({ place }), [place]);
  useLayoutEffect(() => place(anchor), [anchor, place]);
  // At most 240 GUI pixels of words, 8 CSS pixels from the screen's edges (the frame 3 a side).
  const width = Math.max(
    16,
    Math.floor(Math.min(document.documentElement.clientWidth - 16, 240 * UNIT) / UNIT) - 6,
  );
  return createPortal(
    <div
      ref={node}
      className="pl-inv-tip"
      data-shown=""
      style={{ "--pl-inv-px": `${UNIT}px` } as CSSProperties}
      aria-hidden
    >
      <TooltipLines lines={lines} width={width} />
    </div>,
    document.body,
  );
});

function boxOf(el: Element): { left: number; top: number; right: number; bottom: number } {
  const { left, top, right, bottom } = el.getBoundingClientRect();
  return { left, top, right, bottom };
}

/** An item's tooltip lines as the game writes them (the inventory's own). */
export function itemTipLines(item: InvItem, lang: InventoryLang): TooltipLine[] {
  return tooltipLines(item, lang, INVENTORY_TEXTS[lang]);
}

/**
 * Something the game draws its tooltip for outside a line (a dialog's item, CONSOLE-ANY): the
 * console's own game tooltip, beside the pointer while it is over it, beside it for the keyboard's
 * focus and for a tap (the next tap elsewhere, a scroll or Escape closes it). Without lines it is
 * the plain element.
 */
export function GameTipTarget({
  lines,
  label,
  className,
  children,
}: {
  lines: TooltipLine[] | null;
  /** Its accessible name (what the tooltip says first). */
  label: string;
  className?: string;
  children: ReactNode;
}) {
  const [tip, setTip] = useState<{
    anchor: TipAnchor;
    by: "pointer" | "touch" | "keyboard";
  } | null>(null);
  const tipRef = useRef<TipHandle>(null);
  const node = useRef<HTMLButtonElement>(null);
  const frame = useRef(0);
  const pointer = useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  useEffect(() => {
    if (!tip) return;
    const close = () => setTip(null);
    const away = (e: Event) => {
      if (!node.current?.contains(e.target as Node)) close();
    };
    const onEscape = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    document.addEventListener("pointerdown", away, true);
    document.addEventListener("keydown", onEscape);
    return () => {
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
      document.removeEventListener("pointerdown", away, true);
      document.removeEventListener("keydown", onEscape);
    };
  }, [tip]);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  const follow = (e: PointerEvent) => {
    if (e.pointerType === "touch" || !lines) return;
    pointer.current = { x: e.clientX, y: e.clientY };
    if (tip?.by !== "pointer") {
      setTip({ anchor: { pointer: pointer.current }, by: "pointer" });
      return;
    }
    if (!frame.current) {
      frame.current = requestAnimationFrame(() => {
        frame.current = 0;
        tipRef.current?.place({ pointer: pointer.current });
      });
    }
  };
  if (!lines) {
    return (
      <span role="img" aria-label={label} className={className}>
        {children}
      </span>
    );
  }
  return (
    <>
      {/* A button only to show its tooltip: the keyboard reaches it, a tap opens it. */}
      <button
        ref={node}
        type="button"
        aria-label={label}
        className={className}
        onPointerEnter={follow}
        onPointerMove={follow}
        onPointerLeave={(e) => {
          if (e.pointerType !== "touch" && tip?.by === "pointer") setTip(null);
        }}
        onPointerDown={(e) => {
          if (e.pointerType !== "touch" || !lines || !node.current) return;
          const box = boxOf(node.current);
          setTip((open) => (open ? null : { anchor: { slot: box, by: "touch" }, by: "touch" }));
        }}
        onFocus={() => {
          if (lines && node.current?.matches(":focus-visible")) {
            setTip({ anchor: { slot: boxOf(node.current), by: "keyboard" }, by: "keyboard" });
          }
        }}
        onBlur={() => setTip((open) => (open?.by === "keyboard" ? null : open))}
      >
        {children}
      </button>
      {tip && lines ? <GameTip ref={tipRef} lines={lines} anchor={tip.anchor} /> : null}
    </>
  );
}

function ActivePart({
  run,
  event,
  lang,
  onClick,
  itemOf,
}: {
  run: ConsoleRun;
  event: ConsoleEvent;
  lang: InventoryLang;
  onClick?: (click: ConsoleClick) => void;
  itemOf?: ConsoleTextProps["itemOf"];
}) {
  const click = onClick ? event.click : undefined;
  const lines = useMemo(() => linesOf(event.hover, lang, itemOf), [event.hover, lang, itemOf]);
  const [tip, setTip] = useState<{
    anchor: TipAnchor;
    by: "pointer" | "touch" | "keyboard";
  } | null>(null);
  const tipRef = useRef<TipHandle>(null);
  const partRef = useRef<HTMLSpanElement>(null);
  const frame = useRef(0);
  const pointer = useRef<{ x: number; y: number }>({ x: 0, y: 0 });

  // A tooltip goes with what scrolls under it, and a tap's with the next tap elsewhere.
  useEffect(() => {
    if (!tip) return;
    const close = () => setTip(null);
    const away = (e: Event) => {
      if (!partRef.current?.contains(e.target as Node)) close();
    };
    const onEscape = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    document.addEventListener("pointerdown", away, true);
    document.addEventListener("keydown", onEscape);
    return () => {
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
      document.removeEventListener("pointerdown", away, true);
      document.removeEventListener("keydown", onEscape);
    };
  }, [tip]);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  const follow = (e: PointerEvent) => {
    if (e.pointerType === "touch" || !lines) return;
    pointer.current = { x: e.clientX, y: e.clientY };
    if (tip?.by !== "pointer") {
      setTip({ anchor: { pointer: pointer.current }, by: "pointer" });
      return;
    }
    if (!frame.current) {
      frame.current = requestAnimationFrame(() => {
        frame.current = 0;
        tipRef.current?.place({ pointer: pointer.current });
      });
    }
  };
  const tap = (e: PointerEvent) => {
    if (e.pointerType !== "touch" || click || !lines || !partRef.current) return;
    setTip((open) =>
      open
        ? null
        : { anchor: { slot: boxOf(partRef.current as Element), by: "touch" }, by: "touch" },
    );
  };
  const act = () => {
    if (!click || !onClick) return;
    setTip(null);
    onClick(click);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      act();
    }
  };

  const hint = !run.underlined;
  return (
    <>
      <span
        ref={partRef}
        className={hint ? "pl-con-hint" : undefined}
        style={runStyle(run, hint)}
        onPointerEnter={follow}
        onPointerMove={follow}
        onPointerLeave={(e) => {
          if (e.pointerType !== "touch" && tip?.by === "pointer") setTip(null);
        }}
        onPointerDown={tap}
        {...(click
          ? {
              role: "button",
              tabIndex: 0,
              "data-pl-con-act": "",
              onClick: act,
              onKeyDown,
              onFocus: () => {
                if (lines && partRef.current?.matches(":focus-visible")) {
                  setTip({
                    anchor: { slot: boxOf(partRef.current), by: "keyboard" },
                    by: "keyboard",
                  });
                }
              },
              onBlur: () => setTip((open) => (open?.by === "keyboard" ? null : open)),
            }
          : {})}
      >
        {run.text}
        {click ? <ActIcon a={click.a} /> : null}
      </span>
      {tip && lines ? <GameTip ref={tipRef} lines={lines} anchor={tip.anchor} /> : null}
    </>
  );
}

function PlainPart({ run }: { run: ConsoleRun }) {
  const style = runStyle(run);
  return style ? <span style={style}>{run.text}</span> : run.text;
}

/**
 * The line's words in the game's looks when its parts are the contract's, else `children` (or
 * the plain text): a part without a colour takes the line's own (its kind's tone).
 */
export const ConsoleText = memo(function ConsoleText({
  text,
  st,
  ev,
  lang,
  onClick,
  itemOf,
  children,
  ground = "console",
}: ConsoleTextProps) {
  const events = useMemo(() => safeEvents(ev), [ev]);
  const runs = useMemo(() => lineRuns(text, st, events, ground), [text, st, events, ground]);
  if (!runs) return <>{children ?? text}</>;
  return (
    <>
      {runs.map((run, i) => {
        const event = run.event === undefined ? null : (events[run.event] ?? null);
        // A hover shows when the host can draw it; a click only where the console can send.
        const hover = event?.hover && (!("item" in event.hover) || itemOf);
        const active = event && (hover || (onClick && event.click));
        return active ? (
          <ActivePart
            // biome-ignore lint/suspicious/noArrayIndexKey: a line's parts are its order and never change
            key={i}
            run={run}
            event={event}
            lang={lang}
            onClick={onClick}
            itemOf={itemOf}
          />
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: a line's parts are its order and never change
          <PlainPart key={i} run={run} />
        );
      })}
    </>
  );
});
