#!/usr/bin/env node
// Writes an updater manifest from a folder of signed bundles (the output of
// collect-artifacts.mjs): Tauri's `latest.json`, or with --channel the channel file of
// docs/content-format.md (`releases/<channel>.json`).
//
//   node scripts/release/channel.mjs --dir release-files --version 2.1.0 \
//     --base-url https://github.com/projects-gg/MinecraftLauncher/releases/download/v2.1.0/ \
//     [--notes-file notes.md] [--channel beta --rollout 100 --mandatory --min-version 2.0.3] \
//     [--require windows-x86_64,darwin-aarch64] [--out latest.json]
//
// --notes-file: the release notes as the tag message gives them, Turkish, then optionally a
// `--- en ---` line and the English notes; the manifest carries them as `notes` and
// `localizedNotes` (lib.mjs `notesFields`, docs/release.md).

import { writeFileSync } from "node:fs";

import {
  buildManifest,
  canonicalJson,
  parseArgs,
  readBundles,
  readNotes,
  requirePlatforms,
  run,
  UsageError,
} from "./lib.mjs";

const USAGE = `usage: node scripts/release/channel.mjs --dir <bundles> --version <v> --base-url <url>
  [--notes-file <md>] [--channel <name> [--rollout 0-100] [--mandatory] [--min-version <v>]]
  [--require <key,key>] [--out <file>]`;

run(async () => {
  const { flags } = parseArgs(process.argv.slice(2), {
    dir: "string",
    version: "string",
    "base-url": "string",
    "notes-file": "string",
    channel: "string",
    rollout: "string",
    mandatory: "boolean",
    "min-version": "string",
    require: "string",
    out: "string",
  });
  if (!flags.dir || !flags.version || !flags["base-url"]) throw new UsageError("missing options");
  const manifest = buildManifest({
    version: flags.version,
    notes: readNotes(flags["notes-file"]),
    bundles: readBundles(flags.dir),
    baseUrl: flags["base-url"],
    channel: flags.channel,
    rollout: flags.rollout === undefined ? undefined : Number(flags.rollout),
    mandatory: flags.mandatory,
    minVersion: flags["min-version"],
  });
  if (flags.require) requirePlatforms(manifest, flags.require.split(",").filter(Boolean));
  const json = canonicalJson(manifest);
  if (flags.out) {
    writeFileSync(flags.out, json);
    console.log(`wrote ${flags.out} (${Object.keys(manifest.platforms).length} platform entries)`);
  } else {
    process.stdout.write(json);
  }
}, USAGE);
