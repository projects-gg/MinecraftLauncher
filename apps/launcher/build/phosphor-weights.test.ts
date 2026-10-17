import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { dropUnusedWeights, USED_WEIGHTS } from "./phosphor-weights";

const launcher = fileURLToPath(new URL("..", import.meta.url));

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === "node_modules" ? [] : sources(path);
    return /\.(ts|tsx)$/.test(name) ? [path] : [];
  });
}

describe("phosphor icon weights", () => {
  it("drops only the weights the launcher never draws", () => {
    const definition = readFileSync(
      join(launcher, "node_modules/@phosphor-icons/react/dist/defs/Archive.es.js"),
      "utf8",
    );
    const out = dropUnusedWeights(definition);
    // The package still prints its definitions the way the plugin expects.
    expect(out.length).toBeLessThan(definition.length);
    for (const weight of USED_WEIGHTS) expect(out).toContain(`"${weight}"`);
    expect(out).not.toContain('"thin"');
    expect(out).not.toContain('"light"');
    expect(out.trimEnd()).toMatch(/\]\);\s*export\s*\{/);
  });

  it("is asked only for weights that are kept", () => {
    const files = [
      ...sources(join(launcher, "src")),
      ...sources(join(launcher, "../../packages/ui/src")),
    ];
    const asked = new Set<string>();
    for (const file of files) {
      const text = readFileSync(file, "utf8");
      for (const match of text.matchAll(/\bweight(?:=\{?|:\s*)"([a-z]+)"/g)) {
        if (match[1]) asked.add(match[1]);
      }
    }
    expect(asked.size).toBeGreaterThan(0);
    for (const weight of asked) expect(USED_WEIGHTS).toContain(weight);
  });
});
