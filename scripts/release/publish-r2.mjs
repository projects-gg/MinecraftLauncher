#!/usr/bin/env node
// Publishes launcher releases to Cloudflare R2 (docs/content-format.md, "Launcher release
// channels"). See docs/release.md for the whole flow.
//
//   publish  Upload every file of --dir to v1/releases/<version>/ (immutable: identical files are
//            skipped, different bytes under an existing name are refused), verify each object,
//            then write v1/releases/<channel>.json last (the commit point).
//   promote  Copy the release announced in <from>.json (it must be --version) to <to>.json with
//            the rollout / mandatory / minVersion you choose. beta -> stable, or stable -> stable
//            to widen a staged rollout. Its notes go along unchanged (`localizedNotes` too).
//   prune    Delete old release folders (also run after every publish and promote unless
//            --no-prune): a folder goes once a newer one is 2 days old, unless stable.json or
//            beta.json names it or it is the release before stable (latest.php's previous stable,
//            and what a rollback with --allow-downgrade needs). Nothing is deleted when a channel
//            file or the listing cannot be read; a failed clean-up never fails the release.
//            Every release also stays on GitHub Releases (projects-gg/MinecraftLauncher).
//   show     Print a channel file.
//
// --notes-file: the release notes as the tag message gives them, Turkish, then optionally a
// `--- en ---` line and the English notes (lib.mjs `parseNotes`, docs/release.md).
// --expect-current: publish only while the channel still announces that version (or already this
// one: a run again). A stable taken from a beta goes to the beta channel this way, so a newer beta
// that came meanwhile is never replaced by the older beta's code.
//
//   node scripts/release/publish-r2.mjs publish --dir release-files --version 2.1.0 \
//     [--channel beta|stable|none] [--notes-file notes.md] [--rollout 100] [--mandatory] \
//     [--min-version 2.0.3|none] [--require windows-x86_64,...] [--expect-current 2.1.0-beta.2] \
//     [--allow-downgrade] [--no-prune] [--dry-run]
//   node scripts/release/publish-r2.mjs promote --version 2.1.0 [--from beta] [--to stable] \
//     [--rollout 25] [--mandatory[=false]] [--min-version 2.0.3|none] [--allow-downgrade] \
//     [--no-prune] [--dry-run]
//   node scripts/release/publish-r2.mjs prune [--dry-run]
//   node scripts/release/publish-r2.mjs show [--channel stable]
//
// Environment: R2_ACCOUNT_ID, R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY, R2_BUCKET (an R2 API token
// with Object Read & Write on that bucket). Optional: CDN_BASE_URL (default
// https://cdn.projects.gg), R2_PREFIX (default v1), R2_ENDPOINT. --dry-run never writes; it
// still reads R2 when credentials are present, and `publish --dry-run` also works without them.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import {
  buildManifest,
  canonicalJson,
  compareVersions,
  contentType,
  md5Hex,
  parseArgs,
  parseVersion,
  planFolderPrune,
  previousRelease,
  R2Client,
  Refusal,
  readBundles,
  readNotes,
  requirePlatforms,
  rfc3339,
  run,
  sha256Hex,
  UsageError,
  versionFolders,
} from "./lib.mjs";

const USAGE = `usage: node scripts/release/publish-r2.mjs <publish|promote|prune|show> [options]
  publish --dir <dir> --version <v> [--channel beta|stable|none] [--notes-file <md>]
          [--rollout 0-100] [--mandatory] [--min-version <v>|none] [--require <keys>]
          [--expect-current <v>] [--allow-downgrade] [--no-prune] [--dry-run]
  promote --version <v> [--from beta] [--to stable] [--rollout 0-100] [--mandatory[=false]]
          [--min-version <v>|none] [--allow-downgrade] [--no-prune] [--dry-run]
  prune   [--dry-run]
  show    [--channel stable]`;

