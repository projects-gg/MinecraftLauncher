// UI-CN: the shared `cn` (packages/ui/src/lib/cn.ts) knows the theme's own names, so a later class
// replaces only what it means to replace. Unknown to tailwind-merge, a size such as `text-meta` read
// as a colour: `cn("text-meta text-ash-300")` lost the size (the cloud inventory's status line came
// out at the page's 14 px) and `cn("text-ash-300 text-meta")` the colour.
import { readFileSync } from "node:fs";
import { cn } from "@pl/ui";
import { describe, expect, it } from "vitest";

const css = readFileSync(
  new URL("../../../../packages/ui/src/styles.css", import.meta.url),
  "utf8",
);
const themeStart = css.indexOf("@theme {");
const themeBlock = css.slice(themeStart, css.indexOf("\n}", themeStart));

/** The names styles.css's `@theme` gives a namespace ("--text-meta" → "meta"), less companions. */
function names(namespace: string): string[] {
  const pattern = new RegExp(`^\\s*--${namespace}-([a-z0-9-]+?):`, "gm");
  return [...themeBlock.matchAll(pattern)]
    .map((match) => match[1] ?? "")
    .filter((name) => name && !name.includes("--"));
}

const sizes = names("text");
const colours = names("color");

describe("cn with the theme's names", () => {
  it("reads the theme from styles.css", () => {
    expect(sizes).toEqual([
      "hero",
      "page",
      "section",
      "title",
      "body",
      "ui",
      "meta",
      "caption",
      "micro",
    ]);
    expect(colours).toContain("ash-300");
  });

  it("keeps a size and a colour together, in either order", () => {
    for (const size of sizes) {
      for (const colour of ["ash-300", "bone-50", "gold-ink"]) {
        expect(cn(`text-${size} text-${colour}`)).toBe(`text-${size} text-${colour}`);
        expect(cn(`text-${colour}`, `text-${size}`)).toBe(`text-${colour} text-${size}`);
      }
    }
  });

  it("lets a later size replace an earlier one, and a later colour an earlier colour", () => {
    expect(cn("text-ui text-ash-300", "text-meta")).toBe("text-ash-300 text-meta");
    expect(cn("text-meta", "text-[15px]")).toBe("text-[15px]");
    expect(cn("text-sm", "text-caption")).toBe("text-caption");
    expect(cn("text-meta text-ash-300", "text-bone-100")).toBe("text-meta text-bone-100");
    expect(cn("text-meta", "text-ash-300/80")).toBe("text-meta text-ash-300/80");
    // Per state, as the plain classes: a hover colour leaves the size alone.
    expect(cn("text-meta hover:text-bone-100", "hover:text-gold-300")).toBe(
      "text-meta hover:text-gold-300",
    );
  });

  it("files every name styles.css defines in its own group", () => {
    const groups: Array<[namespace: string, earlier: string, prefix: string]> = [
      ["text", "text-base", "text"],
      ["spacing", "px-4", "px"],
      ["radius", "rounded-md", "rounded"],
      ["ease", "ease-out", "ease"],
      ["animate", "animate-spin", "animate"],
      ["font", "font-mono", "font"],
    ];
    for (const [namespace, earlier, prefix] of groups) {
      const own = names(namespace);
      expect(own.length).toBeGreaterThan(0);
      for (const name of own) {
        expect(cn(earlier, `${prefix}-${name}`)).toBe(`${prefix}-${name}`);
        expect(cn(`${prefix}-${name}`, earlier)).toBe(earlier);
      }
    }
  });

  it("keeps the classes the old merge dropped", () => {
    // The welcome's big title: its colour went with `text-hero`.
    expect(
      cn("font-display font-semibold text-bone-50 outline-none", "text-hero tracking-[-0.01em]"),
    ).toBe("font-display font-semibold text-bone-50 outline-none text-hero tracking-[-0.01em]");
    // The troubleshooter's verdict: its size went with `text-bone-50`.
    expect(cn("grid", "font-display text-title font-semibold text-bone-50")).toBe(
      "grid font-display text-title font-semibold text-bone-50",
    );
    // The cloud inventory's status line.
    expect(cn("flex items-center text-meta text-ash-300")).toBe(
      "flex items-center text-meta text-ash-300",
    );
  });

  it("keeps a leading given after a size, and lets a later size replace an earlier leading", () => {
    // As Tailwind draws it: the leading wins over the size's own line height.
    expect(cn("text-meta leading-none")).toBe("text-meta leading-none");
    // tailwind-merge's rule for every size (`text-sm` too): a size brings its own line height.
    expect(cn("leading-none", "text-meta")).toBe("text-meta");
  });
});
