// Self-tests for CI's side of OVL-AUTO (`overlay-auto.mjs`): `node --test scripts/release/` (CI).

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { crc32 } from "node:zlib";

import {
  buildReport,
  checkTry,
  compareReleases,
  describeTested,
  errorLines,
  GAMETEST_SHOTS,
  issueText,
  junitTotals,
  LABEL,
  LABEL_COMPAT,
  LABEL_READY,
  lastCompat,
  namesVersion,
  newestFabricApi,
  newestTry,
  planTries,
  releaseNumbers,
  testsPassed,
  tildeTakes,
  tryOf,
} from "./overlay-auto.mjs";
import { LICENSE_ENTRY, MOD_DIR, ROOT } from "./overlay-jars.mjs";

const TARGETS = [
  {
    key: "26.1.2",
    minecraft: "26.1.2",
    fabricApi: "0.152.1+26.1.2",
    range: "~26.1.2",
    compat: "mc26_1",
  },
  { key: "26.2", minecraft: "26.2", fabricApi: "0.161.0+26.2", range: "~26.2", compat: "mc26_2" },
  { key: "26.3", minecraft: "26.3", fabricApi: "0.164.0+26.3", range: "~26.3", compat: "mc26_3" },
];

/** A zip of stored `[name, content]` entries. */
function makeZip(entries) {
  const parts = [];
  const directory = [];
  let offset = 0;
  for (const [name, content] of entries) {
    const nameBytes = Buffer.from(name);
    const data = Buffer.from(content);
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50, 0);
    header.writeUInt16LE(20, 4);
    header.writeUInt32LE(crc32(data), 14);
    header.writeUInt32LE(data.length, 18);
    header.writeUInt32LE(data.length, 22);
    header.writeUInt16LE(nameBytes.length, 26);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt32LE(crc32(data), 16);
    central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(nameBytes.length, 28);
    central.writeUInt32LE(offset, 42);
    parts.push(header, nameBytes, data);
    directory.push(central, nameBytes);
    offset += header.length + nameBytes.length + data.length;
  }
  const dir = Buffer.concat(directory);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(dir.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...parts, dir, end]);
}

const GPL = "GNU GENERAL PUBLIC LICENSE\nVersion 3, 29 June 2007\n";

function jar({ range = "~26.4", proto = 1, extra = [], license = GPL } = {}) {
  const meta = {
    schemaVersion: 1,
    id: "projects_overlay",
    version: "0.1.0+26.4",
    custom: { "projects:bridge": { proto } },
    depends: { minecraft: range },
  };
  return makeZip([
    ["fabric.mod.json", JSON.stringify(meta)],
    ...(license ? [[LICENSE_ENTRY, license]] : []),
    ...extra,
  ]);
}

const GREEN = {
  junit: { passed: 412, failed: 0, skipped: 0 },
  gametest: { passed: 1, failed: 0, skipped: 0 },
};

test("release versions are read and ordered as Mojang numbers them", () => {
  assert.deepEqual(releaseNumbers("26.4.1"), [26, 4, 1]);
  assert.equal(releaseNumbers("26.4-snapshot-6"), null);
  assert.equal(releaseNumbers("26w44a"), null);
  assert.ok(compareReleases("26.10", "26.9") > 0);
  assert.ok(compareReleases("26.4", "26.4.0") === 0);
  assert.ok(compareReleases("1.21.11", "26.1") < 0);
  assert.ok(compareReleases("26.4-pre1", "26.4") < 0, "anything else sorts first");
});

test("a tilde range takes its patches and nothing else", () => {
  assert.ok(tildeTakes("~26.4", "26.4"));
  assert.ok(tildeTakes("~26.4", "26.4.3"));
  assert.ok(tildeTakes("~26.1.2", "26.1.3"));
  assert.ok(!tildeTakes("~26.1.2", "26.1.1"));
  assert.ok(!tildeTakes("~26.4", "26.5"));
  assert.ok(!tildeTakes("~26.4", "26.40"));
  assert.ok(!tildeTakes(">=26.4", "26.4"), "only the kind build.gradle writes");
  assert.ok(!tildeTakes("~26.4", "26.4-snapshot-6"));
});