const CDN = (process.env.CDN_BASE_URL || "https://cdn.projects.gg").replace(/\/+$/, "");
const PREFIX = (process.env.R2_PREFIX || "v1").replace(/^\/+|\/+$/g, "");
const CHANNELS = ["stable", "beta"];
const IMMUTABLE = "public, max-age=31536000, immutable";
const MUTABLE = "public, max-age=60";
const JSON_TYPE = "application/json; charset=utf-8";
/** An unnamed release folder stays this long after a newer one is uploaded. */
const PRUNE_GRACE_MS = 2 * 24 * 60 * 60 * 1000;

const channelKey = (channel) => `${PREFIX}/releases/${channel}.json`;
const releaseDir = (version) => `${PREFIX}/releases/${version}/`;

function checkChannel(name) {
  if (!CHANNELS.includes(name)) throw new UsageError(`unknown channel "${name}" (${CHANNELS})`);
  return name;
}

function rolloutFlag(value) {
  if (value === undefined) return undefined;
  const n = Number(value);
  if (!Number.isInteger(n) || n < 0 || n > 100) throw new UsageError("--rollout must be 0-100");
  return n;
}

async function readChannel(r2, channel) {
  const bytes = await r2.get(channelKey(channel));
  if (!bytes) return null;
  const doc = JSON.parse(bytes.toString("utf8"));
  parseVersion(doc.version);
  return doc;
}

function guardDowngrade(channel, current, version, allow) {
  if (!current) return;
  const cmp = compareVersions(version, current.version);
  if (cmp < 0 && !allow) {
    throw new Error(
      `${channel} announces ${current.version}; refusing to move it back to ${version} (--allow-downgrade)`,
    );
  }
  if (cmp === 0) console.log(`note: ${channel} already announces ${version}; rewriting it`);
}

/** `--expect-current`: the channel must still announce `expected`, or already `version`. */
function guardExpected(channel, current, version, expected) {
  if (expected === undefined) return;
  const want = parseVersion(expected).raw;
  if (current?.version === want || current?.version === version) return;
  throw new Refusal(
    "release/channel-moved",
    `${channel} announces ${current?.version ?? "nothing"}, not ${want}; refusing to replace it with ${version}`,
  );
}

async function writeChannel(r2, channel, doc, dryRun) {
  const json = canonicalJson(doc);
  const key = channelKey(channel);
  if (dryRun) {
    console.log(`\n[dry-run] would write ${key}:\n${json}`);
    return;
  }
  const body = Buffer.from(json);
  await r2.put(key, body, {
    contentType: JSON_TYPE,
    cacheControl: MUTABLE,
    sha256: sha256Hex(body),
  });
  const back = await r2.get(key);
  if (!back?.equals(body)) throw new Error(`read-back of ${key} does not match`);
  console.log(`\nwrote ${CDN}/${key}:\n${json}`);
}

