import { describe, expect, it, vi } from "vitest";
import type { InstanceSummary, OfficialStatus } from "@/lib/api";

// The library's helpers come with the backend's API: stubbed, nothing here calls it.
vi.mock("@/lib/api", () => ({ api: {} }));

const {
  carriesOverlay,
  OVERLAY_LINES,
  officialMissing,
  overlayHasNoHome,
  overlayHome,
  overlayLine,
  shipsFor,
} = await import("./line");

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

const VERSIONS = ["26.1.2", "26.2", "26.3"];
const hoursAgo = (hours: number) => new Date(Date.now() - hours * 3_600_000).toISOString();

const projects = instance("projects", { managed: true, lastPlayedAt: hoursAgo(20) });
const vanilla = instance("vanilla", { loader: "vanilla", mcVersion: "26.3" });
const forge = instance("hardcore", { loader: "forge", mcVersion: "1.20.1" });
const oldFabric = instance("yaratici", { mcVersion: "1.21.1", lastPlayedAt: hoursAgo(2) });
const ownFabric = instance("survival", { mcVersion: "26.2.1", lastPlayedAt: hoursAgo(5) });

/** The line with everything known: the official pack there, the switch on, Play on `play`. */
function line(
  play: InstanceSummary | undefined,
  extra: Partial<Parameters<typeof overlayLine>[0]> = {},
) {
  return overlayLine({
    on: true,
    leaves: false,
    instances: [projects, vanilla, forge, oldFabric],
    official: status({ instanceId: "projects" }),
    versions: VERSIONS,
    play,
    locked: false,
    ...extra,
  });
}

describe("the overlay switch's line", () => {
  it("says how it opens while it is off, whatever else is true", () => {
    expect(line(vanilla, { on: false })).toEqual({ kind: "opens" });
    expect(
      line(vanilla, { on: false, leaves: true, instances: [vanilla], official: status() }),
    ).toEqual({ kind: "opens" });
  });

  it("says how it opens while the install Play opens carries it", () => {
    expect(line(projects)).toEqual({ kind: "opens" });
    expect(line(ownFabric, { instances: [projects, ownFabric] })).toEqual({ kind: "opens" });
  });

  it("puts the most basic blocker first: no home, then Kapansın, then the install", () => {
    // No official pack and no Fabric install of a shipped version: "Kur" before anything else.
    const homeless = {
      instances: [vanilla, forge, oldFabric],
      official: status({ removed: true }),
    };
    expect(line(vanilla, homeless).kind).toBe("noHome");
    expect(line(vanilla, { ...homeless, leaves: true }).kind).toBe("noHome");
    // The launcher closes with the game: no install gets it, whichever Play opens.
    expect(line(vanilla, { leaves: true })).toEqual({ kind: "closes" });
    expect(line(projects, { leaves: true })).toEqual({ kind: "closes" });
    // Then the install itself.
    expect(line(vanilla).kind).toBe("notFabric");
  });

  it("tells an install of another loader from a Fabric install of another version", () => {
    expect(line(vanilla).kind).toBe("notFabric");
    expect(line(forge).kind).toBe("notFabric");
    expect(line(instance("neo", { loader: "neoForge", mcVersion: "26.1.2" })).kind).toBe(
      "notFabric",
    );
    expect(line(instance("q", { loader: "quilt", mcVersion: "26.1.2" })).kind).toBe("notFabric");
    expect(line(oldFabric).kind).toBe("version");
    expect(line(instance("next", { mcVersion: "26.4" })).kind).toBe("version");
    expect(line(instance("snap", { mcVersion: "26.2-pre1" })).kind).toBe("version");
  });

  it("offers the official pack to switch to", () => {
    expect(line(vanilla).switchTo?.id).toBe("projects");
    expect(line(oldFabric).switchTo?.id).toBe("projects");
  });

  it("keeps the switch waiting while a game is prepared or open", () => {
    expect(line(vanilla, { locked: true })).toEqual({
      kind: "notFabric",
      switchTo: projects,
      locked: true,
    });
    expect(line(vanilla).locked).toBe(false);
  });

  it("offers nothing when no install carries it but the official pack is there", () => {
    // The pack on a version the mod is not shipped for yet (OVL-AUTO's case).
    const ahead = instance("projects", { managed: true, mcVersion: "26.4" });
    expect(
      line(ahead, { instances: [ahead, vanilla], official: status({ instanceId: "projects" }) }),
    ).toEqual({ kind: "version", switchTo: undefined, locked: false });
  });

  it("decides nothing about the install while a list is unknown", () => {
    expect(line(vanilla, { versions: undefined })).toEqual({ kind: "opens" });
    expect(line(vanilla, { official: undefined })).toEqual({ kind: "opens" });
    expect(line(vanilla, { instances: undefined })).toEqual({ kind: "opens" });
    expect(line(undefined)).toEqual({ kind: "opens" });
    // "Kapansın" needs no list.
    expect(line(vanilla, { versions: undefined, leaves: true })).toEqual({ kind: "closes" });
  });

  it("names every state the row draws", () => {
    expect([...OVERLAY_LINES]).toEqual(["opens", "noHome", "closes", "notFabric", "version"]);
  });
});