test("new releases are tried with the last compatibility class once Fabric API has them", () => {
  const releases = ["26.5", "26.4.1", "26.4", "26.3.1", "26.3", "26.2", "26.1.2", "1.21.11"];
  const apis = new Map([
    ["26.4", "0.170.0+26.4"],
    ["26.5", null],
  ]);
  const plan = planTries({ targets: TARGETS, releases, fabricApi: apis });
  // 26.3.1 is a patch the 26.3 jar takes; 26.4.1 is covered by the 26.4 try's range.
  assert.deepEqual(plan.tries, [
    { minecraft: "26.4", fabricApi: "0.170.0+26.4", compat: "mc26_3", range: "~26.4" },
  ]);
  assert.deepEqual(plan.waiting, ["26.5"]);
  assert.deepEqual(plan.left, []);

  // An issue (open or closed) or a published jar leaves a version alone.
  const titles = ["Overlay: Minecraft 26.4: tested jar ready"];
  assert.deepEqual(
    planTries({ targets: TARGETS, releases, fabricApi: apis, issues: titles }).left,
    ["26.4"],
  );
  assert.deepEqual(
    planTries({ targets: TARGETS, releases, fabricApi: apis, published: ["26.4"] }).tries,
    [],
  );
  assert.ok(namesVersion("Overlay: Minecraft 26.4: needs a compat class", "26.4"));
  assert.ok(!namesVersion("Overlay: Minecraft 26.4.1: needs a compat class", "26.4"));

  // Nothing new: nothing to try.
  const today = planTries({ targets: TARGETS, releases: ["26.3", "26.3.1"], fabricApi: new Map() });
  assert.deepEqual(today, { tries: [], waiting: [], left: [] });
});

test("a named version is tried with its own row, else the last class", () => {
  assert.deepEqual(tryOf(TARGETS, "26.3"), {
    minecraft: "26.3",
    fabricApi: "0.164.0+26.3",
    compat: "mc26_3",
    range: "~26.3",
    key: "26.3",
  });
  assert.deepEqual(tryOf(TARGETS, "26.4", "0.170.0+26.4"), {
    minecraft: "26.4",
    fabricApi: "0.170.0+26.4",
    compat: "mc26_3",
    range: "~26.4",
  });
  assert.throws(() => tryOf(TARGETS, "26.4", null), /no Fabric API release/);
  assert.equal(lastCompat([...TARGETS].reverse()), "mc26_3");
  assert.throws(
    () => lastCompat([{ minecraft: "26.3", range: "~26.3" }]),
    /no compatibility class/,
  );
  // A push's dry run: the newest target, a known-good run of the whole chain.
  assert.equal(newestTry([...TARGETS].reverse()).key, "26.3");
});

test("only versions and class names reach Gradle and the workflow", () => {
  const good = { minecraft: "26.4", fabricApi: "0.170.0+26.4", compat: "mc26_3", range: "~26.4" };
  assert.equal(checkTry(good), good);
  assert.ok(checkTry({ ...good, key: "26.3" }));
  for (const bad of [
    { ...good, minecraft: "26.4\nGRADLE_ARGS=x" },
    { ...good, fabricApi: "0.170.0 -Pevil" },
    { ...good, fabricApi: "$(id)" },
    { ...good, compat: "../../x" },
    { ...good, compat: "main" },
    { ...good, range: "~26.4;rm" },
    { ...good, key: "" },
    { ...good, minecraft: undefined },
  ]) {
    assert.throws(
      () => checkTry(bad),
      /is not a version|no compatibility class/,
      JSON.stringify(bad),
    );
  }
});

test("the newest Fabric API release is picked from Modrinth's list", () => {
  assert.equal(
    newestFabricApi([
      {
        version_number: "0.170.1+26.4",
        version_type: "beta",
        date_published: "2026-11-03T00:00:00Z",
      },
      {
        version_number: "0.170.0+26.4",
        version_type: "release",
        date_published: "2026-11-02T00:00:00Z",
      },
      {
        version_number: "0.169.0+26.4",
        version_type: "release",
        date_published: "2026-11-01T00:00:00Z",
      },
    ]),
    "0.170.0+26.4",
  );
  assert.equal(newestFabricApi([]), null);
});

test("JUnit results are summed from Gradle's XML", () => {
  const totals = junitTotals([
    '<?xml version="1.0"?><testsuite name="a" tests="10" skipped="1" failures="0" errors="0">',
    '<testsuite name="b" tests="5" skipped="0" failures="1" errors="1" timestamp="x">',
  ]);
  assert.deepEqual(totals, { passed: 12, failed: 2, skipped: 1 });
  assert.deepEqual(junitTotals([]), { passed: 0, failed: 0, skipped: 0 });
});

test("a jar counts as tested only when both suites ran green", () => {
  assert.ok(testsPassed(GREEN));
  assert.ok(!testsPassed({ ...GREEN, gametest: { passed: 0, failed: 1, skipped: 0 } }));
  assert.ok(!testsPassed({ ...GREEN, junit: { passed: 0, failed: 0, skipped: 3 } }));
  assert.deepEqual(GAMETEST_SHOTS.length, 5);
});

