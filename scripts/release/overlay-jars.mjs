#!/usr/bin/env node
// The in-game overlay's jars the launcher ships (OVL-B): one per Minecraft version the mod builds
// for (`mods/projects-overlay/build.gradle` → `targets`), plus `index.json`, in the launcher's
// resource folder `apps/launcher/src-tauri/overlay/` (git-ignored). The launcher reads the index
// (`pl_overlay::install::Bundle`) and puts the jar for an instance's version in place before a start.
//
//   node scripts/release/overlay-jars.mjs build [--test] [--force] [--out <dir>]
//   node scripts/release/overlay-jars.mjs check [--out <dir>] [--source]
//
// build   Builds every version with Gradle (`assemble`; `--test` runs `build`, JUnit included) unless
//         the folder already holds jars of the current source (`--force` builds anyway), copies the
//         jars and writes the index. Needs a JDK 25 (`JAVA_HOME`); Gradle's home follows
//         `GRADLE_USER_HOME`.
// check   The folder holds one sound jar per version of build.gradle, as the index says (size,
//         SHA-1, the jar's own fabric.mod.json); `--source` also requires the jars to be built from
//         the mod's current source. CI runs it before `tauri build`; a shipping release build of
//         the launcher refuses to compile without an index (apps/launcher/src-tauri/build.rs).

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { inflateRawSync } from "node:zlib";

import { parseArgs, run, UsageError } from "./lib.mjs";

const USAGE = `usage: node scripts/release/overlay-jars.mjs build [--test] [--force] [--out <dir>]
       node scripts/release/overlay-jars.mjs check [--out <dir>] [--source]`;

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const MOD_DIR = join(ROOT, "mods", "projects-overlay");
export const OUT_DIR = join(ROOT, "apps", "launcher", "src-tauri", "overlay");
export const MOD_ID = "projects_overlay";
/** The GPL text every jar carries at its root (build.gradle's `jar`, Fabric's template name). */
export const LICENSE_ENTRY = "LICENSE_projects-overlay";
export const INDEX_FILE = "index.json";
export const SCHEMA = 1;

// --- build.gradle ---------------------------------------------------------------------------

/**
 * The Minecraft versions build.gradle builds for, in its order: `targets` rows such as
 * `'26.2'  : [minecraft: '26.2', fabricApi: '0.161.0+26.2', range: '~26.2', compat: 'mc26_2'],`.
 */
