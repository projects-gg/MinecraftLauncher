#!/usr/bin/env node
// The in-game overlay for Minecraft versions the launcher's jars do not cover (OVL-AUTO), CI's side
// (.github/workflows/overlay-auto.yml). CI holds no keys and publishes nothing: a green run leaves
// the tested jar and `overlay-release.json` as artifacts, Studio imports them on the owner's
// computer and the signed index is what makes the jar trusted (docs/content-format.md, "In-game
// overlay jars").
//
//   node scripts/release/overlay-auto.mjs watch [--newest | --minecraft <version>] [--issues <file>] [--published <file>]
//   node scripts/release/overlay-auto.mjs report --jar <file> --junit <dir> --shots <dir>
//       --minecraft <v> --fabric-api <v> --compat <c> --out <dir>
//   node scripts/release/overlay-auto.mjs issue --kind ready|compat --minecraft <v> [--report <file>]
//       [--fabric-api <v>] [--compat <c>] [--step <name>] [--run <url>] [--artifact <url>] [--log <file>]
//
// watch   The versions to try, as a GitHub Actions matrix (`tries=` and `any=` in $GITHUB_OUTPUT,
//         else printed): Mojang releases no target's range takes and newer than every target, the
//         first of each minor version (its range takes the patches), once Modrinth lists a Fabric
//         API release for it. A version with an `overlay-auto` issue (open or closed; `--issues`, the
//         JSON of `gh issue list --json title`) or a published jar (`--published`, the live index) is
//         left alone. `--minecraft` tries that one version whatever else is true: a target's own row,
//         else the last compatibility class. `--newest` tries the newest target (a known-good run).
// report  Describes the tested jar as Studio checks it (id, version, range, bridge protocol, the GPL
//         text, no test code), counts the JUnit results and the game test's screenshots, and writes
//         `overlay-release.json` with the jar beside it into `--out`. Fails unless both ran green.
// issue   The title and body of the run's issue (`--kind ready`: the jar is ready for Studio;
//         `compat`: the version needs a new compatibility class), facts only: the repository is public.

