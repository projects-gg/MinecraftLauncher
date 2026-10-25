// SMALL11: the launcher's text faces (packages/ui/src/fonts) are our latin subsets of Figtree and
// Tektur, with an "r" + "ı" kerning pair, beside fontsource's other subsets. Both halves must come
// from the same fontsource release, or one word could mix two versions of a face: after upgrading
// either package, run packages/ui/scripts/build-text-fonts.py again (it rewrites the files and
// fonts.json). The pair only applies when "ı" comes from our file, beside "r".
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

interface Built {
  package: string;
  version: string;
  /** SHA-256 of fontsource's latin file the build read. */
  source: string;
  file: string;
  sha256: string;
  pairs: Record<string, number>;
}

const ui = new URL("../../../../packages/ui/", import.meta.url);
const read = (path: string) => readFileSync(new URL(path, ui));
const text = (path: string) => read(path).toString("utf8");
const sha256 = (data: Buffer) => createHash("sha256").update(data).digest("hex");

const manifest = JSON.parse(text("src/fonts/fonts.json")) as Record<string, Built>;
const pkg = JSON.parse(text("package.json")) as { dependencies: Record<string, string> };
const faces = text("src/fonts/fonts.css").split("@font-face").slice(1);
const familyOf = (face: string) => /font-family: "([^"]+)"/.exec(face)?.[1];

describe("text fonts", () => {
  it("covers both faces", () => {
    expect(Object.keys(manifest).sort()).toEqual(["figtree", "tektur"]);
  });

  it.each(Object.values(manifest))("$file is built from the installed $package", (built) => {
    expect(pkg.dependencies[built.package]).toBe(built.version);
    const installed = JSON.parse(text(`node_modules/${built.package}/package.json`)) as {
      version: string;
    };
    expect(installed.version).toBe(built.version);
    expect(sha256(read(`node_modules/${built.package}/files/${built.file}`))).toBe(built.source);
    expect(sha256(read(`src/fonts/${built.file}`))).toBe(built.sha256);
    expect(built.pairs["r dotlessi"]).toBeGreaterThan(0);
  });

  it.each(Object.values(manifest))("$file is its family's latin face, declared last", (built) => {
    const ours = faces.findIndex((face) => face.includes(`url("./${built.file}")`));
    expect(ours).toBeGreaterThan(-1);
    const face = faces[ours] ?? "";
    expect(face).toMatch(/U\+0000-00FF,/);
    expect(face).toMatch(/U\+0131,/);
    const family = familyOf(face);
    expect(faces.findLastIndex((other) => familyOf(other) === family)).toBe(ours);
    // The family's other subsets are the same release's files.
    for (const other of faces.filter((f) => familyOf(f) === family && f !== face)) {
      expect(other).toContain(`url("${built.package}/files/`);
    }
  });
});
