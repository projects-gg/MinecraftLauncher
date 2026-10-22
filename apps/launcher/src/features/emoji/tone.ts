// Skin tones (CHAT2 §3.1): one choice for the whole picker, kept on this computer, the same tone
// on every person of an emoji (as Slack and Discord do). 0 is the default yellow.
import type { EmojiEntry } from "./data";

export type SkinTone = 0 | 1 | 2 | 3 | 4 | 5;
export const SKIN_TONES: readonly SkinTone[] = [0, 1, 2, 3, 4, 5];

/** The Fitzpatrick modifiers, light to dark (index = tone − 1). */
const MODIFIERS = [0x1f3fb, 0x1f3fc, 0x1f3fd, 0x1f3fe, 0x1f3ff];
const VS16 = 0xfe0f;

export function isSkinTone(value: unknown): value is SkinTone {
  return typeof value === "number" && SKIN_TONES.includes(value as SkinTone);
}

/** The emoji in the tone, or as it is when it has no tones (or the tone is the default). */
export function withTone(
  entry: Pick<EmojiEntry, "emoji" | "tones">,
  tone: SkinTone,
  toneBases: ReadonlySet<number>,
): string {
  if (tone === 0 || !entry.tones) return entry.emoji;
  if (entry.tones !== "rule") return entry.tones[tone - 1] ?? entry.emoji;
  const modifier = MODIFIERS[tone - 1] as number;
  const cps = [...entry.emoji].map((ch) => ch.codePointAt(0) as number);
  const out: number[] = [];
  for (let index = 0; index < cps.length; index++) {
    const cp = cps[index] as number;
    out.push(cp);
    if (toneBases.has(cp)) {
      out.push(modifier);
      if (cps[index + 1] === VS16) index++;
    }
  }
  return String.fromCodePoint(...out);
}

/** An emoji without its skin tones and presentation selectors (to find it in the list). */
export function bareEmoji(emoji: string): string {
  return [...emoji]
    .filter((ch) => {
      const cp = ch.codePointAt(0) as number;
      return cp !== VS16 && !MODIFIERS.includes(cp);
    })
    .join("");
}
