import { describe, expect, it, vi } from "vitest";

vi.mock("@/bindings", () => ({ commands: { updateRestartInfo: vi.fn(async () => null) } }));
vi.mock("@/lib/platform", () => ({ isTauri: () => false }));

const { chooseNotes, landingRoute } = await import("./after-update");

/** What the backend hands over after the restart. */
const left = (
  change: Partial<{ version: string; notes: string | null; route: string | null }>,
) => ({
  version: "2.5.0",
  notes: null,
  route: null,
  ...change,
});

describe("chooseNotes", () => {
  const stored = { version: "2.5.0", notes: "- eski kopya" };

  it("prefers the restart note's notes", () => {
    expect(chooseNotes(left({ notes: "- yeni" }), stored)).toEqual({
      version: "2.5.0",
      notes: "- yeni",
    });
  });

  it("falls back to the stored copy (an older launcher installed this version)", () => {
    expect(chooseNotes(null, stored)).toEqual(stored);
    expect(chooseNotes(left({ notes: "  " }), stored)).toEqual(stored);
  });

  it("shows nothing without notes", () => {
    expect(chooseNotes(left({}), null)).toBeNull();
    expect(chooseNotes(null, null)).toBeNull();
  });
});

describe("landingRoute", () => {
  it("returns to the page the player left", () => {
    expect(landingRoute(left({ route: "/afk" }), "/")).toBe("/afk");
    expect(landingRoute(left({ route: "/settings?section=updates" }), "/")).toBe(
      "/settings?section=updates",
    );
  });

  it("leaves a player who already went elsewhere where they are", () => {
    expect(landingRoute(left({ route: "/afk" }), "/library")).toBeNull();
  });

  it("ignores the start page and anything that is not a launcher path", () => {
    expect(landingRoute(left({ route: "/" }), "/")).toBeNull();
    expect(landingRoute(left({ route: "//evil.example" }), "/")).toBeNull();
    expect(landingRoute(left({ route: "afk" }), "/")).toBeNull();
    expect(landingRoute(left({}), "/")).toBeNull();
    expect(landingRoute(null, "/")).toBeNull();
  });
});
