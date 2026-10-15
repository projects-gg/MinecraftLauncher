#!/usr/bin/env node
// Publishes launcher releases to Cloudflare R2 (docs/content-format.md, "Launcher release
// channels"). See docs/release.md for the whole flow.
//
//   publish  Upload every file of --dir to v1/releases/<version>/ (immutable: identical files are
//            skipped, different bytes under an existing name are refused), verify each object,
//            then write v1/releases/<channel>.json last (the commit point).
//   promote  Copy the release announced in <from>.json (it must be --version) to <to>.json with
//            the rollout / mandatory / minVersion you choose. beta -> stable, or stable -> stable
//            to widen a staged rollout.
//   show     Print a channel file.
//
//   node scripts/release/publish-r2.mjs publish --dir release-files --version 2.1.0 \
//     [--channel beta|stable|none] [--notes-file notes.md] [--rollout 100] [--mandatory] \
//     [--min-version 2.0.3|none] [--require windows-x86_64,...] [--allow-downgrade] [--dry-run]
//   node scripts/release/publish-r2.mjs promote --version 2.1.0 [--from beta] [--to stable] \
//     [--rollout 25] [--mandatory[=false]] [--min-version 2.0.3|none] [--allow-downgrade] [--dry-run]
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
  R2Client,
  readBundles,
  readNotes,
  requirePlatforms,
  rfc3339,
  run,
  sha256Hex,
  UsageError,
} from "./lib.mjs";

const USAGE = `usage: node scripts/release/publish-r2.mjs <publish|promote|show> [options]
  publish --dir <dir> --version <v> [--channel beta|stable|none] [--notes-file <md>]
          [--rollout 0-100] [--mandatory] [--min-version <v>|none] [--require <keys>]
          [--allow-downgrade] [--dry-run]
  promote --version <v> [--from beta] [--to stable] [--rollout 0-100] [--mandatory[=false]]
          [--min-version <v>|none] [--allow-downgrade] [--dry-run]
  show    [--channel stable]`;

const CDN = (process.env.CDN_BASE_URL || "https://cdn.projects.gg").replace(/\/+$/, "");
const PREFIX = (process.env.R2_PREFIX || "v1").replace(/^\/+|\/+$/g, "");
const CHANNELS = ["stable", "beta"];
const IMMUTABLE = "public, max-age=31536000, immutable";
const MUTABLE = "public, max-age=60";
const JSON_TYPE = "application/json; charset=utf-8";

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

  // 2. The channel file, last.
  if (manifest) await writeChannel(r2, channel, manifest, dryRun);
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
  const target = from === to ? source : await readChannel(r2, to);
  if (from !== to) guardDowngrade(to, target, version, flags["allow-downgrade"]);

  const same = from === to;
  const rollout = rolloutFlag(flags.rollout) ?? (same ? (source.rollout ?? 100) : 100);
  const mandatory = flags.mandatory ?? (same ? Boolean(source.mandatory) : false);
  const minVersion = flags["min-version"] ?? target?.minVersion;
  const doc = {
    version: source.version,
    notes: source.notes ?? "",
    pub_date: same ? source.pub_date : rfc3339(),
    platforms: source.platforms,
    channel: to,
    rollout,
    mandatory,
    minVersion: minVersion && minVersion !== "none" ? parseVersion(minVersion).raw : undefined,
  };
  await writeChannel(r2, to, doc, Boolean(flags["dry-run"]));
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
    "allow-downgrade": "boolean",
    "dry-run": "boolean",
  });
  if (positional.length) throw new UsageError(`unexpected argument ${positional[0]}`);
  const commands = { publish, promote, show };
  if (!Object.hasOwn(commands, command ?? "")) throw new UsageError("expected a command");
  const { client, missing } = R2Client.fromEnv();
  if (!client && flags["dry-run"]) console.log(`note: no R2 credentials (${missing.join(", ")})`);
  if (!client && !flags["dry-run"]) throw new Error(`missing environment: ${missing.join(", ")}`);
  await commands[command](flags, client);
}, USAGE);
