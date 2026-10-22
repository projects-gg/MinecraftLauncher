import { STICKER_MOTIONS, STICKER_SETS } from "@pl/stickers";
import { describe, expect, it } from "vitest";
import { playStickerMotion, STICKER_PATTERNS } from "./sticker-motion";

/** Whether a transform leaves the element where it was (every number 0, scales 1). */
function atRest(transform: string): boolean {
  return [...transform.matchAll(/(\w+)\(([^)]*)\)/g)].every(([, name, args]) =>
    (args ?? "")
      .split(",")
      .map((value) => Number.parseFloat(value))
      .every((value) => (name?.startsWith("scale") ? value === 1 : value === 0)),
  );
}

describe("sticker motion", () => {
  it("has a pattern for every motion a sticker names", () => {
    expect(Object.keys(STICKER_PATTERNS).sort()).toEqual([...STICKER_MOTIONS].sort());
    for (const sticker of STICKER_SETS.flatMap((set) => set.stickers)) {
      expect(STICKER_PATTERNS[sticker.motion], sticker.id).toBeDefined();
    }
  });

  it("moves only transform and opacity, once, and ends where it began", () => {
    for (const [name, pattern] of Object.entries(STICKER_PATTERNS)) {
      const offsets = pattern.frames.map((frame) => frame.offset as number);
      expect(offsets[0], name).toBe(0);
      expect(offsets.at(-1), name).toBe(1);
      expect(offsets.every((offset, i) => i === 0 || offset > (offsets[i - 1] as number))).toBe(
        true,
      );
      for (const frame of pattern.frames) {
        for (const key of Object.keys(frame)) {
          expect(["offset", "transform", "opacity", "easing"], `${name} ${key}`).toContain(key);
        }
      }
      expect(atRest(String(pattern.frames[0]?.transform)), name).toBe(true);
      expect(atRest(String(pattern.frames.at(-1)?.transform)), name).toBe(true);
      expect(pattern.duration, name).toBeLessThanOrEqual(1200);
    }
  });

  it("lets a pattern under way finish instead of starting it again", () => {
    const started: Array<{ playState: string }> = [];
    const element = {
      style: {} as Record<string, string>,
      animate: () => {
        const animation = { playState: "running", addEventListener: () => {} };
        started.push(animation);
        return animation;
      },
    } as unknown as HTMLElement;
    expect(playStickerMotion(element, "bounce")).not.toBeNull();
    expect(playStickerMotion(element, "bounce")).toBeNull();
    expect(started).toHaveLength(1);
    (started[0] as { playState: string }).playState = "finished";
    expect(playStickerMotion(element, "wave")).not.toBeNull();
    expect(started).toHaveLength(2);
  });
});
