import { describe, expect, it } from "vitest";
import type { OfficialFamily } from "@/lib/api";
import { newerMembers, offerSeenKey, packOffer } from "./pack-offer";

// Made-up versions: the rules never depend on a real one.
const member = (minecraft: string, packId: string, isDefault = false) => ({
  packId,
  name: "Pack",
  version: "1.0.0",
  minecraft,
  default: isDefault,
});
const family = (over: Partial<OfficialFamily> = {}): OfficialFamily => ({
  members: [member("9.3", "pack-9.3"), member("9.2", "pack-9.2"), member("9.1", "pack", true)],
  matched: { id: "pack-9.3", name: "Pack", version: "1.0.0", mcVersion: "9.3", loader: "fabric" },
  removed: false,
  ...over,
});

describe("packOffer", () => {
  it("offers nothing without a member for the picked version and loader", () => {
    expect(packOffer(undefined, { minecraft: "9.3", loader: "fabric" })).toBeNull();
    expect(
      packOffer(family({ matched: undefined }), { minecraft: "9.3", loader: "fabric" }),
    ).toBeNull();
    expect(packOffer(family(), { minecraft: "9.3", loader: "quilt" })).toBeNull();
    expect(packOffer(family(), { minecraft: "9.4", loader: "fabric" })).toBeNull();
    expect(packOffer(family(), { minecraft: "", loader: "fabric" })).toBeNull();
  });

  it("installs when there is no official instance (deleted or never made)", () => {
    const offer = packOffer(family({ removed: true }), { minecraft: "9.3", loader: "fabric" });
    expect(offer).toMatchObject({ kind: "install", pack: { id: "pack-9.3" } });
  });

  it("switches the official instance when it is on another version", () => {
    const offer = packOffer(family({ instanceId: "projects", instanceMinecraft: "9.1" }), {
      minecraft: "9.3",
      loader: "fabric",
    });
    expect(offer).toEqual(
      expect.objectContaining({ kind: "switch", instanceId: "projects", from: "9.1" }),
    );
  });

  it("opens the official instance when it is already there, and never offers it to itself", () => {
    const there = family({ instanceId: "projects", instanceMinecraft: "9.3" });
    expect(packOffer(there, { minecraft: "9.3", loader: "fabric" })?.kind).toBe("open");
    expect(packOffer(there, { minecraft: "9.3", loader: "fabric" }, "projects")).toBeNull();
    expect(packOffer(there, { minecraft: "9.3", loader: "fabric" }, "mine")?.kind).toBe("open");
  });
});

describe("newerMembers", () => {
  it("lists the newer members not seen yet, newest first", () => {
    expect(newerMembers(family(), "9.1", []).map((m) => m.packId)).toEqual([
      "pack-9.3",
      "pack-9.2",
    ]);
    expect(newerMembers(family(), "9.1", ["pack-9.3"]).map((m) => m.packId)).toEqual(["pack-9.2"]);
    expect(newerMembers(family(), "9.2", []).map((m) => m.packId)).toEqual(["pack-9.3"]);
  });

  it("is quiet on the newest member, off the family and without a family", () => {
    expect(newerMembers(family(), "9.3", [])).toEqual([]);
    expect(newerMembers(family(), "8.0", [])).toEqual([]);
    expect(newerMembers(undefined, "9.1", [])).toEqual([]);
  });
});

it("keys a hidden offer by instance and pack", () => {
  expect(offerSeenKey("mine", "pack-9.3")).toBe("mine:pack-9.3");
});
