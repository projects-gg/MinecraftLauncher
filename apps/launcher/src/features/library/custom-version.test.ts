import { describe, expect, it } from "vitest";
import type { CustomVersion, LoaderKind } from "@/lib/api";
import { customBase, customName, gameLine, pickedCustom } from "./custom-version";

const names: Record<LoaderKind, string> = {
  vanilla: "Vanilla",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoForge: "NeoForge",
};
const loaderName = (loader: LoaderKind) => names[loader];

const optifine: CustomVersion = {
  id: "1.21.11-OptiFine_HD_U_J9",
  minecraft: "1.21.11",
  loader: "vanilla",
  kind: "optiFine",
};
const forge: CustomVersion = {
  id: "1.21.11-forge-61.2.5",
  minecraft: "1.21.11",
  loader: "forge",
  loaderVersion: "61.2.5",
  kind: "forge",
};
const iris: CustomVersion = {
  id: "iris-fabric-loader-0.18.1-1.21.11",
  minecraft: "1.21.11",
  loader: "fabric",
  loaderVersion: "0.18.1",
  kind: "fabric",
};

describe("custom versions", () => {
  it("names versions the way players know them", () => {
    expect(customName(optifine, loaderName)).toEqual({
      label: "OptiFine 1.21.11",
      detail: "HD U J9",
      friendly: true,
    });
    expect(customName(forge, loaderName)).toEqual({
      label: "Forge 1.21.11",
      detail: "61.2.5",
      friendly: true,
    });
    expect(
      customName(
        {
          id: "neoforge-21.11.38-beta",
          minecraft: "1.21.11",
          loader: "neoForge",
          loaderVersion: "21.11.38-beta",
        },
        loaderName,
      ).label,
    ).toBe("NeoForge 1.21.11");
    // A version of its own keeps its own name.
    expect(customName(iris, loaderName)).toEqual({ label: iris.id, friendly: false });
  });

  it("names an instance by the version it launches", () => {
    expect(gameLine({ loader: "fabric", mcVersion: "1.21.4" }, loaderName)).toBe("Fabric 1.21.4");
    expect(gameLine({ loader: "vanilla", mcVersion: "1.21.4" }, loaderName)).toBe(
      "Minecraft 1.21.4",
    );
    expect(
      gameLine(
        { loader: "vanilla", mcVersion: "1.21.11", customVersion: "1.21.11-OptiFine_HD_U_J9" },
        loaderName,
      ),
    ).toBe("OptiFine 1.21.11");
    expect(
      gameLine(
        { loader: "fabric", loaderVersion: "0.18.1", mcVersion: "1.21.11", customVersion: iris.id },
        loaderName,
      ),
    ).toBe(iris.id);
  });

  it("says what a version runs on beside Minecraft", () => {
    expect(customBase(optifine)).toBe("optiFine");
    expect(customBase(forge)).toBe("forge");
    expect(customBase({ ...optifine, kind: "other" })).toBeNull();
  });

  it("keeps the pick while it is listed, else the newest", () => {
    expect(pickedCustom([optifine, forge], forge.id)).toBe(forge);
    expect(pickedCustom([optifine, forge], "gone")).toBe(optifine);
    expect(pickedCustom([], "")).toBeUndefined();
  });
});
