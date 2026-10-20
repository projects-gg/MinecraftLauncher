import {
  type OptionBox,
  PLATE_SPRING,
  type PlateDrift,
  plateAt,
  rebase,
  settleTime,
  underlineFrames,
} from "@pl/ui";
import { describe, expect, it } from "vitest";

// The gliding underline of `Tabs` (INTEGRATE1, packages/ui/src/glide.ts).
const still = { x: 0, v: 0 };
const atRest: PlateDrift = { left: still, right: still, top: still };

/** A keyframe's translateX and scaleX. */
function bar(frame: Keyframe | undefined) {
  const found = /^translateX\((-?[\d.]+)px\) scaleX\((-?[\d.]+)\)$/.exec(String(frame?.transform));
  return { x: Number(found?.[1]), scale: Number(found?.[2]) };
}

/** The project dialog's underlines ("Açıklama", "Galeri 4", "Sürümler") in the tab list's pixels. */
const aciklama: OptionBox = { left: 8, right: 74, top: 32, bottom: 34 };
const galeri: OptionBox = { left: 92, right: 172, top: 32, bottom: 34 };
const surumler: OptionBox = { left: 190, right: 256, top: 32, bottom: 34 };
const width = (box: OptionBox) => box.right - box.left;
const toGaleri = rebase(atRest, aciklama, galeri);

describe("underlineFrames", () => {
  const glide = underlineFrames(PLATE_SPRING, toGaleri, width(galeri));

  it("starts on the old tab and ends at rest under the new one", () => {
    if (!glide) throw new Error("no glide");
    const first = bar(glide.frames[0]);
    expect(galeri.left + first.x).toBeCloseTo(aciklama.left, 2);
    expect(galeri.left + first.x + width(galeri) * first.scale).toBeCloseTo(aciklama.right, 2);
    expect(glide.frames.at(-1)).toEqual({ offset: 1, transform: "translateX(0px) scaleX(1)" });
    expect(glide.frames[0]?.offset).toBe(0);
  });

  it("takes as long as the plate's spring, both edges together", () => {
    if (!glide) throw new Error("no glide");
    expect(glide.duration).toBe(Math.round(settleTime(PLATE_SPRING, toGaleri) * 1000));
    expect(glide.duration).toBeGreaterThan(200);
    expect(glide.duration).toBeLessThan(500);
    for (const frame of glide.frames) {
      const { x, scale } = bar(frame);
      const left = galeri.left + x;
      const right = left + width(galeri) * scale;
      // Never flipped or thinner than the narrower tab's, never past the tab it goes to.
      expect(right - left).toBeGreaterThan(width(aciklama) - 0.5);
      expect(left).toBeGreaterThan(aciklama.left - 0.5);
      expect(right).toBeLessThan(galeri.right + 1.5);
    }
  });

  it("keeps to its row: whatever moved the list up or down is not in the motion", () => {
    // The old shared layout animation saw the list 240 px lower after a scroller clamped.
    const moved = { ...toGaleri, top: { x: 240, v: 0 } };
    expect(underlineFrames(PLATE_SPRING, moved, width(galeri))).toEqual(glide);
    for (const frame of glide?.frames ?? []) {
      expect(String(frame.transform)).not.toMatch(/translateY|translate\(/);
    }
  });

  it("carries on from where the bar is when the next tab is picked mid-glide", () => {
    const mid = plateAt(PLATE_SPRING, toGaleri, 0.06);
    const next = underlineFrames(PLATE_SPRING, rebase(mid, galeri, surumler), width(surumler));
    if (!next) throw new Error("no glide");
    const first = bar(next.frames[0]);
    expect(surumler.left + first.x).toBeCloseTo(galeri.left + mid.left.x, 2);
    expect(surumler.left + first.x + width(surumler) * first.scale).toBeCloseTo(
      galeri.right + mid.right.x,
      2,
    );
  });

  it("does nothing when nothing moves or the bar has no width", () => {
    expect(underlineFrames(PLATE_SPRING, atRest, width(galeri))).toBeNull();
    expect(underlineFrames(PLATE_SPRING, toGaleri, 0)).toBeNull();
  });
});
