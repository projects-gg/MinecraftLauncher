// Self-tests for the overlay jar bundling (`overlay-jars.mjs`): `node --test scripts/release/` (CI).

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { crc32, deflateRawSync } from "node:zlib";

import {
  checkBundle,
  describeJar,
  INDEX_FILE,
  LICENSE_ENTRY,
  MOD_DIR,
  parseModVersion,
  parseTargets,
  ROOT,
  readZipEntry,
  sha1,
  sourceFiles,
  sourceStamp,
} from "./overlay-jars.mjs";

/** A zip of `[name, content, stored?]` entries (deflated unless stored). */
function makeZip(entries) {
  const parts = [];
  const central = [];
  let offset = 0;
  for (const [name, content, stored] of entries) {
    const data = Buffer.from(content);
    const body = stored ? data : deflateRawSync(data);
    const method = stored ? 0 : 8;
    const nameBytes = Buffer.from(name);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(method, 8);
    local.writeUInt32LE(crc32(data), 14);
    local.writeUInt32LE(body.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    parts.push(local, nameBytes, body);
    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0);
    entry.writeUInt16LE(20, 4);
    entry.writeUInt16LE(20, 6);
    entry.writeUInt16LE(method, 10);
    entry.writeUInt32LE(crc32(data), 16);
    entry.writeUInt32LE(body.length, 20);
    entry.writeUInt32LE(data.length, 24);
    entry.writeUInt16LE(nameBytes.length, 28);
    entry.writeUInt32LE(offset, 42);
    central.push(entry, nameBytes);
    offset += 30 + nameBytes.length + body.length;
  }
  const directory = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...parts, directory, end]);
}

const GPL =
  "                    GNU GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007\n";

function overlayJar(version, range, extra = "", license = GPL) {
  const json = JSON.stringify({
    schemaVersion: 1,
    id: "projects_overlay",
    version,
    depends: { minecraft: range, "fabric-screen-api-v1": "*" },
  });
  return makeZip([
    ["fabric.mod.json", json],
    ["build.txt", extra, true],
    ...(license === null ? [] : [[LICENSE_ENTRY, license]]),
  ]);
}

const TARGETS = [
  { key: "26.1.2", minecraft: "26.1.2", fabricApi: "0.152.1+26.1.2", range: "~26.1.2" },
  { key: "26.2", minecraft: "26.2", fabricApi: "0.161.0+26.2", range: "~26.2" },
];

/** A sound folder for TARGETS with source stamp `stamp`. */
function folder(stamp = "abc") {
  const dir = mkdtempSync(join(tmpdir(), "overlay-jars-"));
  const jars = TARGETS.map((target) => {
    const version = `0.1.0+${target.minecraft}`;
    const file = `projects-overlay-${version}.jar`;
    const bytes = overlayJar(version, target.range, target.key);
    writeFileSync(join(dir, file), bytes);
    return describeJar(file, bytes, target);
  });
  writeFileSync(join(dir, INDEX_FILE), JSON.stringify({ schema: 1, source: stamp, jars }));
  return dir;
}