async function publish(flags, r2) {
  if (!flags.dir || !flags.version) throw new UsageError("publish needs --dir and --version");
  const version = parseVersion(flags.version).raw;
  const channel = flags.channel === "none" ? null : checkChannel(flags.channel ?? "beta");
  const dryRun = Boolean(flags["dry-run"]);
  const names = readdirSync(flags.dir)
    .filter((n) => statSync(join(flags.dir, n)).isFile() && n !== "latest.json")
    .sort();
  if (!names.length) throw new Error(`no files in ${flags.dir}`);
  for (const name of names) {
    if (!/^[A-Za-z0-9._-]+$/.test(name)) throw new Error(`"${name}" is not a valid mirror name`);
  }

  // Build and check the channel document before uploading anything.
  let manifest = null;
  let current = null;
  if (channel) {
    if (r2) current = await readChannel(r2, channel);
    if (r2) guardExpected(channel, current, version, flags["expect-current"]);
    guardDowngrade(channel, current, version, flags["allow-downgrade"]);
    const minVersion = flags["min-version"] ?? current?.minVersion;
    manifest = buildManifest({
      version,
      notes: readNotes(flags["notes-file"]),
      bundles: readBundles(flags.dir),
      baseUrl: `${CDN}/${releaseDir(version)}`,
      channel,
      rollout: rolloutFlag(flags.rollout),
      mandatory: Boolean(flags.mandatory),
      minVersion: minVersion === "none" ? undefined : minVersion,
    });
    if (flags.require) requirePlatforms(manifest, flags.require.split(",").filter(Boolean));
  }

  // 1. Immutable release files.
  for (const name of names) {
    const body = readFileSync(join(flags.dir, name));
    const sha256 = sha256Hex(body);
    const md5 = md5Hex(body);
    const key = releaseDir(version) + name;
    const existing = r2 ? await r2.head(key) : null;
    if (existing) {
      if (existing.sha256 === sha256 || (!existing.sha256 && existing.etag === md5)) {
        console.log(`same     ${key}`);
        continue;
      }
      throw new Error(`${key} already exists with other content; release files are immutable`);
    }
    if (dryRun) {
      console.log(`[dry-run] would upload ${key} (${body.length} bytes, sha256 ${sha256})`);
      continue;
    }
    await r2.put(key, body, { contentType: contentType(name), cacheControl: IMMUTABLE, sha256 });
    const check = await r2.head(key);
    const etagOk = !check?.etag || check.etag.includes("-") || check.etag === md5;
    if (!check || check.size !== body.length || check.sha256 !== sha256 || !etagOk) {
      throw new Error(`verification failed for ${key}`);
    }
    console.log(`uploaded ${key} (${body.length} bytes)`);
  }

  // 2. The channel file, last, after reading it again: uploading took minutes, and a release that
  // reached the channel meanwhile (a newer beta) is not replaced.
  if (manifest) {
    if (r2) {
      const now = await readChannel(r2, channel);
      guardExpected(channel, now, version, flags["expect-current"]);
      if (now?.version !== current?.version) {
        guardDowngrade(channel, now, version, flags["allow-downgrade"]);
      }
    }
    await writeChannel(r2, channel, manifest, dryRun);
  }
  if (r2 && !flags["no-prune"]) await prune(flags, r2);
}

async function promote(flags, r2) {
  if (!flags.version) throw new UsageError("promote needs --version");
  if (!r2) throw new Error("promote reads R2; set the R2_* variables (also for --dry-run)");
  const version = parseVersion(flags.version).raw;
  const from = checkChannel(flags.from ?? "beta");
  const to = checkChannel(flags.to ?? "stable");
  const source = await readChannel(r2, from);
  if (!source) throw new Error(`${channelKey(from)} does not exist`);
  if (source.version !== version) {
    throw new Error(`${from} announces ${source.version}, not ${version}; publish it first`);
  }
  for (const [key, entry] of Object.entries(source.platforms ?? {})) {
    if (!entry.url.startsWith(`${CDN}/`)) {
      console.log(`note: ${key} points outside ${CDN}, not checked`);
      continue;
    }
    if (!(await r2.head(entry.url.slice(CDN.length + 1)))) {
      throw new Error(`${key}: ${entry.url} is missing from the bucket`);
    }
  }
  // Read last, after the checks above: what the target announces right before it is written.
  const target = await readChannel(r2, to);
  if (from === to && target?.version !== version) {
    throw new Error(`${to} announces ${target?.version ?? "nothing"} now, not ${version}`);
  }
  if (from !== to) guardDowngrade(to, target, version, flags["allow-downgrade"]);

  const same = from === to;
  const rollout = rolloutFlag(flags.rollout) ?? (same ? (source.rollout ?? 100) : 100);
  const mandatory = flags.mandatory ?? (same ? Boolean(source.mandatory) : false);
  const minVersion = flags["min-version"] ?? target?.minVersion;
  const doc = {
    version: source.version,
    notes: source.notes ?? "",
    localizedNotes: source.localizedNotes,
    pub_date: same ? source.pub_date : rfc3339(),
    platforms: source.platforms,
    channel: to,
    rollout,
    mandatory,
    minVersion: minVersion && minVersion !== "none" ? parseVersion(minVersion).raw : undefined,
  };
  await writeChannel(r2, to, doc, Boolean(flags["dry-run"]));
  if (!flags["no-prune"]) await prune(flags, r2);
}