import { createHash } from "node:crypto";
import {
  appendFileSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { parseArgs, run, UsageError } from "./lib.mjs";
import {
  bridgeProto,
  DEV_ENTRIES,
  LICENSE_ENTRY,
  MOD_DIR,
  MOD_ID,
  modJson,
  parseTargets,
  readZipEntry,
  zipEntryNames,
} from "./overlay-jars.mjs";

const USAGE = `usage: node scripts/release/overlay-auto.mjs watch [--newest | --minecraft <version>] [--issues <file>] [--published <file>]
       node scripts/release/overlay-auto.mjs report --jar <file> --junit <dir> --shots <dir> --minecraft <v> --fabric-api <v> --compat <c> --out <dir>
       node scripts/release/overlay-auto.mjs issue --kind ready|compat --minecraft <v> [--report <file>] [--fabric-api <v>] [--compat <c>] [--step <name>] [--run <url>] [--artifact <url>] [--log <file>]`;

export const REPORT_FILE = "overlay-release.json";
export const REPORT_SCHEMA = 1;
/** Every issue of these runs carries it (easy to filter), and one of the two kinds. */
export const LABEL = "overlay-auto";
export const LABEL_READY = "overlay-ready";
export const LABEL_COMPAT = "overlay-compat";
/** The screenshots the game test takes (src/gametest): all of them, or it did not run through. */
export const GAMETEST_SHOTS = [
  "overlay-title",
  "overlay-world",
  "overlay-pause",
  "overlay-panel",
  "overlay-emoji",
];
const MOJANG_VERSIONS = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const MODRINTH_FABRIC_API = "https://api.modrinth.com/v2/project/P7dR8mSH/version";
const USER_AGENT = "projects-gg/MinecraftLauncher overlay-auto (github.com/projects-gg)";

// --- versions -------------------------------------------------------------------------------

/** A release's numbers ("26.4.1" → [26, 4, 1]); snapshots, pre-releases and others: null. */
export function releaseNumbers(version) {
  return /^\d+(\.\d+){1,2}$/.test(version) ? version.split(".").map(Number) : null;
}

/** Release order (missing parts are 0); anything that is not a release sorts first. */
export function compareReleases(a, b) {
  const x = releaseNumbers(a);
  const y = releaseNumbers(b);
  if (!x || !y) return x ? 1 : y ? -1 : 0;
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/**
 * Whether Fabric's `~<base>` range (the only kind build.gradle writes) takes `version`: the same
 * major and minor with that patch or a later one (`~26.4` takes 26.4.1, `~26.1.2` takes 26.1.3).
 */
export function tildeTakes(range, version) {
  const base = range.startsWith("~") ? releaseNumbers(range.slice(1)) : null;
  const have = releaseNumbers(version);
  if (!base || !have) return false;
  return have[0] === base[0] && have[1] === base[1] && (have[2] ?? 0) >= (base[2] ?? 0);
}

/** "26.4.1" → "26.4". */
const minorOf = (version) => version.split(".").slice(0, 2).join(".");

/** Whether an issue title names `minecraft` exactly (the titles say "Minecraft <version>:"). */
export function namesVersion(title, minecraft) {
  return String(title).includes(`Minecraft ${minecraft}:`);
}

/**
 * The versions to try. `releases`: Mojang's release ids; `fabricApi`: version → the newest Fabric
 * API release for it (null while there is none); `issues`: titles of the runs' issues;
 * `published`: Minecraft versions the live index has a jar for. Answers `{ tries, waiting, left }`.
 */
export function planTries({ targets, releases, fabricApi, issues = [], published = [] }) {
  const newest = targets
    .map((t) => t.minecraft)
    .sort(compareReleases)
    .at(-1);
  const compat = lastCompat(targets);
  const covered = (version) => targets.some((t) => tildeTakes(t.range, version));
  const firsts = new Map();
  for (const version of releases) {
    if (!releaseNumbers(version) || covered(version)) continue;
    if (newest && compareReleases(version, newest) <= 0) continue;
    const minor = minorOf(version);
    const seen = firsts.get(minor);
    if (!seen || compareReleases(version, seen) < 0) firsts.set(minor, version);
  }
  const tries = [];
  const waiting = [];
  const left = [];
  for (const version of [...firsts.values()].sort(compareReleases)) {
    if (published.includes(version) || issues.some((title) => namesVersion(title, version))) {
      left.push(version);
      continue;
    }
    const api = fabricApi.get(version);
    if (!api) {
      waiting.push(version);
      continue;
    }
    tries.push({ minecraft: version, fabricApi: api, compat, range: `~${version}` });
  }
  return { tries, waiting, left };
}

/** The compatibility class of the newest target: what a new version is tried with first. */
export function lastCompat(targets) {
  const sorted = [...targets].sort((a, b) => compareReleases(a.minecraft, b.minecraft));
  const compat = sorted.at(-1)?.compat;
  if (!compat) throw new Error("build.gradle's newest target names no compatibility class");
  return compat;
}

/** The try of one named version: its own row when build.gradle has one, else the last class. */
export function tryOf(targets, minecraft, fabricApi) {
  const own = targets.find((t) => t.key === minecraft || t.minecraft === minecraft);
  if (own) {
    return {
      minecraft: own.minecraft,
      fabricApi: own.fabricApi,
      compat: own.compat,
      range: own.range,
      key: own.key,
    };
  }
  if (!fabricApi) throw new Error(`no Fabric API release for Minecraft ${minecraft} yet`);
  return { minecraft, fabricApi, compat: lastCompat(targets), range: `~${minecraft}` };
}

/** The try of build.gradle's newest target: a known-good run of the whole chain (a push's dry run). */
export function newestTry(targets) {
  const newest = [...targets].sort((a, b) => compareReleases(a.minecraft, b.minecraft)).at(-1);
  if (!newest) throw new Error("build.gradle lists no target");
  return tryOf(targets, newest.key);
}

/**
 * The try's fields go into Gradle's command line and the workflow's environment: versions and
 * class names only (what Mojang, Modrinth or a dispatch hands over is checked here).
 */
export function checkTry(entry) {
  const safe = /^[A-Za-z0-9][A-Za-z0-9.+_~-]{0,63}$/;
  for (const field of ["minecraft", "fabricApi", "compat", "range", "key"]) {
    const value = entry[field];
    if (value === undefined && field === "key") continue;
    if (typeof value !== "string" || !safe.test(value.replace(/^~/, "x"))) {
      throw new Error(
        `the try's ${field} ${JSON.stringify(value)} is not a version or a class name`,
      );
    }
  }
  if (!/^mc\d+(_\d+)*$/.test(entry.compat))
    throw new Error(`no compatibility class ${entry.compat}`);
  return entry;
}

/** The newest Fabric API release in Modrinth's list for a version (newest first by date). */
export function newestFabricApi(versions) {
  const releases = versions
    .filter((v) => v.version_type === "release" && typeof v.version_number === "string")
    .sort((a, b) => String(b.date_published).localeCompare(String(a.date_published)));
  return releases[0]?.version_number ?? null;
}

// --- report ---------------------------------------------------------------------------------

/** JUnit XML (Gradle's `build/test-results/test/*.xml`) summed: passed, failed, skipped. */
export function junitTotals(xmlTexts) {
  const totals = { passed: 0, failed: 0, skipped: 0 };
  for (const xml of xmlTexts) {
    for (const suite of xml.matchAll(/<testsuite\b([^>]*)>/g)) {
      const attr = (name) => Number(new RegExp(`\\b${name}="(\\d+)"`).exec(suite[1])?.[1] ?? 0);
      const tests = attr("tests");
      const failed = attr("failures") + attr("errors");
      const skipped = attr("skipped");
      totals.failed += failed;
      totals.skipped += skipped;
      totals.passed += Math.max(0, tests - failed - skipped);
    }
  }
  return totals;
}

/** Both suites ran something and nothing failed (pl-content `OverlayCiTests::passed`). */
export function testsPassed(tests) {
  return [tests.junit, tests.gametest].every((count) => count.passed > 0 && count.failed === 0);
}

const sha = (algorithm, bytes) => createHash(algorithm).update(bytes).digest("hex");

/**
 * The report's entry for a tested jar, from the jar itself: what Studio checks again on import
 * (docs/content-format.md, "overlay-release.json").
 */
export function describeTested(
  file,
  bytes,
  { minecraft, range, fabricApi, loader, compat, tests },
) {
  const meta = modJson(bytes);
  if (meta.id !== MOD_ID) throw new Error(`${file}: mod id ${meta.id}, not ${MOD_ID}`);
  if (meta.depends?.minecraft !== range) {
    throw new Error(`${file}: Minecraft range ${meta.depends?.minecraft}, the try says ${range}`);
  }
  if (
    !readZipEntry(bytes, LICENSE_ENTRY)?.toString("utf8").includes("GNU GENERAL PUBLIC LICENSE")
  ) {
    throw new Error(`${file}: no GPL text (${LICENSE_ENTRY})`);
  }
  const test = zipEntryNames(bytes).find((name) => DEV_ENTRIES.some((re) => re.test(name)));
  if (test) throw new Error(`${file}: carries test code (${test})`);
  return {
    minecraft,
    range,
    version: String(meta.version),
    proto: bridgeProto(meta, file),
    file,
    sha1: sha("sha1", bytes),
    sha256: sha("sha256", bytes),
    size: bytes.length,
    fabricApi,
    loader,
    ...(compat ? { compat } : {}),
    tests,
  };
}

/** `overlay-release.json` (pl-content `OverlayCiRelease`). */
export function buildReport({ commit, run, testedAt, jars }) {
  if (!/^[0-9a-f]{40}$/.test(commit ?? "")) throw new Error(`commit ${commit} is not a full SHA`);
  return {
    schema: REPORT_SCHEMA,
    modId: MOD_ID,
    commit,
    ...(run ? { run } : {}),
    testedAt,
    jars,
  };
}

// --- issues ---------------------------------------------------------------------------------

/** The title and body of a run's issue: facts only (the repository is public). */
export function issueText(kind, data) {
  const { minecraft } = data;
  if (kind === "ready") {
    const jar = data.report?.jars?.find((entry) => entry.minecraft === minecraft);
    if (!jar) throw new Error(`the report has no jar for Minecraft ${minecraft}`);
    const count = (c) => `${c.passed} passed, ${c.failed} failed, ${c.skipped} skipped`;
    const lines = [
      `The in-game overlay's jar for Minecraft ${minecraft} was built and tested.`,
      "",
      `- Jar: \`${jar.file}\``,
      `- SHA-256: \`${jar.sha256}\``,
      `- Bridge protocol: ${jar.proto}`,
      `- Built with: Fabric API ${jar.fabricApi}, Fabric Loader ${jar.loader}${jar.compat ? `, compatibility class \`${jar.compat}\`` : ""}`,
      `- JUnit: ${count(jar.tests.junit)}`,
      `- Client game test: ${count(jar.tests.gametest)}`,
      `- Commit: ${data.report.commit}`,
    ];
    if (data.artifact) lines.push(`- Artifact: ${data.artifact}`);
    if (data.run) lines.push(`- Run: ${data.run}`);
    lines.push(
      "",
      "Next: import the artifact in Projects Studio (Katman modu), which checks the jar again, then publish.",
    );
    return {
      title: `Overlay: Minecraft ${minecraft}: tested jar ready`,
      labels: [LABEL, LABEL_READY],
      body: lines.join("\n"),
    };
  }
  if (kind === "compat") {
    const lines = [
      `Building or testing the in-game overlay for Minecraft ${minecraft} failed with the last compatibility class.`,
      "",
      `- Tried with: Fabric API ${data.fabricApi ?? "?"}${data.compat ? `, compatibility class \`${data.compat}\`` : ""}`,
    ];
    if (data.step) lines.push(`- Failed step: ${data.step}`);
    if (data.run) lines.push(`- Run: ${data.run}`);
    if (data.log) {
      lines.push("", "First errors:", "", "```", data.log, "```");
    }
    lines.push(
      "",
      `Next: add \`src/mc${minecraft.replaceAll(".", "_")}/java/gg/projects/overlay/compat/\` and a row in \`mods/projects-overlay/build.gradle\`, then run the workflow for ${minecraft}.`,
    );
    return {
      title: `Overlay: Minecraft ${minecraft}: needs a compat class`,
      labels: [LABEL, LABEL_COMPAT],
      body: lines.join("\n"),
    };
  }
  throw new UsageError(`unknown issue kind ${kind}`);
}

/** The lines of a build log worth showing in an issue: compiler and test errors, at most `max`. */
export function errorLines(log, max = 20) {
  return log
    .split(/\r?\n/)
    .filter((line) =>
      /(error:|\bFAILED\b|Exception|Caused by:|Mixin .*fail|What went wrong)/i.test(line),
    )
    .map((line) => line.replace(/\s+$/, "").slice(0, 300))
    .slice(0, max)
    .join("\n");
}

// --- main -----------------------------------------------------------------------------------

async function getJson(url) {
  const response = await fetch(url, {
    headers: { "User-Agent": USER_AGENT, Accept: "application/json" },
  });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return response.json();
}

async function fabricApiFor(minecraft) {
  const query = new URLSearchParams({
    loaders: JSON.stringify(["fabric"]),
    game_versions: JSON.stringify([minecraft]),
  });
  return newestFabricApi(await getJson(`${MODRINTH_FABRIC_API}?${query}`));
}

function output(name, value) {
  const line = `${name}=${typeof value === "string" ? value : JSON.stringify(value)}`;
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `${line}\n`);
  else console.log(line);
}

