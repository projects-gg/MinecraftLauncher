import { describe, expect, it } from "vitest";
import { baseFileName, modRowKeys } from "./mod-keys";

describe("mod row keys", () => {
  it("keeps a row's key when its mod is switched off or on", () => {
    expect(modRowKeys(["sodium.jar", "iris.jar"])).toEqual(
      new Map([
        ["sodium.jar", "sodium.jar"],
        ["iris.jar", "iris.jar"],
      ]),
    );
    expect(modRowKeys(["sodium.jar.disabled"]).get("sodium.jar.disabled")).toBe("sodium.jar");
    expect(baseFileName("sodium.jar.disabled")).toBe("sodium.jar");
    expect(baseFileName("disabled-mod.jar")).toBe("disabled-mod.jar");
  });

  it("gives both copies of a mod a row of their own", () => {
    const keys = modRowKeys(["sodium.jar", "sodium.jar.disabled", "iris.jar"]);
    expect(keys.get("sodium.jar")).toBe("sodium.jar");
    expect(keys.get("sodium.jar.disabled")).toBe("sodium.jar.disabled");
    expect(keys.get("iris.jar")).toBe("iris.jar");
  });

  it("never repeats a key, whatever the names", () => {
    const names = [
      "a.jar",
      "a.jar.disabled",
      "a.jar.disabled.disabled",
      "b.jar.disabled",
      "b.jar.disabled.disabled",
    ];
    const keys = [...modRowKeys(names).values()];
    expect(keys).toHaveLength(names.length);
    expect(new Set(keys).size).toBe(names.length);
  });
});
