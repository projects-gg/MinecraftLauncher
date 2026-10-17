import { describe, expect, it } from "vitest";
import { withoutMovement } from "./calm";

describe("withoutMovement", () => {
  it("keeps fades and timing, drops every movement", () => {
    const transition = { duration: 0.2 };
    expect(
      withoutMovement({
        opacity: 0,
        x: "100%",
        y: -4,
        scale: 0.96,
        rotate: 30,
        filter: "blur(3px)",
        transition,
      }),
    ).toEqual({ opacity: 0, transition });
  });

  it("leaves labels and switches alone", () => {
    expect(withoutMovement(false)).toBe(false);
    expect(withoutMovement("lifted")).toBe("lifted");
    expect(withoutMovement(undefined)).toBeUndefined();
  });
});
