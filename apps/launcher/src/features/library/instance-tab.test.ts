import { describe, expect, it } from "vitest";
import { instanceTab } from "./instance-tab";

describe("the tab a link opens", () => {
  it("opens the tab the link asks for", () => {
    expect(instanceTab("mods", false)).toBe("mods");
    expect(instanceTab("settings", true)).toBe("settings");
    // A vanilla instance still has a mods tab (it says a loader is needed).
    expect(instanceTab("mods", true)).toBe("mods");
    expect(instanceTab("modcenter", false)).toBe("modcenter");
  });

  it("falls back to the overview, never to no tab at all", () => {
    expect(instanceTab(undefined, false)).toBe("overview");
    expect(instanceTab("", false)).toBe("overview");
    expect(instanceTab("mods?emptyContent=1", false)).toBe("overview");
    expect(instanceTab("Mods", false)).toBe("overview");
    expect(instanceTab(3, false)).toBe("overview");
    // No mod center without a mod loader.
    expect(instanceTab("modcenter", true)).toBe("overview");
  });
});