test("the report describes the jar from the jar itself", () => {
  const bytes = jar();
  const entry = describeTested("projects-overlay-0.1.0+26.4.jar", bytes, {
    minecraft: "26.4",
    range: "~26.4",
    fabricApi: "0.170.0+26.4",
    loader: "0.19.5",
    compat: "mc26_3",
    tests: GREEN,
  });
  assert.equal(entry.proto, 1);
  assert.equal(entry.version, "0.1.0+26.4");
  assert.equal(entry.sha256, createHash("sha256").update(bytes).digest("hex"));
  assert.equal(entry.sha1, createHash("sha1").update(bytes).digest("hex"));
  assert.equal(entry.size, bytes.length);
  const doc = buildReport({
    commit: "6b892a6a228d36d16b41e7c61b69fe8c0aa6749b",
    run: "https://github.com/projects-gg/MinecraftLauncher/actions/runs/1",
    testedAt: "2026-11-02T10:00:00Z",
    jars: [entry],
  });
  assert.equal(doc.schema, 1);
  assert.equal(doc.modId, "projects_overlay");
  assert.throws(() => buildReport({ commit: "6b892a6a", testedAt: "x", jars: [] }), /full SHA/);

  const opts = { minecraft: "26.4", range: "~26.4", fabricApi: "x", loader: "y", tests: GREEN };
  assert.throws(() => describeTested("x.jar", jar({ range: "~26.3" }), opts), /range ~26.3/);
  assert.throws(() => describeTested("x.jar", jar({ license: null }), opts), /no GPL text/);
  assert.throws(() => describeTested("x.jar", jar({ proto: 0 }), opts), /bridge protocol stamp/);
  assert.throws(
    () =>
      describeTested(
        "x.jar",
        jar({ extra: [["gg/projects/overlay/gametest/OverlaySmokeTest.class", "x"]] }),
        opts,
      ),
    /test code/,
  );
});

test("issues say only the facts", () => {
  const report = buildReport({
    commit: "6b892a6a228d36d16b41e7c61b69fe8c0aa6749b",
    testedAt: "2026-11-02T10:00:00Z",
    jars: [
      describeTested("projects-overlay-0.1.0+26.4.jar", jar(), {
        minecraft: "26.4",
        range: "~26.4",
        fabricApi: "0.170.0+26.4",
        loader: "0.19.5",
        compat: "mc26_3",
        tests: GREEN,
      }),
    ],
  });
  const ready = issueText("ready", {
    minecraft: "26.4",
    report,
    run: "https://github.com/x/y/actions/runs/1",
    artifact: "https://github.com/x/y/actions/runs/1/artifacts/2",
  });
  assert.equal(ready.title, "Overlay: Minecraft 26.4: tested jar ready");
  assert.deepEqual(ready.labels, [LABEL, LABEL_READY]);
  assert.match(ready.body, new RegExp(report.jars[0].sha256));
  assert.match(ready.body, /JUnit: 412 passed, 0 failed, 0 skipped/);
  assert.match(ready.body, /artifacts\/2/);
  assert.doesNotMatch(ready.body, /[A-Z]:\\|\/home\/|hazar|secrets/i);

  const compat = issueText("compat", {
    minecraft: "26.4",
    fabricApi: "0.170.0+26.4",
    compat: "mc26_3",
    step: "Build and JUnit",
    run: "https://github.com/x/y/actions/runs/1",
    log: errorLines(
      "> Task :compileJava\nCompat.java:12: error: cannot find symbol\nBUILD FAILED in 1m\n",
    ),
  });
  assert.equal(compat.title, "Overlay: Minecraft 26.4: needs a compat class");
  assert.deepEqual(compat.labels, [LABEL, LABEL_COMPAT]);
  assert.match(compat.body, /error: cannot find symbol/);
  assert.match(compat.body, /src\/mc26_4\/java/);
  assert.throws(
    () => issueText("ready", { minecraft: "26.5", report }),
    /no jar for Minecraft 26.5/,
  );
});

test("the workflow keeps to its permissions and never signs", () => {
  const workflow = readFileSync(join(ROOT, ".github", "workflows", "overlay-auto.yml"), "utf8");
  const permissions = /^permissions:\n((?: {2}.+\n)+)/m.exec(workflow)?.[1] ?? "";
  assert.deepEqual(
    permissions
      .trim()
      .split("\n")
      .map((line) => line.trim()),
    ["contents: read", "issues: write"],
  );
  assert.doesNotMatch(workflow, /secrets\.(?!GITHUB_TOKEN)/, "no secret but the run's own token");
  assert.doesNotMatch(workflow, /permissions:\s*\n\s+(contents|actions|packages|id-token): write/);
  // The game test's screenshots are the ones the report counts.
  const smoke = readFileSync(
    join(
      MOD_DIR,
      "src",
      "gametest",
      "java",
      "gg",
      "projects",
      "overlay",
      "gametest",
      "OverlaySmokeTest.java",
    ),
    "utf8",
  );
  for (const shot of GAMETEST_SHOTS)
    assert.match(smoke, new RegExp(`takeScreenshot\\("${shot}"\\)`));
});
