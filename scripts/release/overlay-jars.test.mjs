// Self-tests for the overlay jar bundling (`overlay-jars.mjs`): `node --test scripts/release/` (CI).

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { crc32, deflateRawSync } from "node:zlib";

import {
  bridgeProto,
  checkBundle,
  DEV_ENTRIES,
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
  zipEntryNames,
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

function overlayJar(version, range, extra = "", license = GPL, proto = 1) {
  const json = JSON.stringify({
    schemaVersion: 1,
    id: "projects_overlay",
    version,
    ...(proto === null ? {} : { custom: { "projects:bridge": { proto } } }),
    depends: { minecraft: range, "fabric-screen-api-v1": "*" },
  });
  return makeZip([
    ["fabric.mod.json", json],
    ["build.txt", extra, true],
    ...(license === null ? [] : [[LICENSE_ENTRY, license]]),
  ]);
}

const TARGETS = [
  {
    key: "26.1.2",
    minecraft: "26.1.2",
    fabricApi: "0.152.1+26.1.2",
    range: "~26.1.2",
    compat: "mc26_1",
  },
  { key: "26.2", minecraft: "26.2", fabricApi: "0.161.0+26.2", range: "~26.2", compat: "mc26_2" },
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
    proto: 1,
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

test("a jar says the bridge protocol it speaks (OVL-AUTO)", () => {
  const unstamped = overlayJar("0.1.0+26.2", "~26.2", "", GPL, null);
  assert.throws(() => describeJar("x.jar", unstamped, TARGETS[1]), /no bridge protocol stamp/);
  for (const bad of [0, -1, 1.5, "1"]) {
    const jar = overlayJar("0.1.0+26.2", "~26.2", "", GPL, bad);
    assert.throws(() => describeJar("x.jar", jar, TARGETS[1]), /no bridge protocol stamp/);
  }
  const next = overlayJar("0.1.0+26.2", "~26.2", "", GPL, 2);
  assert.equal(describeJar("x.jar", next, TARGETS[1]).proto, 2);
  assert.equal(bridgeProto({ custom: { "projects:bridge": { proto: 1 } } }, "x.jar"), 1);

  // The mod's own fabric.mod.json carries the stamp, and the protocol the Java side and the
  // launcher speak is the same number (the mod's JUnit checks the Java side too).
  const meta = JSON.parse(
    readFileSync(join(MOD_DIR, "src", "main", "resources", "fabric.mod.json"), "utf8"),
  );
  const stamped = String(meta.custom["projects:bridge"].proto);
  const java = readFileSync(
    join(MOD_DIR, "src", "main", "java", "gg", "projects", "overlay", "bridge", "Protocol.java"),
    "utf8",
  );
  assert.equal(/public static final int VERSION = (\d+);/.exec(java)?.[1], stamped);
  const rust = readFileSync(join(ROOT, "crates", "pl-overlay", "src", "protocol.rs"), "utf8");
  assert.equal(/pub const PROTO: u32 = (\d+);/.exec(rust)?.[1], stamped);
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

test("a jar with test code is refused", () => {
  const json = JSON.stringify({
    schemaVersion: 1,
    id: "projects_overlay",
    version: "0.1.0+26.2",
    custom: { "projects:bridge": { proto: 1 } },
    depends: { minecraft: "~26.2" },
  });
  for (const entry of [
    "gg/projects/overlay/dev/H1eScenario.class",
    "gg/projects/overlay/dev/mixin/WindowNoFocusMixin.class",
    "projects_overlay_dev.mixins.json",
    "gg/projects/overlay/gametest/OverlaySmokeTest.class",
  ]) {
    const jar = makeZip([
      ["fabric.mod.json", json],
      [LICENSE_ENTRY, GPL],
      [entry, "x"],
    ]);
    assert.throws(() => describeJar("x.jar", jar, TARGETS[1]), /carries test code/);
  }
  assert.deepEqual(zipEntryNames(overlayJar("0.1.0+26.2", "~26.2")), [
    "fabric.mod.json",
    "build.txt",
    LICENSE_ENTRY,
  ]);
  assert.throws(() => zipEntryNames(Buffer.from("not a zip at all".repeat(3))), /not a zip/);

  // The driver is build.gradle's own `dev` source set (never the shipped jar's), and everything
  // it registers sits where DEV_ENTRIES looks.
  const gradle = readFileSync(join(MOD_DIR, "build.gradle"), "utf8");
  assert.match(gradle, /projects_overlay \{\s+sourceSet sourceSets\.main\s+\}/);
  assert.match(gradle, /projects_overlay_dev \{\s+sourceSet sourceSets\.dev\s+\}/);
  const dev = JSON.parse(
    readFileSync(join(MOD_DIR, "src", "dev", "resources", "fabric.mod.json"), "utf8"),
  );
  const classes = dev.entrypoints.client.map((name) => `${name.replaceAll(".", "/")}.class`);
  assert.ok(classes.length >= 1);
  for (const { config } of dev.mixins) {
    const mixins = JSON.parse(
      readFileSync(join(MOD_DIR, "src", "dev", "resources", config), "utf8"),
    );
    classes.push(`${mixins.package.replaceAll(".", "/")}/${mixins.client[0]}.class`);
  }
  for (const name of [...classes, ...dev.mixins.map((mixin) => mixin.config)]) {
    assert.ok(
      DEV_ENTRIES.some((re) => re.test(name)),
      name,
    );
  }
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
  assert.ok(files.includes("crates/pl-content/data/chat-emojis.json"));
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
