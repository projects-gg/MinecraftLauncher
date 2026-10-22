import { describe, expect, it, vi } from "vitest";
import type { InstanceSummary, OfficialStatus } from "@/lib/api";

// The library's helpers come with the backend's API: stubbed, nothing here calls it.
vi.mock("@/lib/api", () => ({ api: {} }));

const { officialMissing, overlayLine } = await import("./line");

function instance(id: string, extra: Partial<InstanceSummary> = {}): InstanceSummary {
  return {
    id,
    name: id,
    icon: "grass",
    mcVersion: "26.1.2",
    loader: "fabric",
    managed: false,
    playTimeSecs: 0,
    state: "ready",
    ...extra,
  };
}

const status = (extra: Partial<OfficialStatus> = {}): OfficialStatus => ({
  pack: { id: "projects", name: "Projects", version: "v13", mcVersion: "26.1.2", loader: "fabric" },
  removed: false,
  ...extra,
});

describe("the overlay switch's line", () => {
  it("says how it opens while it is off, whatever else is true", () => {
    expect(overlayLine(false, false, false)).toBe(0);
    expect(overlayLine(false, true, true)).toBe(0);
  });

  it("says why it will not work while it is on, the missing pack first", () => {
    expect(overlayLine(true, false, false)).toBe(0);
    expect(overlayLine(true, true, false)).toBe(1);
    expect(overlayLine(true, false, true)).toBe(2);
    expect(overlayLine(true, true, true)).toBe(2);
  });
});

describe("the official pack behind the overlay", () => {
  it("is missing once no instance follows it", () => {
    expect(officialMissing([], undefined)).toBe(true);
    expect(officialMissing([instance("vanilla")], status({ removed: true }))).toBe(true);
  });

  it("is there with the official instance, a copy, or the one the status names", () => {
    expect(officialMissing([instance("projects", { managed: true })], status())).toBe(false);
    expect(officialMissing([instance("kopya", { managed: true })], status({ removed: true }))).toBe(
      false,
    );
    expect(officialMissing([instance("projects")], status({ instanceId: "projects" }))).toBe(false);
  });

  it("counts as there while either list is unknown", () => {
    expect(officialMissing(undefined, status({ removed: true }))).toBe(false);
    expect(officialMissing([instance("vanilla")], undefined)).toBe(false);
  });
});
