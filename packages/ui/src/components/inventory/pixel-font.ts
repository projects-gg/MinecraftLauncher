// The game's letters as whole pixels (Projects Pixel's bitmaps, pixel-glyphs.ts): how far a text
// moves the pen, where it wraps and the pixels it lights. Plain data, so the view, the title layout
// (title.ts), the tests and the phone (FEAT-M) measure words the same way.
import { PIXEL_GLYPHS } from "./pixel-glyphs";

/** A line's box in GUI pixels: the game's tooltip pitch. Its top is the glyph cell's top. */
export const LINE_BOX = 10;

/** How far a character the bitmaps lack moves the pen (the fallback font draws it). */
const MISSING_ADVANCE = 6;

/** How far one character moves the pen, in GUI pixels (its width and the gap after it). */
export function glyphAdvance(char: string): number {
  return PIXEL_GLYPHS[char]?.[0] ?? MISSING_ADVANCE;
}

/** How far a text moves the pen in the game's font, in GUI pixels. */
export function textAdvance(text: string): number {
  let width = 0;
  for (const char of text) width += glyphAdvance(char);
  return width;
}

/** Whether every character of the text has pixels (otherwise the line falls back to the font). */
export function canDraw(text: string): boolean {
  for (const char of text) if (!PIXEL_GLYPHS[char]) return false;
  return true;
}

/** The text's first characters that fit `width` GUI pixels (whole characters, nothing added). */
export function clipText(text: string, width: number): string {
  let pen = 0;
  let out = "";
  for (const char of text) {
    const advance = glyphAdvance(char);
    // A character's last column of pixels is the one before its gap.
    if (pen + advance - 1 > width) break;
    out += char;
    pen += advance;
  }
  return out;
}

/**
 * A line broken at spaces into lines of at most `width` GUI pixels; a word wider than that breaks
 * between characters. The space at a break is dropped.
 */
export function wrapText(text: string, width: number): string[] {
  if (width <= 0 || textAdvance(text) <= width) return [text];
  const lines: string[] = [];
  const space = glyphAdvance(" ");
  let line = "";
  let pen = 0;
  for (const word of text.split(" ")) {
    const advance = textAdvance(word);
    if (line && pen + space + advance <= width) {
      line += ` ${word}`;
      pen += space + advance;
      continue;
    }
    if (line) lines.push(line);
    line = "";
    pen = 0;
    if (advance <= width) {
      line = word;
      pen = advance;
      continue;
    }
    for (const char of word) {
      const step = glyphAdvance(char);
      if (line && pen + step > width) {
        lines.push(line);
        line = "";
        pen = 0;
      }
      line += char;
      pen += step;
    }
  }
  if (line) lines.push(line);
  return lines;
}

const paths = new Map<string, string>();

/**
 * The text's pixels as one SVG path in GUI pixels: x from the pen's start, y down from the glyph
 * cell's top (the baseline is under row 6; accents of capitals reach above 0, tails below 7).
 * Runs of lit pixels in a row are one rectangle each.
 */
export function textPath(text: string): string {
  const cached = paths.get(text);
  if (cached !== undefined) return cached;
  let d = "";
  let pen = 0;
  for (const char of text) {
    const glyph = PIXEL_GLYPHS[char];
    if (!glyph) {
      pen += MISSING_ADVANCE;
      continue;
    }
    const [advance = MISSING_ADVANCE, top = 0] = glyph;
    for (let i = 2; i < glyph.length; i++) {
      const mask = glyph[i] ?? 0;
      const y = 6 - top + (i - 2);
      let x = 0;
      while (mask >> x) {
        if (!((mask >> x) & 1)) {
          x++;
          continue;
        }
        let w = 1;
        while ((mask >> (x + w)) & 1) w++;
        d += `M${pen + x} ${y}h${w}v1h${-w}z`;
        x += w;
      }
    }
    pen += advance;
  }
  // Item names and lore repeat while the player looks around; a bounded memory keeps them.
  if (paths.size > 600) paths.clear();
  paths.set(text, d);
  return d;
}
