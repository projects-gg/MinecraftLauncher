// A window title laid out the way the game draws it, with IA-ART's menu glyphs
// (docs/content-format.md, "Item art" → Glyphs): from the title's place, a private-use character
// with a picture draws it (its left edge at the pen, its top at y + 7 − ascent) and moves the pen
// by its advance; one without a picture only moves the pen; one the document does not know is left
// out; any other character is title text, in the game's font, moving the pen by its width
// (pixel-font.ts, the same bitmaps the view draws).
// Plain data, so the view, its tests and the phone (FEAT-M) lay titles out the same way.
import { textAdvance } from "./pixel-font";
import type { TitleGlyph, TitleRun } from "./types";

export interface TitlePicture {
  /** Its top-left in the window's GUI pixels, and its drawn size. */
  x: number;
  y: number;
  width: number;
  height: number;
  src: string;
}

export interface TitleText {
  /** Where the text starts (the pen), in the window's GUI pixels; its top is the title's y. */
  x: number;
  text: string;
}

export interface TitleLayout {
  pictures: TitlePicture[];
  texts: TitleText[];
  /** How far the whole title moves the pen (for a centred title). */
  advance: number;
}

/**
 * Lays a title's runs out from the pen at (x, y). Without `glyph` every private-use character is
 * left out, and so is any the lookup does not know.
 */
export function layoutTitle(
  runs: TitleRun[],
  x: number,
  y: number,
  glyph?: (codePoint: number) => TitleGlyph | null | undefined,
): TitleLayout {
  const pictures: TitlePicture[] = [];
  const texts: TitleText[] = [];
  let pen = x;
  for (const run of runs) {
    if ("text" in run) {
      if (run.text) texts.push({ x: pen, text: run.text });
      pen += textAdvance(run.text);
      continue;
    }
    const found = glyph?.(run.glyph);
    if (!found) continue;
    const image = found.image;
    if (image && image.width > 0 && image.height > 0) {
      pictures.push({
        x: pen,
        y: y + 7 - image.ascent,
        width: image.width,
        height: image.height,
        src: image.src,
      });
    }
    pen += found.advance;
  }
  return { pictures, texts, advance: pen - x };
}