/** Old release folders (see the header). Never throws once R2 is there: the release succeeded. */
async function prune(flags, r2) {
  if (!r2) throw new Error("prune reads R2; set the R2_* variables (also for --dry-run)");
  const dryRun = Boolean(flags["dry-run"]);
  try {
    const named = async () => {
      const versions = [];
      for (const channel of CHANNELS) {
        const doc = await readChannel(r2, channel);
        if (!doc) throw new Error(`${channelKey(channel)} does not exist`);
        versions.push(doc.version);
      }
      return versions;
    };
    const [stable, beta] = await named();
    const base = `${PREFIX}/releases/`;
    const objects = await r2.list(base);
    const keep = new Set([stable, beta]);
    const previous = previousRelease(versionFolders(objects, base), stable);
    if (previous) keep.add(previous);
    // A channel that moved while listing: leave it to the next run rather than guess.
    const again = await named();
    if (again[0] !== stable || again[1] !== beta) throw new Error("a channel changed meanwhile");
    const plan = planFolderPrune(objects, base, keep, Date.now(), PRUNE_GRACE_MS);
    for (const name of plan.unknown) console.log(`clean-up: ${base}${name}/ is not a version`);
    for (const { name, reason } of plan.waiting) console.log(`clean-up: ${name} waits (${reason})`);
    let freed = 0;
    for (const { name, objects: files } of plan.remove) {
      for (const file of files) {
        if (dryRun) console.log(`[dry-run] would delete ${file.key}`);
        else await r2.delete(file.key);
        freed += file.size;
      }
      console.log(`clean-up: ${dryRun ? "would delete" : "deleted"} ${base}${name}/`);
    }
    console.log(
      `clean-up: kept ${[...keep].join(", ")}; ${dryRun ? "would free" : "freed"} ${Math.round(freed / 1e6)} MB`,
    );
  } catch (err) {
    console.log(`clean-up skipped: ${err.message}`);
    // In CI, an annotation on the run: a clean-up that keeps failing must not go unseen.
    if (process.env.GITHUB_ACTIONS === "true") {
      console.log(`::warning title=R2 clean-up skipped::${err.message.replace(/\r?\n/g, " ")}`);
    }
  }
}

async function show(flags, r2) {
  if (!r2) throw new Error("show reads R2; set the R2_* variables");
  const channel = checkChannel(flags.channel ?? "stable");
  const doc = await readChannel(r2, channel);
  if (!doc) throw new Error(`${channelKey(channel)} does not exist`);
  process.stdout.write(canonicalJson(doc));
}

run(async () => {
  const [command, ...rest] = process.argv.slice(2);
  const { flags, positional } = parseArgs(rest, {
    dir: "string",
    version: "string",
    channel: "string",
    from: "string",
    to: "string",
    "notes-file": "string",
    rollout: "string",
    mandatory: "boolean",
    "min-version": "string",
    require: "string",
    "expect-current": "string",
    "allow-downgrade": "boolean",
    "no-prune": "boolean",
    "dry-run": "boolean",
  });
  if (positional.length) throw new UsageError(`unexpected argument ${positional[0]}`);
  const commands = { publish, promote, prune, show };
  if (!Object.hasOwn(commands, command ?? "")) throw new UsageError("expected a command");
  const { client, missing } = R2Client.fromEnv();
  if (!client && flags["dry-run"]) console.log(`note: no R2 credentials (${missing.join(", ")})`);
  if (!client && !flags["dry-run"]) throw new Error(`missing environment: ${missing.join(", ")}`);
  await commands[command](flags, client);
}, USAGE);
