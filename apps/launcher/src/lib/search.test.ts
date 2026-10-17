import { describe, expect, it } from "vitest";
import { fold, matchesQuery } from "./search";

describe("fold", () => {
  it("treats every Turkish i the same", () => {
    expect(fold("Iris")).toBe("iris");
    expect(fold("İzmir")).toBe("izmir");
    expect(fold("ırmak")).toBe("irmak");
  });

  it("drops marks so plain letters find Turkish names", () => {
    expect(fold("Şarkı Dünyası")).toBe("sarki dunyasi");
    expect(fold("Çağ")).toBe("cag");
  });
});

describe("matchesQuery", () => {
  it("finds English mod names typed in lower case", () => {
    expect(matchesQuery("Iris Shaders", "iris")).toBe(true);
    expect(matchesQuery("Immersive Portals", "imm")).toBe(true);
  });

  it("matches everything for an empty query and nothing for a stranger", () => {
    expect(matchesQuery("Sodium", "  ")).toBe(true);
    expect(matchesQuery("Sodium", "lith")).toBe(false);
  });
});
