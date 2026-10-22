import { beforeEach, describe, expect, it, vi } from "vitest";
import type { InstanceSummary, OfficialStatus } from "@/lib/api";

// LIB1: the official instance once it can be deleted. The backend and the launch around the
// join helpers are stubbed.
const instancesApi = vi.fn();
const ensureOfficialInstance = vi.fn();
const play = vi.fn(async () => {});
const selectInstance = vi.fn();
const showError = vi.fn();
vi.mock("@/lib/api", () => ({
  api: {
    instances: () => instancesApi(),
    ensureOfficialInstance: () => ensureOfficialInstance(),
  },
}));
vi.mock("@/stores/launch", () => ({
  useLaunch: { getState: () => ({ phase: { kind: "idle" }, play }) },
}));
vi.mock("@/stores/prefs", () => ({
  usePrefs: { getState: () => ({ selectInstance, selectedInstanceId: null }) },
}));
vi.mock("@/features/content/jobs", () => ({ showError: (error: unknown) => showError(error) }));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));

const {
  deletesLastOfficial,
  hasOfficial,
  libraryOffer,
  officialOrOffer,
  playTarget,
  useOfficialOffer,
} = await import("./official");
const { joinProjects, joinServer } = await import("@/features/social/join");

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

const projects = instance("projects", { name: "Projects", managed: true, icon: "dragon-egg" });
const copy = instance("projects-kopya", { name: "Projects (kopya)", managed: true });
const vanilla = instance("vanilla", { loader: "vanilla" });
const status = (extra: Partial<OfficialStatus> = {}): OfficialStatus => ({
  pack: {
    id: "projects",
    name: "Projects",
    version: "v13",
    mcVersion: "26.1.2",
    loader: "fabric",
  },
  removed: false,
  ...extra,
});

describe("the library's offer", () => {
  it("shows the empty state once no instance is left", () => {
    expect(libraryOffer([], status({ removed: true }), false)).toBe("empty");
    // Even before the status answers, and while searching: there is nothing to search.
    expect(libraryOffer([], undefined, true)).toBe("empty");
    expect(libraryOffer(undefined, status(), false)).toBeNull();
  });

  it("offers the pack where it was when only the official instance is missing", () => {
    expect(libraryOffer([vanilla], status({ removed: true }), false)).toBe("official");
    // Never deleted but missing (offline at the first start): the same quiet offer.
    expect(libraryOffer([vanilla], status(), false)).toBe("official");
    // Not while searching, and not before the status is known.
    expect(libraryOffer([vanilla], status(), true)).toBeNull();
    expect(libraryOffer([vanilla], undefined, false)).toBeNull();
  });

  it("offers nothing while an official instance is there, a copy included", () => {
    expect(libraryOffer([projects, vanilla], status({ instanceId: "projects" }), false)).toBeNull();
    // Installed a moment ago: the list knows before the status does.
    expect(libraryOffer([vanilla, projects], status({ removed: true }), false)).toBeNull();
    expect(libraryOffer([copy], status(), false)).toBeNull();
    // Before any pack: the vanilla "Projects" instance the status names.
    const vanillaProjects = instance("projects", { name: "Projects", loader: "vanilla" });
    expect(
      libraryOffer([vanillaProjects], { removed: false, instanceId: "projects" }, false),
    ).toBeNull();
    expect(hasOfficial([vanilla], status())).toBe(false);
  });
});

describe("the delete confirmation", () => {
  it("says the pack can come back only when the last official instance goes", () => {
    const all = [projects, copy, vanilla];
    expect(deletesLastOfficial(all, status({ instanceId: "projects" }), "projects")).toBe(false);
    expect(deletesLastOfficial(all, status({ instanceId: "projects" }), "projects-kopya")).toBe(
      false,
    );
    expect(deletesLastOfficial(all, status({ instanceId: "projects" }), "vanilla")).toBe(false);
    expect(
      deletesLastOfficial(
        [copy, vanilla],
        status({ instanceId: "projects-kopya" }),
        "projects-kopya",
      ),
    ).toBe(true);
    expect(deletesLastOfficial([projects], status({ instanceId: "projects" }), "projects")).toBe(
      true,
    );
    // Before any pack: the vanilla "Projects" the status names.
    const vanillaProjects = instance("projects", { name: "Projects", loader: "vanilla" });
    expect(
      deletesLastOfficial(
        [vanillaProjects, vanilla],
        { removed: false, instanceId: "projects" },
        "projects",
      ),
    ).toBe(true);
    expect(deletesLastOfficial([vanilla], status(), "gone")).toBe(false);
  });
});

describe("the instance OYNA starts", () => {
  it("is the selected one while it exists, else the first", () => {
    expect(playTarget([vanilla, projects], "projects")?.id).toBe("projects");
    // The selected official instance was deleted.
    expect(playTarget([vanilla], "projects")?.id).toBe("vanilla");
    expect(playTarget([], "projects")).toBeUndefined();
  });
});

describe("actions that need the official instance", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useOfficialOffer.getState().close();
  });

  it("use the one there (or the one made now when the player never deleted it)", async () => {
    ensureOfficialInstance.mockResolvedValue(projects);
    expect(await officialOrOffer({ kind: "play" })).toBe(projects);
    expect(useOfficialOffer.getState().intent).toBeNull();
  });

  it("open the offer instead of making it again after a deletion", async () => {
    ensureOfficialInstance.mockResolvedValue(null);
    expect(await officialOrOffer({ kind: "play" })).toBeNull();
    expect(useOfficialOffer.getState().intent).toEqual({ kind: "play" });
  });

  it("let 'Katıl' offer the pack without an error or a launch", async () => {
    instancesApi.mockResolvedValue([vanilla]);
    ensureOfficialInstance.mockResolvedValue(null);
    await joinProjects();
    expect(play).not.toHaveBeenCalled();
    expect(showError).not.toHaveBeenCalled();
    expect(useOfficialOffer.getState().intent).toEqual({ kind: "join" });
  });

  it("let an invite to another server offer it when there is nothing to start", async () => {
    instancesApi.mockResolvedValue([]);
    ensureOfficialInstance.mockResolvedValue(null);
    await joinServer({ host: "survival.example.net", port: 25570 });
    expect(play).not.toHaveBeenCalled();
    expect(showError).not.toHaveBeenCalled();
    expect(useOfficialOffer.getState().intent).toEqual({
      kind: "joinServer",
      host: "survival.example.net",
      port: 25570,
    });
  });
});
