// Putting an emoji into the composer (CHAT2 §3.1, §3.3): at the caret, in place of a selection,
// only when the whole text still fits the limit (counted in code points, as the service counts),
// so an emoji is never cut in half and the field never holds more than it can send.

/** Code points of a text (what the chat's limit counts). */
export function codePoints(text: string): number {
  let count = 0;
  for (const _ of text) count++;
  return count;
}

/**
 * The field's text with `insert` in place of the selection `[start, end)` (UTF-16 offsets, as a
 * textarea gives them) and the caret after it; null when the result would pass `max`.
 */
export function insertText(
  value: string,
  start: number,
  end: number,
  insert: string,
  max: number,
): { value: string; caret: number } | null {
  const from = Math.max(0, Math.min(start, end, value.length));
  const to = Math.min(value.length, Math.max(start, end, from));
  const next = value.slice(0, from) + insert + value.slice(to);
  if (codePoints(next) > max) return null;
  return { value: next, caret: from + insert.length };
}

let rgi: RegExp | null | undefined;

/** One emoji (RGI) or a lone pictograph, with or without VS16. */
function isEmoji(grapheme: string): boolean {
  if (rgi === undefined) {
    try {
      // Built at run time: the `v` flag (RGI_Emoji) is newer than some build targets.
      // biome-ignore lint/complexity/useRegexLiterals: a literal with the v flag fails older build targets
      rgi = new RegExp("^\\p{RGI_Emoji}$", "v");
    } catch {
      rgi = null;
    }
  }
  return (rgi?.test(grapheme) ?? false) || /^\p{Extended_Pictographic}\uFE0F?$/u.test(grapheme);
}

/**
 * How many emoji a message is made of when it is only one to three emoji (spaces between them
 * allowed): such a message is drawn large and without a bubble (§3.1). 0 for anything else.
 */
export function bigEmojiCount(text: string): number {
  const trimmed = text.trim();
  if (!trimmed || trimmed.length > 64) return 0;
  let count = 0;
  for (const { segment } of new Intl.Segmenter("en", { granularity: "grapheme" }).segment(
    trimmed,
  )) {
    if (/^\s+$/u.test(segment)) continue;
    count += 1;
    if (count > 3 || !isEmoji(segment)) return 0;
  }
  return count;
}