const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));

async function watch(flags) {
  const targets = parseTargets(readFileSync(join(MOD_DIR, "build.gradle"), "utf8"));
  let tries;
  if (flags.newest) {
    tries = [newestTry(targets)];
  } else if (flags.minecraft) {
    const own = targets.some((t) => t.key === flags.minecraft || t.minecraft === flags.minecraft);
    const api = own ? null : await fabricApiFor(flags.minecraft);
    tries = [tryOf(targets, flags.minecraft, api)];
  } else {
    const manifest = await getJson(MOJANG_VERSIONS);
    const releases = manifest.versions.filter((v) => v.type === "release").map((v) => v.id);
    const issues = flags.issues ? readJson(flags.issues).map((issue) => issue.title) : [];
    const published = flags.published
      ? (readJson(flags.published).overlay?.jars ?? []).map((jar) => jar.minecraft)
      : [];
    const candidates = planTries({ targets, releases, fabricApi: new Map(), issues, published });
    const apis = new Map();
    for (const version of candidates.waiting) apis.set(version, await fabricApiFor(version));
    const plan = planTries({ targets, releases, fabricApi: apis, issues, published });
    for (const version of plan.waiting)
      console.log(`Minecraft ${version}: no Fabric API release yet`);
    for (const version of plan.left)
      console.log(`Minecraft ${version}: already has an issue or a published jar`);
    tries = plan.tries;
  }
  for (const t of tries) {
    checkTry(t);
    console.log(`try Minecraft ${t.minecraft} with ${t.compat}, Fabric API ${t.fabricApi}`);
  }
  output("tries", { include: tries });
  output("any", tries.length > 0 ? "true" : "false");
}

