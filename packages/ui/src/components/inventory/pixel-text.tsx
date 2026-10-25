// One line of the game's words drawn as whole pixels: an SVG of rectangles from the font's bitmaps
// (pixel-font.ts), sized in GUI pixels, so every letter lands on whole device pixels. Font text
// does not: Chromium smooths its edges even at whole pixel sizes (a grey fringe on the titles at
// 1280). The game's drop shadow is the same pixels one right and one down, a quarter of the colour.
// A line with a character the bitmaps lack is set in the font instead (Projects Pixel, then the
// system's), as before.
import type { CSSProperties } from "react";
import { cn } from "../../lib/cn";
import { canDraw, clipText, LINE_BOX, textAdvance, textPath } from "./pixel-font";

export interface PixelLineProps {
  text: string;
  color: string;
  /** The drop shadow's colour; none when left out (the game's window titles have none). */
  shadow?: string;
  /** Characters past this many GUI pixels are left out (a window title stops at the window's edge). */
  maxWidth?: number;
  className?: string;
  style?: CSSProperties;
}

const unit = (n: number) => `calc(${n} * var(--pl-inv-px))`;

export function PixelLine({ text, color, shadow, maxWidth, className, style }: PixelLineProps) {
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
          textShadow: shadow ? `var(--pl-inv-px) var(--pl-inv-px) 0 ${shadow}` : undefined,
          maxWidth: maxWidth === undefined ? undefined : unit(maxWidth),
        }}
      >
        {text}
      </span>
    );
  }
  const width = textAdvance(shown);
  const d = textPath(shown);
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
