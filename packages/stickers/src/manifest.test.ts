import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import manifest from "../pl1/manifest.json";
import { stickerImage } from "./images";
import {
  findSticker,
  isKnownSticker,
  parseStickerSet,
  STICKER_MOTIONS,
  STICKER_SETS,
} from "./index";

const dir = fileURLToPath(new URL("../pl1/", import.meta.url));
const sha256 = (path: string) =>
  createHash("sha256")
    .update(readFileSync(join(dir, path)))
    .digest("hex");
const clone = () => JSON.parse(JSON.stringify(manifest));

/** Canvas size of a WebP (VP8X, VP8L or VP8 header). */
function webpSize(path: string): [number, number] {
  const bytes = readFileSync(join(dir, path));
  expect(bytes.toString("ascii", 0, 4)).toBe("RIFF");
  expect(bytes.toString("ascii", 8, 12)).toBe("WEBP");
  const chunk = bytes.toString("ascii", 12, 16);
  if (chunk === "VP8X") return [bytes.readUIntLE(24, 3) + 1, bytes.readUIntLE(27, 3) + 1];
  if (chunk === "VP8L") {
    const bits = bytes.readUInt32LE(21);
    return [(bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1];
  }
  return [bytes.readUInt16LE(26) & 0x3fff, bytes.readUInt16LE(28) & 0x3fff];
}

describe("the pl1 manifest", () => {
  const [pl1] = STICKER_SETS;

  it("holds 32 stickers, eight in each of the four themes, in order", () => {
    expect(pl1?.set).toBe("pl1");
    expect(pl1?.stickers).toHaveLength(32);
    expect(pl1?.themes.map((theme) => theme.id)).toEqual([
      "reactions",
      "game",
      "server",
      "greetings",
    ]);
    for (const theme of pl1?.themes ?? []) {
      expect(pl1?.stickers.filter((sticker) => sticker.theme === theme.id)).toHaveLength(8);
    }
    expect(pl1?.stickers.map((sticker) => sticker.order)).toEqual(
      Array.from({ length: 32 }, (_, index) => index + 1),
    );
  });

  it("gives every sticker its names, words and one known motion, using every motion", () => {
    for (const sticker of pl1?.stickers ?? []) {
      expect(sticker.name.tr.length).toBeGreaterThan(0);
      expect(sticker.name.en).not.toMatch(/[çğıöşüÇĞİÖŞÜ]/);
      expect(sticker.keywords.tr.length).toBeGreaterThan(0);
      expect(sticker.keywords.en.length).toBeGreaterThan(0);
      expect(STICKER_MOTIONS).toContain(sticker.motion);
    }
    expect(new Set(pl1?.stickers.map((sticker) => sticker.motion))).toEqual(
      new Set(STICKER_MOTIONS),
    );
  });

  it("lists files that are there, with their bytes and SHA-256", () => {
    for (const sticker of manifest.stickers) {
      for (const file of Object.values(sticker.files)) {
        expect(statSync(join(dir, file.path)).size, file.path).toBe(file.bytes);
        expect(sha256(file.path), file.path).toBe(file.sha256);
      }
    }
    const atlas = manifest.formats.overlay;
    expect(statSync(join(dir, atlas.path)).size).toBe(atlas.bytes);
    expect(sha256(atlas.path)).toBe(atlas.sha256);
  });

  it("keeps SHA256SUMS.txt in step with every image in the set", () => {
    const sums = readFileSync(join(dir, "SHA256SUMS.txt"), "utf8").trim().split(/\r?\n/);
    const listed = new Map(sums.map((line) => [line.slice(66), line.slice(0, 64)]));
    const files = ["launcher", "android", "overlay"].flatMap((sub) =>
      readdirSync(join(dir, sub)).map((name) =>
        relative(dir, join(dir, sub, name)).replaceAll("\\", "/"),
      ),
    );
    expect([...listed.keys()].sort()).toEqual(files.sort());
    for (const [path, hash] of listed) expect(sha256(path), path).toBe(hash);
  });

  it("stays within the size budget (chat2.md §8.3)", () => {
    let launcher = 0;
    let android = 0;
    for (const sticker of manifest.stickers) {
      expect(sticker.files.launcher.bytes).toBeLessThanOrEqual(25 * 1024);
      expect(sticker.files.android.bytes).toBeLessThanOrEqual(40 * 1024);
      expect(webpSize(sticker.files.launcher.path)).toEqual([256, 256]);
      expect(webpSize(sticker.files.android.path)).toEqual([384, 384]);
      launcher += sticker.files.launcher.bytes;
      android += sticker.files.android.bytes;
    }
    expect(launcher).toBeLessThanOrEqual(819 * 1024);
    expect(android).toBeLessThanOrEqual(1331 * 1024);
    expect(manifest.formats.overlay.bytes).toBeLessThanOrEqual(320 * 1024);
  });
});

describe("parseStickerSet", () => {
  it("refuses a broken manifest", () => {
    const cases: Array<[string, (m: ReturnType<typeof clone>) => void]> = [
      [
        "two stickers share an id",
        (m) => (m.stickers[1] = { ...m.stickers[0], order: 2, atlas: m.stickers[1].atlas }),
      ],
      ["ref", (m) => (m.stickers[0].ref = "pl2/lol")],
      ["unknown motion", (m) => (m.stickers[0].motion = "spin")],
      ["unknown theme", (m) => (m.stickers[0].theme = "music")],
      ["atlas cell taken", (m) => (m.stickers[1].atlas = m.stickers[0].atlas)],
      ["outside the atlas", (m) => (m.stickers[0].atlas = { column: 8, row: 0 })],
      ["order", (m) => (m.stickers[0].order = 40)],
      ["launcher image path", (m) => (m.stickers[0].files.launcher.path = "launcher/x.webp")],
      ["version", (m) => (m.version = 2)],
    ];
    for (const [message, breakIt] of cases) {
      const broken = clone();
      breakIt(broken);
      expect(() => parseStickerSet(broken), message).toThrow(message);
    }
  });
});

describe("references", () => {
  it("finds the stickers of this build and nothing else", () => {
    expect(findSticker("pl1/gg")?.name).toEqual({ tr: "GG", en: "GG" });
    expect(findSticker("pl1/nope")).toBeUndefined();
    expect(isKnownSticker("pl1/hi")).toBe(true);
    expect(isKnownSticker("pl2/hi")).toBe(false);
    expect(isKnownSticker("PL1/HI")).toBe(false);
    expect(isKnownSticker("pl1/../hi")).toBe(false);
  });

  it("points at the launcher's image", () => {
    const gg = findSticker("pl1/gg");
    expect(gg && stickerImage(gg)).toMatch(/\/pl1\/launcher\/gg\.webp$/);
  });
});
