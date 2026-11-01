import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { afterReset, forgetReset, RESET_FRESH_KEY } from "./fresh-start";

function memory(initial: Record<string, string> = {}) {
  const items = new Map(Object.entries(initial));
  return {
    getItem: (key: string) => items.get(key) ?? null,
    removeItem: (key: string) => void items.delete(key),
    items,
  };
}

describe("fresh start after a reset", () => {
  it("is the mark the start leaves", () => {
    const rust = readFileSync(new URL("../../../src-tauri/src/reset.rs", import.meta.url), "utf8");
    expect(rust).toContain(`const FRESH_KEY: &str = "${RESET_FRESH_KEY}";`);
  });

  it("follows the mark until the welcome is done", () => {
    const storage = memory({ [RESET_FRESH_KEY]: "1" });
    expect(afterReset(storage)).toBe(true);
    forgetReset(storage);
    expect(afterReset(storage)).toBe(false);
    expect(storage.items.size).toBe(0);
  });

  it("is an ordinary first run without the mark or without storage", () => {
    expect(afterReset(memory())).toBe(false);
    const broken = {
      getItem: () => {
        throw new Error("storage disabled");
      },
      removeItem: () => {
        throw new Error("storage disabled");
      },
    };
    expect(afterReset(broken)).toBe(false);
    expect(() => forgetReset(broken)).not.toThrow();
  });
});
