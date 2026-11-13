// SQOL-U3: an unnamed room's title (who is in it), fitted to its line at a name's end ("KorAtesi,
// x7Ender ve 2 kişi daha") instead of cut inside a name by the line's ellipsis; the whole list in a
// tooltip and to screen readers whenever the line says less.
import { Tooltip } from "@pl/ui";
import type { TFunction } from "i18next";
import { type RefObject, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { fitNames } from "./panel-rules";
import { namesLine } from "./rules";

let pen: CanvasRenderingContext2D | null | undefined;

/** How wide a line is drawn in `of`'s font; null where nothing can measure it (a test's page). */
function widthIn(of: HTMLElement): ((line: string) => number) | null {
  if (pen === undefined) {
    try {
      pen = document.createElement("canvas").getContext("2d");
    } catch {
      pen = null;
    }
  }
  const ctx = pen;
  if (!ctx) return null;
  const style = getComputedStyle(of);
  ctx.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
  if ("letterSpacing" in ctx) {
    ctx.letterSpacing = style.letterSpacing === "normal" ? "0px" : style.letterSpacing;
  }
  return (line) => ctx.measureText(line).width;
}

/** The names as one key, so a new array with the same names measures nothing again. */
const SEP = "\u0000";

function lineFor(key: string, of: HTMLElement | null, room: number | null, t: TFunction): string {
  const names = key ? key.split(SEP) : [];
  const width = of ? widthIn(of) : null;
  if (room === null || !width) return namesLine(names, t);
  // A pixel to spare: the page may draw a word a hair wider than the canvas measures it.
  return fitNames(names, (line) => width(line) + 1 <= room, t);
}

export function FittedNames({
  names,
  room,
  box,
  refit,
  className,
}: {
  names: readonly string[];
  /** How wide the line may be now, in CSS pixels; null when unknown (the line truncates as before). */
  room: () => number | null;
  /** A box whose size changes the line's room (the header's row); the window's size always does. */
  box?: RefObject<HTMLElement | null>;
  /** Anything else that changes the line's room (the pencil beside the title). */
  refit?: unknown;
  className?: string;
}) {
  const { t } = useTranslation();
  const ref = useRef<HTMLSpanElement>(null);
  const roomNow = useRef(room);
  const key = names.join(SEP);
  const [line, setLine] = useState(() => namesLine(names, t));
  useLayoutEffect(() => {
    roomNow.current = room;
  });
  // biome-ignore lint/correctness/useExhaustiveDependencies: `refit` is a change to measure again
  useLayoutEffect(() => {
    let gone = false;
    const measure = () => {
      if (!gone) setLine(lineFor(key, ref.current, roomNow.current(), t));
    };
    measure();
    // The title's own font may still be on its way.
    void document.fonts?.ready.then(measure);
    window.addEventListener("resize", measure);
    const watched = box?.current;
    const observer =
      watched && typeof ResizeObserver !== "undefined" ? new ResizeObserver(measure) : null;
    if (watched) observer?.observe(watched);
    return () => {
      gone = true;
      window.removeEventListener("resize", measure);
      observer?.disconnect();
    };
  }, [key, t, box, refit]);
  const whole = names.join(", ");
  const cut = line !== whole;
  return (
    <Tooltip content={whole} disabled={!cut}>
      <span ref={ref} className={className}>
        <span aria-hidden={cut || undefined}>{line}</span>
        {cut ? <span className="sr-only">{whole}</span> : null}
      </span>
    </Tooltip>
  );
}
