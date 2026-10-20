import {
  glideFrames,
  type OptionBox,
  PLATE_SPRING,
  type PlateDrift,
  plateAt,
  rebase,
  SEGMENTED_GLIDE_MS,
  settleTime,
  springAt,
} from "@pl/ui";
import { describe, expect, it } from "vitest";

// The gliding plate of `Segmented` (MOTION1, packages/ui/src/glide.ts).
const still = { x: 0, v: 0 };
const atRest: PlateDrift = { left: still, right: still, top: still };

/** The x and y of a keyframe's translate. */
function move(frame: Keyframe | undefined) {
  const found = /translate\((-?[\d.]+)px, (-?[\d.]+)px\)/.exec(String(frame?.transform));
  return { x: Number(found?.[1]), y: Number(found?.[2]) };
}

/** The create dialog's options: "Sıfırdan", "Hazır set", "Mod paketi bul", "Dosyadan içe aktar". */
const sifirdan: OptionBox = { left: 0, right: 67, top: 0, bottom: 26 };
const hazir: OptionBox = { left: 67, right: 140, top: 0, bottom: 26 };
const dosyadan: OptionBox = { left: 248, right: 380, top: 0, bottom: 26 };
/** A glide from "Sıfırdan" to "Dosyadan içe aktar", across the two between. */
const across: PlateDrift = rebase(atRest, sifirdan, dosyadan);

describe("springAt", () => {
  it("solves the spring exactly, speed included", () => {
    expect(springAt(PLATE_SPRING, { x: 100, v: 0 }, 0)).toEqual({ x: 100, v: 0 });
    for (const spring of [
      PLATE_SPRING,
      { stiffness: 400, damping: 40 },
      { stiffness: 300, damping: 60 },
    ]) {
      for (const t of [0.01, 0.05, 0.12, 0.3]) {
        const h = 1e-6;
        const before = springAt(spring, { x: 100, v: -400 }, t - h).x;
        const after = springAt(spring, { x: 100, v: -400 }, t + h).x;
        expect(springAt(spring, { x: 100, v: -400 }, t).v).toBeCloseTo(
          (after - before) / (2 * h),
          2,
        );
      }
      expect(Math.abs(springAt(spring, { x: 100, v: -400 }, 1.5).x)).toBeLessThan(0.05);
    }
  });

  it("barely overshoots: the plate never swings past the option", () => {
    let lowest = 0;
    for (let t = 0; t < 1; t += 0.001) {
      lowest = Math.min(lowest, springAt(PLATE_SPRING, { x: 248, v: 0 }, t).x);
    }
    expect(lowest).toBeGreaterThan(-1.5);
  });
});

describe("plate glide timing", () => {
  it("is within a few pixels of the pick by SEGMENTED_GLIDE_MS (the language fade waits that long)", () => {
    const now = plateAt(PLATE_SPRING, across, SEGMENTED_GLIDE_MS / 1000);
    expect(Math.abs(now.left.x)).toBeLessThan(0.06 * Math.abs(across.left.x));
    expect(Math.abs(now.right.x)).toBeLessThan(0.06 * Math.abs(across.right.x));
  });

  it("ends soon after, and at once when there is nothing to do", () => {
    const seconds = settleTime(PLATE_SPRING, across);
    expect(seconds).toBeGreaterThan(0.2);
    expect(seconds).toBeLessThan(0.5);
    expect(settleTime(PLATE_SPRING, atRest)).toBe(0);
  });
});

describe("glideFrames", () => {
  const frames = glideFrames(PLATE_SPRING, across);

  it("starts where the plate was and ends at rest under the pick", () => {
    if (!frames) throw new Error("no glide");
    const first = { right: move(frames.right[0]), left: move(frames.left[0]) };
    // Right edge: the window's right layer; left edge: both layers' moves together.
    expect(dosyadan.right + first.right.x).toBeCloseTo(sifirdan.right, 3);
    expect(dosyadan.left + first.right.x + first.left.x).toBeCloseTo(sifirdan.left, 3);
    for (const list of [frames.right, frames.left, frames.ink]) {
      expect(list.at(-1)?.transform).toBe("translate(0px, 0px)");
      expect(list[0]?.offset).toBe(0);
      expect(list.at(-1)?.offset).toBe(1);
    }
    const offsets = frames.right.map((frame) => frame.offset ?? 0);
    expect(offsets.every((offset, i) => i === 0 || offset > (offsets[i - 1] ?? 1))).toBe(true);
    expect(frames.duration).toBeGreaterThan(200);
  });

  it("keeps the labels in ink exactly where the labels are, in every frame", () => {
    if (!frames) throw new Error("no glide");
    frames.ink.forEach((frame, i) => {
      const total = move(frames.right[i]).x + move(frames.left[i]).x + move(frame).x;
      const down = move(frames.right[i]).y + move(frames.left[i]).y + move(frame).y;
      expect(Math.abs(total)).toBeLessThan(0.002);
      expect(Math.abs(down)).toBeLessThan(0.002);
    });
  });

  it("is never narrower than an option on its way", () => {
    if (!frames) throw new Error("no glide");
    frames.right.forEach((frame, i) => {
      const right = dosyadan.right + move(frame).x;
      const left = dosyadan.left + move(frame).x + move(frames.left[i]).x;
      expect(right - left).toBeGreaterThan(66);
    });
  });

  it("slides along the row: a glide within one row never moves up or down", () => {
    if (!frames) throw new Error("no glide");
    for (const list of [frames.right, frames.left, frames.ink]) {
      expect(list.every((frame) => move(frame).y === 0)).toBe(true);
    }
  });

  it("has nothing to do when the plate is already there", () => {
    expect(glideFrames(PLATE_SPRING, atRest)).toBeNull();
  });
});

describe("rebase", () => {
  it("picks a glide up where the plate is, with the speed it has", () => {
    // On its way to "Dosyadan içe aktar", over "Hazır set", the player picks "Hazır set".
    const now = plateAt(PLATE_SPRING, across, 0.06);
    const turned = rebase(now, dosyadan, hazir);
    expect(hazir.left + turned.left.x).toBeCloseTo(dosyadan.left + now.left.x, 6);
    expect(hazir.right + turned.right.x).toBeCloseTo(dosyadan.right + now.right.x, 6);
    expect(turned.left.v).toBe(now.left.v);
    expect(turned.right.v).toBe(now.right.v);
    const frames = glideFrames(PLATE_SPRING, turned);
    expect(move(frames?.right[0]).x).toBeCloseTo(turned.right.x, 3);
    // It keeps going right for a moment before it turns back: no jump, no sudden stop.
    expect(move(frames?.right[1]).x).toBeGreaterThan(move(frames?.right[0]).x);
  });
});