export function parseTargets(gradle) {
  const block = /def targets = \[([\s\S]*?)\n\]/.exec(gradle);
  if (!block) throw new Error("build.gradle has no `def targets = [ ... ]` block");
  const row =
    /'([^']+)'\s*:\s*\[\s*minecraft:\s*'([^']+)',\s*fabricApi:\s*'([^']+)',\s*range:\s*'([^']+)'/g;
  const targets = [...block[1].matchAll(row)].map((m) => ({
    key: m[1],
    minecraft: m[2],
    fabricApi: m[3],
    range: m[4],
  }));
  if (!targets.length) throw new Error("build.gradle lists no Minecraft version in `targets`");
  return targets;
}

/** `mod_version` from gradle.properties. */
export function parseModVersion(properties) {
  const m = /^mod_version\s*=\s*(\S+)\s*$/m.exec(properties);
  if (!m) throw new Error("gradle.properties has no mod_version");
  return m[1];
}

// --- jars -----------------------------------------------------------------------------------

/** One entry of a zip (stored or deflated; the jars are small, no zip64). `null` when absent. */
export function readZipEntry(zip, name) {
  let eocd = -1;
  for (let i = zip.length - 22; i >= Math.max(0, zip.length - 65_557); i--) {
    if (zip.readUInt32LE(i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error("not a zip file");
  const count = zip.readUInt16LE(eocd + 10);
  let p = zip.readUInt32LE(eocd + 16);
  for (let n = 0; n < count; n++) {
    if (zip.readUInt32LE(p) !== 0x02014b50) throw new Error("broken zip directory");
    const method = zip.readUInt16LE(p + 10);
    const size = zip.readUInt32LE(p + 20);
    const nameLength = zip.readUInt16LE(p + 28);
    const extraLength = zip.readUInt16LE(p + 30);
    const commentLength = zip.readUInt16LE(p + 32);
    const local = zip.readUInt32LE(p + 42);
    if (zip.toString("utf8", p + 46, p + 46 + nameLength) === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + size);
      if (method === 0) return Buffer.from(data);
      if (method === 8) return inflateRawSync(data);
      throw new Error(`zip entry ${name} uses compression method ${method}`);
    }
    p += 46 + nameLength + extraLength + commentLength;
  }
  return null;
}

/** Every entry name of a zip, in its directory's order. */
export function zipEntryNames(zip) {
  let eocd = -1;
  for (let i = zip.length - 22; i >= Math.max(0, zip.length - 65_557); i--) {
    if (zip.readUInt32LE(i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error("not a zip file");
  const names = [];
  const count = zip.readUInt16LE(eocd + 10);
  let p = zip.readUInt32LE(eocd + 16);
  for (let n = 0; n < count; n++) {
    if (zip.readUInt32LE(p) !== 0x02014b50) throw new Error("broken zip directory");
    const nameLength = zip.readUInt16LE(p + 28);
    names.push(zip.toString("utf8", p + 46, p + 46 + nameLength));
    p += 46 + nameLength + zip.readUInt16LE(p + 30) + zip.readUInt16LE(p + 32);
  }
  return names;
}

/**
 * The scenario driver of local test runs (build.gradle's `dev` source set: scripted scenarios, the
 * end-to-end rigs' hooks) is never part of a jar the launcher ships.
 */
export const DEV_ENTRIES = [/^gg\/projects\/overlay\/dev\//, /^projects_overlay_dev\./];

/** The jar's own `fabric.mod.json`. */
export function modJson(jar) {
  const entry = readZipEntry(jar, "fabric.mod.json");
  if (!entry) throw new Error("no fabric.mod.json");
  return JSON.parse(entry.toString("utf8"));
}

export const sha1 = (bytes) => createHash("sha1").update(bytes).digest("hex");

/** The index entry of a built jar (file name as Gradle names it). */
export function describeJar(file, bytes, target) {
  const meta = modJson(bytes);
  if (meta.id !== MOD_ID) throw new Error(`${file}: mod id ${meta.id}, not ${MOD_ID}`);
  const range = meta.depends?.minecraft;
  if (range !== target.range) {
    throw new Error(`${file}: Minecraft range ${range}, build.gradle says ${target.range}`);
  }
  // The mod is GPL-3.0-only: a jar without the license text does not ship.
  const license = readZipEntry(bytes, LICENSE_ENTRY);
  if (!license?.toString("utf8").includes("GNU GENERAL PUBLIC LICENSE")) {
    throw new Error(`${file}: no GPL text (${LICENSE_ENTRY})`);
  }
  const dev = zipEntryNames(bytes).find((name) => DEV_ENTRIES.some((re) => re.test(name)));
  if (dev) throw new Error(`${file}: carries the test scenario driver (${dev})`);
  return {
    minecraft: target.minecraft,
    range,
    file,
    version: String(meta.version),
    sha1: sha1(bytes),
    size: bytes.length,
  };
}

// --- source stamp ---------------------------------------------------------------------------

/**
 * The files a jar is made of: the build files, main and per-version sources, the stickers, the
 * chat emoji list it carries (CHAT-EMOJI, the launcher's built-in one), the license text.
 */
export function sourceFiles(root = ROOT) {
  const mod = join(root, "mods", "projects-overlay");
  const out = [join(root, "LICENSE")];
  const walk = (dir) => {
    if (!existsSync(dir)) return;
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile()) out.push(path);
    }
  };
  for (const file of ["build.gradle", "gradle.properties", "settings.gradle"]) {
    out.push(join(mod, file));
  }
  walk(join(mod, "src", "main"));
  if (existsSync(join(mod, "src"))) {
    for (const entry of readdirSync(join(mod, "src"), { withFileTypes: true })) {
      if (entry.isDirectory() && /^mc\d/.test(entry.name))
        walk(join(mod, "src", entry.name, "java"));
    }
  }
  const stickers = join(root, "packages", "stickers", "pl1");
  out.push(join(stickers, "manifest.json"), join(stickers, "overlay", "pl1-atlas.png"));
  out.push(join(root, "crates", "pl-content", "data", "chat-emojis.json"));
  return out
    .filter((path) => existsSync(path))
    .map((path) => relative(root, path).split(sep).join("/"))
    .sort();
}

/** SHA-256 over the files' paths and contents (line endings as they are on disk). */
export function sourceStamp(root = ROOT) {
  const hash = createHash("sha256");
  for (const path of sourceFiles(root)) {
    hash.update(`${path}\0`);
    hash.update(
      createHash("sha256")
        .update(readFileSync(join(root, path)))
        .digest("hex"),
    );
    hash.update("\n");
  }
  return hash.digest("hex");
}

// --- check ----------------------------------------------------------------------------------

/**
 * Problems of the folder `dir` for `targets` (empty: sound). `stamp`, when given, must be the
 * index's source stamp.
 */
export function checkBundle(dir, targets, stamp) {
  const problems = [];
  const indexPath = join(dir, INDEX_FILE);
  if (!existsSync(indexPath)) return [`${INDEX_FILE} is missing in ${dir}`];
  let index;
  try {
    index = JSON.parse(readFileSync(indexPath, "utf8"));
  } catch (error) {
    return [`${INDEX_FILE} is unreadable: ${error.message}`];
  }
  if (index.schema !== SCHEMA) problems.push(`${INDEX_FILE} has schema ${index.schema}`);
  const jars = Array.isArray(index.jars) ? index.jars : [];
  const listed = jars.map((jar) => jar.minecraft).join(", ");
  const wanted = targets.map((t) => t.minecraft).join(", ");
  if (listed !== wanted)
    problems.push(`the jars are for ${listed || "nothing"}, build.gradle builds ${wanted}`);
  for (const jar of jars) {
    const target = targets.find((t) => t.minecraft === jar.minecraft);
    const path = join(dir, String(jar.file));
    if (!/^[A-Za-z0-9._+-]+\.jar$/.test(String(jar.file)) || !existsSync(path)) {
      problems.push(`${jar.file} is missing`);
      continue;
    }
    const bytes = readFileSync(path);
    if (bytes.length !== jar.size || sha1(bytes) !== jar.sha1) {
      problems.push(`${jar.file} is not the file the index names`);
      continue;
    }
    if (!target) continue;
    try {
      const described = describeJar(jar.file, bytes, target);
      if (described.version !== jar.version)
        problems.push(`${jar.file}: version ${described.version}, index says ${jar.version}`);
    } catch (error) {
      problems.push(error.message);
    }
  }
  if (stamp !== undefined && index.source !== stamp) {
    problems.push("the jars are older than the mod's source (build them again)");
  }
  return problems;
}

// --- build ----------------------------------------------------------------------------------

function gradle(args) {
  const windows = process.platform === "win32";
  // A full path, not a bare name: cmd.exe may be told not to look in the current folder. Quoted
  // for the shell (the checkout's path may hold spaces); the arguments never do.
  const wrapper = windows ? `"${join(MOD_DIR, "gradlew.bat")}"` : "./gradlew";
  const result = spawnSync(wrapper, args, {
    cwd: MOD_DIR,
    stdio: "inherit",
    shell: windows,
  });
  if (result.status !== 0)
    throw new Error(`gradle ${args.join(" ")} failed (exit ${result.status})`);
}

function build({ test, force, out }) {
  const targets = parseTargets(readFileSync(join(MOD_DIR, "build.gradle"), "utf8"));
  const stamp = sourceStamp();
  if (!force && checkBundle(out, targets, stamp).length === 0) {
    console.log(
      `overlay jars up to date in ${out} (${targets.map((t) => t.minecraft).join(", ")})`,
    );
    return;
  }
  const modVersion = parseModVersion(readFileSync(join(MOD_DIR, "gradle.properties"), "utf8"));
  const jars = [];
  for (const target of targets) {
    // Relative to the mod's folder (no spaces reach cmd.exe on Windows); one folder per version.
    const buildDir = `build/bundle-${target.key}`;
    gradle([
      test ? "build" : "assemble",
      "--no-daemon",
      "--console=plain",
      `-Pmc=${target.key}`,
      `-Povl.buildDir=${buildDir}`,
    ]);
    const file = `projects-overlay-${modVersion}+${target.minecraft}.jar`;
    const built = join(MOD_DIR, buildDir, "libs", file);
    if (!existsSync(built)) throw new Error(`Gradle made no ${file} in ${dirname(built)}`);
    jars.push({ built, entry: describeJar(file, readFileSync(built), target) });
  }
  mkdirSync(out, { recursive: true });
  for (const name of readdirSync(out)) {
    if (/^projects-overlay-.*\.jar$/.test(name) || name === INDEX_FILE) rmSync(join(out, name));
  }
  for (const { built, entry } of jars) copyFileSync(built, join(out, entry.file));
  const index = { schema: SCHEMA, modId: MOD_ID, source: stamp, jars: jars.map((j) => j.entry) };
  writeFileSync(join(out, INDEX_FILE), `${JSON.stringify(index, null, 2)}\n`);
  const total = jars.reduce((sum, j) => sum + j.entry.size, 0);
  console.log(
    `overlay jars in ${out}: ${jars.map((j) => j.entry.file).join(", ")} (${total} bytes)`,
  );
}

function check({ out, source }) {
  const targets = parseTargets(readFileSync(join(MOD_DIR, "build.gradle"), "utf8"));
  const problems = checkBundle(out, targets, source ? sourceStamp() : undefined);
  if (problems.length) {
    throw new Error(
      `the in-game overlay's jars are not ready:\n  - ${problems.join("\n  - ")}\n` +
        "Build them: node scripts/release/overlay-jars.mjs build",
    );
  }
  const index = JSON.parse(readFileSync(join(out, INDEX_FILE), "utf8"));
  const total = index.jars.reduce((sum, jar) => sum + jar.size, 0);
  console.log(`overlay jars ok: ${index.jars.map((jar) => jar.file).join(", ")} (${total} bytes)`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  run(async () => {
    const [command, ...rest] = process.argv.slice(2);
    const { flags, positional } = parseArgs(rest, {
      test: "boolean",
      force: "boolean",
      source: "boolean",
      out: "string",
    });
    if (positional.length) throw new UsageError(`unexpected ${positional.join(" ")}`);
    const out = flags.out ? resolve(flags.out) : OUT_DIR;
    if (command === "build") build({ test: Boolean(flags.test), force: Boolean(flags.force), out });
    else if (command === "check") check({ out, source: Boolean(flags.source) });
    else throw new UsageError(command ? `unknown command ${command}` : "no command");
  }, USAGE);
}