function report(flags) {
  for (const name of ["jar", "junit", "shots", "minecraft", "fabric-api", "compat", "out"]) {
    if (!flags[name]) throw new UsageError(`--${name} is needed`);
  }
  const bytes = readFileSync(flags.jar);
  const xml = existsSync(flags.junit)
    ? readdirSync(flags.junit)
        .filter((name) => name.endsWith(".xml"))
        .map((name) => readFileSync(join(flags.junit, name), "utf8"))
    : [];
  const shots = existsSync(flags.shots) ? readdirSync(flags.shots) : [];
  const ranThrough = GAMETEST_SHOTS.every((shot) => shots.includes(`${shot}.png`));
  const tests = {
    junit: junitTotals(xml),
    gametest: { passed: ranThrough ? 1 : 0, failed: ranThrough ? 0 : 1, skipped: 0 },
  };
  const loader = /^loader_version\s*=\s*(\S+)/m.exec(
    readFileSync(join(MOD_DIR, "gradle.properties"), "utf8"),
  )?.[1];
  const file = basename(flags.jar);
  const entry = describeTested(file, bytes, {
    minecraft: flags.minecraft,
    range: flags.range ?? `~${flags.minecraft}`,
    fabricApi: flags["fabric-api"],
    loader,
    compat: flags.compat,
    tests,
  });
  const server = process.env.GITHUB_SERVER_URL;
  const repo = process.env.GITHUB_REPOSITORY;
  const runId = process.env.GITHUB_RUN_ID;
  const doc = buildReport({
    commit: process.env.GITHUB_SHA,
    run: server && repo && runId ? `${server}/${repo}/actions/runs/${runId}` : undefined,
    testedAt: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
    jars: [entry],
  });
  mkdirSync(flags.out, { recursive: true });
  copyFileSync(flags.jar, join(flags.out, file));
  writeFileSync(join(flags.out, REPORT_FILE), `${JSON.stringify(doc, null, 2)}\n`);
  console.log(
    `${REPORT_FILE}: ${file} (${entry.sha256}), JUnit ${tests.junit.passed} passed, game test ${ranThrough ? "passed" : "did not run through"}`,
  );
  if (!testsPassed(tests)) throw new Error("the jar is not tested: a suite failed or did not run");
}

