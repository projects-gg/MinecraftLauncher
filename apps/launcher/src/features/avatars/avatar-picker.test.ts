import { describe, expect, it } from "vitest";
import manifest from "@/assets/avatars/manifest.json";
import { moveInGrid, presetChoices } from "./avatar-picker";
import {
  isPresetId,
  type ManifestAvatar,
  PRESET_AVATARS,
  presetAvatar,
  readManifest,
} from "./presets";

describe("moveInGrid", () => {
  // 11 tiles, 6 per row: 0–5 and 6–10.
  const move = (index: number, key: string) => moveInGrid(index, key, 11, 6);

  it("moves sideways across rows and stops at the ends", () => {
    expect(move(5, "ArrowRight")).toBe(6);
    expect(move(6, "ArrowLeft")).toBe(5);
    expect(move(10, "ArrowRight")).toBe(10);
    expect(move(0, "ArrowLeft")).toBe(0);
  });

  it("moves by rows, landing on the last tile of a shorter last row", () => {
    expect(move(2, "ArrowDown")).toBe(8);
    expect(move(5, "ArrowDown")).toBe(10);
    expect(move(8, "ArrowDown")).toBe(8);
    expect(move(8, "ArrowUp")).toBe(2);
    expect(move(3, "ArrowUp")).toBe(3);
  });

  it("jumps to the ends and ignores other keys", () => {
    expect(move(7, "Home")).toBe(0);
    expect(move(1, "End")).toBe(10);
    expect(move(1, "Enter")).toBeNull();
    expect(moveInGrid(0, "ArrowRight", 0, 6)).toBeNull();
  });
});

describe("bundled pictures", () => {
  it("follow the manifest order, with backend-style ids", () => {
    expect(PRESET_AVATARS.map((a) => a.slug)).toEqual(manifest.avatars.map((a) => a.slug));
    for (const avatar of PRESET_AVATARS) {
      expect(avatar.id).toBe(`preset:${avatar.slug}`);
      expect(isPresetId(avatar.id)).toBe(true);
      expect(avatar.small).toBeTruthy();
      expect(avatar.large).toBeTruthy();
    }
    expect(presetChoices().map((c) => c.label)).toEqual(manifest.avatars.map((a) => a.name));
  });

  it("resolve by id; anything else shows the head", () => {
    const first = PRESET_AVATARS[0];
    expect(first && presetAvatar(first.id)).toBe(first);
    expect(presetAvatar("preset:not-in-the-set")).toBeNull();
    expect(presetAvatar(null)).toBeNull();
    expect(presetAvatar(undefined)).toBeNull();
    expect(presetAvatar(first?.slug)).toBeNull();
  });

  it("validate ids like the backend", () => {
    expect(isPresetId("preset:golden-dragon")).toBe(true);
    expect(isPresetId(`preset:${"a".repeat(32)}`)).toBe(true);
    expect(isPresetId(`preset:${"a".repeat(33)}`)).toBe(false);
    expect(isPresetId("preset:")).toBe(false);
    expect(isPresetId("preset:Fox")).toBe(false);
    expect(isPresetId("preset:../fox")).toBe(false);
    expect(isPresetId("discord")).toBe(false);
    expect(isPresetId(42)).toBe(false);
  });

  it("skip broken manifest entries instead of showing broken tiles", () => {
    const entry = (slug: string, files: Record<string, string>): ManifestAvatar => ({
      slug,
      name: slug,
      alt: slug,
      category: "creature",
      accent: "#ffffff",
      files,
    });
    const known = new Set(["a.webp", "a-128.webp", "b.webp", "c-128.webp"]);
    const avatars = readManifest(
      [
        entry("a", { "256": "a.webp", "128": "a-128.webp" }),
        entry("Bad Slug", { "256": "a.webp" }),
        entry("a", { "256": "a.webp" }),
        entry("b", { "256": "b.webp", "128": "missing.webp" }),
        entry("c", { "128": "c-128.webp" }),
        entry("d", { "256": "missing.webp" }),
      ],
      (file) => (file && known.has(file) ? `/x/${file}` : undefined),
    );
    expect(avatars.map((a) => [a.slug, a.small, a.large])).toEqual([
      ["a", "/x/a-128.webp", "/x/a.webp"],
      ["b", "/x/b.webp", "/x/b.webp"],
      ["c", "/x/c-128.webp", "/x/c-128.webp"],
    ]);
  });
});