test("build.gradle's targets and the mod's version are read", () => {
  const gradle = `
def targets = [
    '26.1.2': [minecraft: '26.1.2', fabricApi: '0.152.1+26.1.2', range: '~26.1.2', compat: 'mc26_1'],
    '26.2'  : [minecraft: '26.2', fabricApi: '0.161.0+26.2', range: '~26.2', compat: 'mc26_2'],
]
def mc = 1`;
  assert.deepEqual(parseTargets(gradle), TARGETS);
  assert.throws(() => parseTargets("plugins {}"), /no `def targets/);
  assert.equal(parseModVersion("a=1\nmod_version=0.2.0\n"), "0.2.0");
  assert.throws(() => parseModVersion("a=1"), /mod_version/);

  // The real build file: every version in order, the first one the default.
  const real = parseTargets(readFileSync(join(MOD_DIR, "build.gradle"), "utf8"));
  assert.ok(real.length >= 3);
  assert.deepEqual(
    real.slice(0, 3).map((t) => t.minecraft),
    ["26.1.2", "26.2", "26.3"],
  );
  assert.ok(real.every((t) => t.range.startsWith("~")));
  parseModVersion(readFileSync(join(MOD_DIR, "gradle.properties"), "utf8"));
});

test("zip entries are read, stored or deflated", () => {
  const zip = makeZip([
    ["a.txt", "hello"],
    ["b.txt", "stored", true],
  ]);
  assert.equal(readZipEntry(zip, "a.txt").toString(), "hello");
  assert.equal(readZipEntry(zip, "b.txt").toString(), "stored");
  assert.equal(readZipEntry(zip, "c.txt"), null);
  assert.throws(
    () => readZipEntry(Buffer.from("not a zip at all, sorry".repeat(2)), "a"),
    /not a zip/,
  );
});

test("a built jar is described from its own fabric.mod.json", () => {
  const bytes = overlayJar("0.1.0+26.2", "~26.2");
  const entry = describeJar("projects-overlay-0.1.0+26.2.jar", bytes, TARGETS[1]);
  assert.deepEqual(entry, {
    minecraft: "26.2",
    range: "~26.2",
    file: "projects-overlay-0.1.0+26.2.jar",
    version: "0.1.0+26.2",
    sha1: sha1(bytes),
    size: bytes.length,
  });
  assert.throws(
    () => describeJar("x.jar", bytes, TARGETS[0]),
    /range ~26.2, build.gradle says ~26.1.2/,
  );
  const other = makeZip([["fabric.mod.json", JSON.stringify({ id: "sodium", version: "1" })]]);
  assert.throws(() => describeJar("x.jar", other, TARGETS[1]), /mod id sodium/);
});

test("a jar without the GPL text is refused", () => {
  const bare = overlayJar("0.1.0+26.2", "~26.2", "", null);
  assert.throws(() => describeJar("x.jar", bare, TARGETS[1]), /no GPL text/);
  const wrong = overlayJar("0.1.0+26.2", "~26.2", "", "MIT License\n");
  assert.throws(() => describeJar("x.jar", wrong, TARGETS[1]), /no GPL text/);

  // build.gradle puts it there: the template's name, from the repository's own license.
  const gradle = readFileSync(join(MOD_DIR, "build.gradle"), "utf8");
  assert.match(gradle, /from\(file\('\.\.\/\.\.\/LICENSE'\)\)/);
  assert.equal(LICENSE_ENTRY, "LICENSE_projects-overlay");
  assert.match(readFileSync(join(ROOT, "LICENSE"), "utf8"), /GNU GENERAL PUBLIC LICENSE/);
});

test("the folder check finds every way a release could ship a wrong overlay", () => {
  const dir = folder("stamp-1");
  try {
    assert.deepEqual(checkBundle(dir, TARGETS), []);
    assert.deepEqual(checkBundle(dir, TARGETS, "stamp-1"), []);
    assert.match(checkBundle(dir, TARGETS, "stamp-2").join(), /older than the mod's source/);

    // A version build.gradle builds is missing.
    const three = [...TARGETS, { key: "26.3", minecraft: "26.3", fabricApi: "x", range: "~26.3" }];
    assert.match(
      checkBundle(dir, three).join(),
      /for 26.1.2, 26.2, build.gradle builds 26.1.2, 26.2, 26.3/,
    );

    // A jar changed after the index was written, then one is gone.
    writeFileSync(
      join(dir, "projects-overlay-0.1.0+26.2.jar"),
      overlayJar("0.1.0+26.2", "~26.2", "x"),
    );
    assert.match(checkBundle(dir, TARGETS).join(), /not the file the index names/);
    rmSync(join(dir, "projects-overlay-0.1.0+26.2.jar"));
    assert.match(
      checkBundle(dir, TARGETS).join(),
      /projects-overlay-0\.1\.0\+26\.2\.jar is missing/,
    );

    // No index at all.
    rmSync(join(dir, INDEX_FILE));
    assert.match(checkBundle(dir, TARGETS).join(), /index\.json is missing/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("the index's version must be the jar's", () => {
  const dir = folder();
  try {
    const index = JSON.parse(readFileSync(join(dir, INDEX_FILE), "utf8"));
    index.jars[0].version = "9.9.9";
    writeFileSync(join(dir, INDEX_FILE), JSON.stringify(index));
    assert.match(checkBundle(dir, TARGETS).join(), /version 0\.1\.0\+26\.1\.2, index says 9\.9\.9/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("the source stamp covers what the jar is made of and nothing else", () => {
  const files = sourceFiles();
  assert.ok(files.includes("mods/projects-overlay/build.gradle"));
  assert.ok(files.includes("mods/projects-overlay/src/main/resources/fabric.mod.json"));
  assert.ok(files.some((f) => /^mods\/projects-overlay\/src\/mc26_2\/java\//.test(f)));
  assert.ok(files.includes("packages/stickers/pl1/manifest.json"));
  assert.ok(files.includes("LICENSE"));
  assert.ok(
    !files.some((f) => f.includes("/src/test/") || f.includes("/tools/") || f.includes("/dev/")),
  );
  assert.match(sourceStamp(), /^[0-9a-f]{64}$/);

  // A copy of the tree with one source file changed stamps differently.
  const copy = mkdtempSync(join(tmpdir(), "overlay-src-"));
  try {
    for (const file of files) {
      mkdirSync(join(copy, file, ".."), { recursive: true });
      writeFileSync(join(copy, file), readFileSync(join(ROOT, file)));
    }
    assert.equal(sourceStamp(copy), sourceStamp());
    writeFileSync(join(copy, "mods/projects-overlay/gradle.properties"), "changed\n");
    assert.notEqual(sourceStamp(copy), sourceStamp());
  } finally {
    rmSync(copy, { recursive: true, force: true });
  }
});
