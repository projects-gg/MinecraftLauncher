import { describe, expect, it } from "vitest";
import { keptOrder, summarizeChanges } from "./modpack-changes";

const change = (path: string, name: string) => ({
  path,
  name,
  folder: path.includes("/") ? (path.split("/")[0] as string) : "",
});

describe("modpack update lists", () => {
  it("names add-ons and counts other files", () => {
    const summary = summarizeChanges(
      [
        change("mods/sodium.jar", "Sodium"),
        change("config/sodium.json", "sodium.json"),
        change("mods/iris.jar", "Iris Shaders"),
        change("options.txt", "options.txt"),
        change("shaderpacks/c.zip", "Complementary"),
      ],
      2,
    );
    expect(summary.named.map((c) => c.name)).toEqual(["Complementary", "Iris Shaders"]);
    expect(summary.more).toBe(1);
    expect(summary.files).toBe(2);
    expect(summarizeChanges([])).toEqual({ named: [], more: 0, files: 0 });
  });

  it("orders kept files", () => {
    const reasons = (["theirs", "changed", "disabled", "removed"] as const)
      .slice()
      .sort((a, b) => keptOrder(a) - keptOrder(b));
    expect(reasons).toEqual(["disabled", "removed", "changed", "theirs"]);
  });
});
