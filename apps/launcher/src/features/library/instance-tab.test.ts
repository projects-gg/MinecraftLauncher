import { describe, expect, it } from "vitest";
import { askedModsSection, instanceTab, modsSections, pickModsSection } from "./instance-tab";

describe("the tab a link opens", () => {
  it("opens the tab the link asks for", () => {
    expect(instanceTab("mods")).toBe("mods");
    expect(instanceTab("settings")).toBe("settings");
  });

  it("opens Modlar for the old mod center and the Modrinth browser for mods", () => {
    expect(instanceTab("modcenter")).toBe("mods");
    expect(instanceTab(undefined, "mod")).toBe("mods");
    expect(instanceTab("overview", "mod")).toBe("mods");
    // The browsers for packs and shaders still open in place of their own tabs.
    expect(instanceTab("resourcePacks", "resourcePack")).toBe("resourcePacks");
  });

  it("falls back to the overview, never to no tab at all", () => {
    expect(instanceTab(undefined)).toBe("overview");
    expect(instanceTab("")).toBe("overview");
    expect(instanceTab("mods?emptyContent=1")).toBe("overview");
    expect(instanceTab("Mods")).toBe("overview");
    expect(instanceTab(3)).toBe("overview");
    expect(instanceTab("toString")).toBe("overview");
  });
});

describe("the sections of Modlar", () => {
  it("puts the official pack's recommended mods first and finding more last", () => {
    expect(modsSections(true)).toEqual(["recommended", "own", "sets", "discover"]);
    expect(modsSections(false)).toEqual(["own", "sets", "discover"]);
  });

  it("reads the section a link asks for", () => {
    expect(askedModsSection("sets")).toBe("sets");
    expect(askedModsSection("own")).toBe("own");
    expect(askedModsSection(undefined, "mod")).toBe("discover");
    expect(askedModsSection("graphics")).toBeUndefined();
    expect(askedModsSection(undefined)).toBeUndefined();
  });

  it("opens the asked section, then the remembered one, then the first", () => {
    expect(pickModsSection(true)).toBe("recommended");
    expect(pickModsSection(false)).toBe("own");
    expect(pickModsSection(true, undefined, "sets")).toBe("sets");
    expect(pickModsSection(true, "discover", "sets")).toBe("discover");
    // Only the official pack has recommended mods.
    expect(pickModsSection(false, "recommended")).toBe("own");
    expect(pickModsSection(false, "recommended", "sets")).toBe("sets");
  });
});
