import { describe, expect, it } from "vitest";
import { defaultInstanceName } from "./default-name";

describe("defaultInstanceName", () => {
  it("uses the loader and version when the name is free", () => {
    expect(defaultInstanceName("Fabric 1.21.4", ["Projects", "Fabric 1.21.1"])).toBe(
      "Fabric 1.21.4",
    );
  });

  it("numbers the name when it is taken", () => {
    expect(defaultInstanceName("Fabric 1.21.4", ["Fabric 1.21.4"])).toBe("Fabric 1.21.4 (2)");
    expect(
      defaultInstanceName("Fabric 1.21.4", ["Fabric 1.21.4", "Fabric 1.21.4 (2)", "Diğer"]),
    ).toBe("Fabric 1.21.4 (3)");
  });

  it("compares names the way players read them", () => {
    expect(defaultInstanceName("Minecraft 1.21.4", [" minecraft 1.21.4 "])).toBe(
      "Minecraft 1.21.4 (2)",
    );
    // Turkish casing: "İ" and "i" are the same letter.
    expect(defaultInstanceName("İlk 1.20", ["ilk 1.20"])).toBe("İlk 1.20 (2)");
  });
});
