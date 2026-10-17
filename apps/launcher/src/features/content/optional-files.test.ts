import { describe, expect, it } from "vitest";
import { optionalFolder, optionalTotals, prettyFileName } from "./optional-files";

describe("optional pack files", () => {
  it("names files readably", () => {
    expect(prettyFileName("distanthorizons-2.3.0-b-1.21.1.jar")).toBe("Distanthorizons");
    expect(prettyFileName("zoomify-2.14.1.jar")).toBe("Zoomify");
    expect(prettyFileName("ComplementaryReimagined_r5.4.zip")).toBe("ComplementaryReimagined r5.4");
    expect(prettyFileName("sodium-fabric-0.6.5+mc1.21.1.jar")).toBe("Sodium fabric");
    expect(prettyFileName("iris_mc1.21.1.jar")).toBe("Iris");
    expect(prettyFileName("2fa.jar")).toBe("2fa");
  });

  it("groups folders", () => {
    expect(optionalFolder("mods")).toBe("mods");
    expect(optionalFolder("shaderpacks")).toBe("shaderpacks");
    expect(optionalFolder("config/extra")).toBe("other");
  });

  it("counts what is still downloaded", () => {
    const preview = {
      downloadFiles: 10,
      downloadBytes: 1000,
      optional: [
        { path: "mods/a.jar", fileName: "a.jar", folder: "mods", size: 300 },
        { path: "mods/b.jar", fileName: "b.jar", folder: "mods", size: 200 },
      ],
    };
    expect(optionalTotals(preview, new Set())).toEqual({ files: 10, bytes: 1000 });
    expect(optionalTotals(preview, new Set(["mods/a.jar"]))).toEqual({ files: 9, bytes: 700 });
    expect(optionalTotals(preview, new Set(["mods/a.jar", "mods/b.jar"]))).toEqual({
      files: 8,
      bytes: 500,
    });
  });
});
