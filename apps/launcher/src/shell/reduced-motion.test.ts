import { stillKeyframes } from "@pl/ui";
import { describe, expect, it } from "vitest";

// Under reduced motion every Web Animation holds its movement at the final value (POLISH3).
describe("stillKeyframes", () => {
  it("holds motion's transform strings at their end", () => {
    // The hero copy's first arrival: it lands at once and fades in its own animation.
    expect(stillKeyframes({ transform: ["translateY(16px)", "translateY(0px)"] })).toEqual({
      transform: ["translateY(0px)", "translateY(0px)"],
    });
    // One value still animates from the current one, so it becomes two.
    expect(stillKeyframes({ scale: "1.05" })).toEqual({ scale: ["1.05", "1.05"] });
  });

  it("keeps a fade in the same animation as written", () => {
    expect(
      stillKeyframes({ opacity: [0, 1], transform: ["scale(0.6)", "scale(1)"], offset: [0, 1] }),
    ).toEqual({ opacity: [0, 1], transform: ["scale(1)", "scale(1)"], offset: [0, 1] });
    expect(
      stillKeyframes([
        { opacity: 0, transform: "translateX(24px)" },
        { opacity: 1, transform: "translateX(0px)" },
      ]),
    ).toEqual([
      { opacity: 0, transform: "translateX(0px)" },
      { opacity: 1, transform: "translateX(0px)" },
    ]);
  });

  it("gives every frame the end value, also those that left it out", () => {
    expect(
      stillKeyframes([{ opacity: 0 }, { opacity: 0.5, translate: "0 6px" }, { translate: "0 0" }]),
    ).toEqual([
      { opacity: 0, translate: "0 0" },
      { opacity: 0.5, translate: "0 0" },
      { translate: "0 0" },
    ]);
    expect(stillKeyframes([{ rotate: "90deg" }])).toEqual([
      { rotate: "90deg" },
      { rotate: "90deg" },
    ]);
  });

  it("leaves animations that do not move alone", () => {
    // Fades, colours and the entrance gate's probe (opacity held at 1 %).
    expect(stillKeyframes({ opacity: [0.01, 0.01] })).toBeNull();
    expect(stillKeyframes([{ backgroundColor: "red" }, { backgroundColor: "blue" }])).toBeNull();
    expect(stillKeyframes(null)).toBeNull();
  });
});
