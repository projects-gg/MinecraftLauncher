#!/usr/bin/env node
// Checks the versions in the macOS app bundles a build made: CFBundleShortVersionString must be
// the release version and CFBundleVersion its numeric macOS bundle version (macBundleVersion in
// lib.mjs). Fails a build that skipped set-version.mjs or a Tauri that stopped honouring
// `bundle.macOS.bundleVersion`.
//
//   node scripts/release/check-mac-bundle.mjs --version <v> [App.app...]
//
// Without paths it checks the .app folders in the JSON array in $ARTIFACT_PATHS (tauri-action's
// `artifactPaths`).

import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import { macBundleVersion, parseArgs, parseVersion, plistString, run, UsageError } from "./lib.mjs";

const USAGE = "usage: node scripts/release/check-mac-bundle.mjs --version <v> [App.app...]";

run(async () => {
  const { flags, positional } = parseArgs(process.argv.slice(2), { version: "string" });
  if (!flags.version) throw new UsageError("missing --version");
  const expected = {
    CFBundleShortVersionString: parseVersion(flags.version).raw,
    CFBundleVersion: macBundleVersion(flags.version),
  };
  let apps = positional;
  if (!apps.length) {
    if (!process.env.ARTIFACT_PATHS) throw new UsageError("no paths and no $ARTIFACT_PATHS");
    apps = JSON.parse(process.env.ARTIFACT_PATHS).filter(
      (path) => path.endsWith(".app") && statSync(path).isDirectory(),
    );
  }
  if (!apps.length) throw new Error("no .app bundle to check");

  for (const app of apps) {
    const plist = readFileSync(join(app, "Contents", "Info.plist"), "utf8");
    for (const [key, value] of Object.entries(expected)) {
      const found = plistString(plist, key);
      if (found !== value) throw new Error(`${app}: ${key} is ${found ?? "missing"}, not ${value}`);
    }
    console.log(
      `${app}: ${expected.CFBundleShortVersionString} (CFBundleVersion ${expected.CFBundleVersion})`,
    );
  }
}, USAGE);
