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

/** How far a text moves the pen in the game's font, in GUI pixels (bold: a pixel more a letter). */
export function textAdvance(text: string, bold = false): number {
  let width = 0;
  for (const char of text) width += glyphAdvance(char) + (bold ? 1 : 0);
  return width;
}

/** A line's parts that look alike (one colour, the same flags). */
export interface StyledText {
  text: string;
  bold?: boolean;
}

/**
 * `wrapText` for a line of parts: the same breaks, each part's letters as wide as it draws them
 * (bold ones a pixel more), the parts cut where the line breaks.
 */
export function wrapRuns<T extends StyledText>(runs: T[], width: number): T[][] {
  const chars: { char: string; run: number }[] = [];
  runs.forEach((run, i) => {
    for (const char of run.text) chars.push({ char, run: i });
  });
  const advance = (entry: { char: string; run: number }) =>
    glyphAdvance(entry.char) + (runs[entry.run]?.bold ? 1 : 0);
  const total = chars.reduce((sum, entry) => sum + advance(entry), 0);
  if (width <= 0 || total <= width) return [runs];
  type Entry = (typeof chars)[number];
  const words: Entry[][] = [[]];
  const spaces: Entry[] = [];
  for (const entry of chars) {
    if (entry.char === " ") {
      spaces.push(entry);
      words.push([]);
    } else {
      words[words.length - 1]?.push(entry);
    }
  }
  const lines: Entry[][] = [];
  let line: Entry[] = [];
  let pen = 0;
  words.forEach((word, i) => {
    const wide = word.reduce((sum, entry) => sum + advance(entry), 0);
    const space = i > 0 ? spaces[i - 1] : undefined;
    const gap = space ? advance(space) : 0;
    if (line.length && pen + gap + wide <= width) {
      if (space) line.push(space);
      line.push(...word);
      pen += gap + wide;
      return;
    }
    if (line.length) lines.push(line);
    line = [];
    pen = 0;
    if (wide <= width) {
      line = [...word];
      pen = wide;
      return;
    }
    for (const entry of word) {
      const step = advance(entry);
      if (line.length && pen + step > width) {
        lines.push(line);
        line = [];
        pen = 0;
      }
      line.push(entry);
      pen += step;
    }
  });
  if (line.length) lines.push(line);
  return lines.map((entries) => {
    const parts: { run: number; text: string }[] = [];
    for (const entry of entries) {
      const tail = parts[parts.length - 1];
      if (tail && tail.run === entry.run) tail.text += entry.char;
      else parts.push({ run: entry.run, text: entry.char });
    }
    return parts.flatMap(({ run, text }) => {
      const source = runs[run];
      return source ? [{ ...source, text }] : [];
    });
  });
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
const leaningPaths = new Map<string, string>();
const boldPaths = new Map<string, string>();
const boldLeaningPaths = new Map<string, string>();

/**
 * The text's pixels as one SVG path in GUI pixels: x from the pen's start, y down from the glyph
 * cell's top (the baseline is under row 6; accents of capitals reach above 0, tails below 7).
 * Runs of lit pixels in a row are one rectangle each. Italic leans each row as the game's glyph
 * quad does (its top a pixel right, its bottom a pixel left: `1 - y / 4` at the row's middle), and
 * whole device pixels turn the lean into the game's steps. Bold letters stand a pixel further
 * apart (the game draws them twice, a pixel apart: the caller draws the path twice).
 */
export function textPath(text: string, italic = false, bold = false): string {
  const memo = italic ? (bold ? boldLeaningPaths : leaningPaths) : bold ? boldPaths : paths;
  const cached = memo.get(text);
  if (cached !== undefined) return cached;
  let d = "";
  let pen = 0;
  for (const char of text) {
    const glyph = PIXEL_GLYPHS[char];
    if (!glyph) {
      pen += MISSING_ADVANCE + (bold ? 1 : 0);
      continue;
    }
    const [advance = MISSING_ADVANCE, top = 0] = glyph;
    for (let i = 2; i < glyph.length; i++) {
      const mask = glyph[i] ?? 0;
      const y = 6 - top + (i - 2);
      const lean = italic ? 0.875 - 0.25 * y : 0;
      let x = 0;
      while (mask >> x) {
        if (!((mask >> x) & 1)) {
          x++;
          continue;
        }
        let w = 1;
        while ((mask >> (x + w)) & 1) w++;
        d += `M${pen + x + lean} ${y}h${w}v1h${-w}z`;
        x += w;
      }
    }
    pen += advance + (bold ? 1 : 0);
  }
  // Item names and lore repeat while the player looks around; a bounded memory keeps them.
  if (memo.size > 600) memo.clear();
  memo.set(text, d);
  return d;
}