function issue(flags) {
  if (!flags.kind || !flags.minecraft) throw new UsageError("--kind and --minecraft are needed");
  const text = issueText(flags.kind, {
    minecraft: flags.minecraft,
    report: flags.report ? readJson(flags.report) : undefined,
    fabricApi: flags["fabric-api"],
    compat: flags.compat,
    step: flags.step,
    run: flags.run,
    artifact: flags.artifact,
    log:
      flags.log && existsSync(flags.log) ? errorLines(readFileSync(flags.log, "utf8")) : undefined,
  });
  output("title", text.title);
  output("labels", text.labels.join(","));
  const body = flags.out ?? "overlay-issue.md";
  writeFileSync(body, `${text.body}\n`);
  output("body", resolve(body));
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  run(async () => {
    const [command, ...rest] = process.argv.slice(2);
    const { flags, positional } = parseArgs(rest, {
      newest: "boolean",
      minecraft: "string",
      issues: "string",
      published: "string",
      jar: "string",
      junit: "string",
      shots: "string",
      "fabric-api": "string",
      compat: "string",
      range: "string",
      out: "string",
      kind: "string",
      report: "string",
      step: "string",
      run: "string",
      artifact: "string",
      log: "string",
    });
    if (positional.length) throw new UsageError(`unexpected ${positional.join(" ")}`);
    if (command === "watch") await watch(flags);
    else if (command === "report") report(flags);
    else if (command === "issue") issue(flags);
    else throw new UsageError(command ? `unknown command ${command}` : "no command");
  }, USAGE);
}