describe("the install the line's switch picks", () => {
  it("is the instance the status names before a copy and the player's own", () => {
    const copy = instance("kopya", { managed: true, lastPlayedAt: hoursAgo(1) });
    const recent = instance("own", { mcVersion: "26.3", lastPlayedAt: hoursAgo(0.5) });
    expect(
      overlayHome([recent, copy, projects], status({ instanceId: "projects" }), VERSIONS)?.id,
    ).toBe("projects");
    expect(overlayHome([recent, copy], status({ removed: true }), VERSIONS)?.id).toBe("kopya");
  });

  it("is the last played of the player's own when the pack is gone", () => {
    const later = instance("later", { mcVersion: "26.3", lastPlayedAt: hoursAgo(1) });
    const never = instance("never", { mcVersion: "26.2" });
    expect(overlayHome([never, ownFabric, later], status({ removed: true }), VERSIONS)?.id).toBe(
      "later",
    );
    expect(overlayHome([never, vanilla], status({ removed: true }), VERSIONS)?.id).toBe("never");
  });

  it("skips installs that cannot carry it, the pack too", () => {
    const ahead = instance("projects", { managed: true, mcVersion: "26.4" });
    expect(
      overlayHome([ahead, vanilla, forge, oldFabric], status({ instanceId: "projects" }), VERSIONS),
    ).toBeUndefined();
  });

  it("switches to it from the line, which then says how it opens", () => {
    const instances = [vanilla, ownFabric];
    const official = status({ removed: true });
    const before = line(vanilla, { instances, official });
    expect(before).toEqual({ kind: "notFabric", switchTo: ownFabric, locked: false });
    expect(line(before.switchTo, { instances, official })).toEqual({ kind: "opens" });
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

describe("the instances the launcher puts the overlay in", () => {
  it("match the jars' ranges: the same minor, that patch or a later one", () => {
    expect(shipsFor("26.2", "26.2")).toBe(true);
    expect(shipsFor("26.2", "26.2.1")).toBe(true);
    expect(shipsFor("26.1.2", "26.1.2")).toBe(true);
    expect(shipsFor("26.1.2", "26.1.3")).toBe(true);
    expect(shipsFor("26.1.2", "26.1.1")).toBe(false);
    expect(shipsFor("26.1.2", "26.1")).toBe(false);
    expect(shipsFor("26.2", "26.20")).toBe(false);
    expect(shipsFor("26.2", "26.3")).toBe(false);
    expect(shipsFor("26.2", "27.2")).toBe(false);
    expect(shipsFor("26.2", "26.2-pre1")).toBe(false);
    expect(shipsFor("26.2", "1.21.11")).toBe(false);
  });

  it("are Fabric instances on a shipped version, official or the player's own", () => {
    const official = status({ removed: true });
    expect(carriesOverlay(instance("own", { mcVersion: "26.2" }), official, VERSIONS)).toBe(true);
    expect(carriesOverlay(instance("own", { mcVersion: "26.2.1" }), official, VERSIONS)).toBe(true);
    expect(
      carriesOverlay(instance("forge", { mcVersion: "26.2", loader: "forge" }), official, VERSIONS),
    ).toBe(false);
    expect(carriesOverlay(instance("old", { mcVersion: "1.21.11" }), official, VERSIONS)).toBe(
      false,
    );
    expect(carriesOverlay(instance("own", { mcVersion: "26.20" }), official, VERSIONS)).toBe(false);
  });

  it("are only the official pack's in a build without the jars (its own item)", () => {
    const official = status({ instanceId: "projects" });
    expect(carriesOverlay(instance("own"), official, [])).toBe(false);
    expect(carriesOverlay(instance("projects"), official, [])).toBe(true);
    expect(carriesOverlay(instance("kopya", { managed: true }), official, [])).toBe(true);
  });

  it("leave the pack line out while one of the player's own can carry it", () => {
    const own = [instance("own", { mcVersion: "26.3" })];
    expect(overlayHasNoHome(own, status({ removed: true }), VERSIONS)).toBe(false);
    expect(
      overlayHasNoHome(
        [instance("vanilla", { loader: "vanilla" })],
        status({ removed: true }),
        VERSIONS,
      ),
    ).toBe(true);
    // A build without the jars: only the official pack carries it.
    expect(overlayHasNoHome(own, status({ removed: true }), [])).toBe(true);
    // Not known yet: no line, as with unknown lists.
    expect(overlayHasNoHome([], undefined, undefined)).toBe(false);
  });
});
