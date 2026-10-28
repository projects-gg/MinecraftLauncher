// One line of the game's words drawn as whole pixels: an SVG of rectangles from the font's bitmaps
// (pixel-font.ts), sized in GUI pixels, so every letter lands on whole device pixels. Font text
// does not: Chromium smooths its edges even at whole pixel sizes (a grey fringe on the titles at
// 1280). The game's drop shadow is the same pixels one right and one down, a quarter of the colour.
// A line with a character the bitmaps lack is set in the font instead (Projects Pixel, then the
// system's), as before.
import type { CSSProperties, ReactNode } from "react";
import { cn } from "../../lib/cn";
import { canDraw, clipText, LINE_BOX, textAdvance, textPath } from "./pixel-font";
import { shadowOf, type TextRun } from "./text";

export interface PixelLineProps {
  text: string;
  color: string;
  /** The drop shadow's colour; none when left out (the game's window titles have none). */
  shadow?: string;
  /** The game's italic (a shulker box's "ve 2 tane daha..."): the rows lean, the shadow too. */
  italic?: boolean;
  /** Characters past this many GUI pixels are left out (a window title stops at the window's edge). */
  maxWidth?: number;
  className?: string;
  style?: CSSProperties;
}

const unit = (n: number) => `calc(${n} * var(--pl-inv-px))`;

export function PixelLine({
  text,
  color,
  shadow,
  italic = false,
  maxWidth,
  className,
  style,
}: PixelLineProps) {
  const shown = maxWidth === undefined ? text : clipText(text, maxWidth);
  if (!canDraw(shown)) {
    return (
      <span
        className={cn("pl-inv-text", className)}
        data-text={text}
        aria-hidden
        style={{
          ...style,
          color,
          fontStyle: italic ? "italic" : undefined,
          textShadow: shadow ? `var(--pl-inv-px) var(--pl-inv-px) 0 ${shadow}` : undefined,
          maxWidth: maxWidth === undefined ? undefined : unit(maxWidth),
        }}
      >
        {text}
      </span>
    );
  }
  const width = textAdvance(shown);
  const d = textPath(shown, italic);
  return (
    <svg
      className={cn("pl-inv-line", className)}
      data-text={text}
      viewBox={`0 0 ${width} ${LINE_BOX}`}
      shapeRendering="crispEdges"
      aria-hidden
      focusable="false"
      style={{ ...style, width: unit(width), height: unit(LINE_BOX) }}
    >
      {shadow ? <path d={d} fill={shadow} transform="translate(1 1)" /> : null}
      <path d={d} fill={color} />
    </svg>
  );
}

export interface PixelRunsProps {
  runs: TextRun[];
  className?: string;
  style?: CSSProperties;
}

/**
 * A line in the game's looks (a server's colours and flags, INV-HEADS), part by part on one pen,
 * as the game's font draws them: each part's drop shadow a quarter of its colour, all shadows
 * under all letters; bold drawn twice a pixel apart, a pixel more a letter; the strikethrough and
 * the underline a pixel high from a pixel before the part to its end (rows 3.5 and 8); obfuscated
 * letters as they are, still. A part with a character the bitmaps lack sets the
 * line in the font instead, as PixelLine does.
 */
export function PixelRuns({ runs, className, style }: PixelRunsProps) {
  // Obfuscated (§k) letters stay as they are, still: a view never moves without end.
  const shown = runs;
  const text = runs.map((run) => run.text).join("");
  if (!shown.every((run) => canDraw(run.text))) {
    return (
      <span className={cn("pl-inv-text", className)} data-text={text} aria-hidden style={style}>
        {shown.map((run, i) => {
          const lines = [run.underlined ? "underline" : "", run.strikethrough ? "line-through" : ""]
            .filter(Boolean)
            .join(" ");
          return (
            <span
              // biome-ignore lint/suspicious/noArrayIndexKey: a line's parts are its order
              key={i}
              style={{
                color: run.color,
                fontWeight: run.bold ? 700 : undefined,
                fontStyle: run.italic ? "italic" : undefined,
                textDecorationLine: lines || undefined,
                textShadow: `var(--pl-inv-px) var(--pl-inv-px) 0 ${shadowOf(run.color)}`,
              }}
            >
              {run.text}
            </span>
          );
        })}
      </span>
    );
  }
  const shadows: ReactNode[] = [];
  const letters: ReactNode[] = [];
  let pen = 0;
  for (const run of shown) {
    const d = textPath(run.text, run.italic, run.bold);
    const width = textAdvance(run.text, run.bold);
    const shadow = shadowOf(run.color);
    const draw = (into: ReactNode[], fill: string, dx: number, dy: number, key: string) => {
      into.push(
        <path key={`${key}${pen}`} d={d} fill={fill} transform={`translate(${pen + dx} ${dy})`} />,
      );
      if (run.bold) {
        into.push(
          <path
            key={`${key}b${pen}`}
            d={d}
            fill={fill}
            transform={`translate(${pen + dx + 1} ${dy})`}
          />,
        );
      }
      for (const [on, y, name] of [
        [run.strikethrough, 3.5, "m"],
        [run.underlined, 8, "n"],
      ] as const) {
        if (!on) continue;
        into.push(
          <rect
            key={`${key}${name}${pen}`}
            x={pen + dx - 1}
            y={y + dy}
            width={width + 1}
            height={1}
            fill={fill}
          />,
        );
      }
    };
    draw(shadows, shadow, 1, 1, "s");
    draw(letters, run.color, 0, 0, "l");
    pen += width;
  }
  return (
    <svg
      className={cn("pl-inv-line", className)}
      data-text={text}
      viewBox={`0 0 ${pen} ${LINE_BOX}`}
      shapeRendering="crispEdges"
      aria-hidden
      focusable="false"
      style={{ ...style, width: unit(pen), height: unit(LINE_BOX) }}
    >
      {shadows}
      {letters}
    </svg>
  );
}
