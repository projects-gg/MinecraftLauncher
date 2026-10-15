#!/usr/bin/env node
// Writes the bundle type into a Tauri executable, the way `tauri bundle` does right before it
// packages the app. Only needed when the executable is code-signed *before* bundling (the
// SignPath flow in .github/workflows/release.yml): Tauri cannot patch an already signed file,
// so we patch first, sign, and let `tauri bundle` skip its own patch (it only warns).
// The updater plugin reads this marker to pick the `-nsis` / `-msi` entry of the update manifest.
//
//   node scripts/release/patch-bundle-type.mjs <app.exe> <nsis|msi>

import { readFileSync, writeFileSync } from "node:fs";

import { parseArgs, run, UsageError } from "./lib.mjs";

const USAGE = "usage: node scripts/release/patch-bundle-type.mjs <app.exe> <nsis|msi>";
const UNKNOWN = "__TAURI_BUNDLE_TYPE_VAR_UNK";
const TYPES = { nsis: "__TAURI_BUNDLE_TYPE_VAR_NSS", msi: "__TAURI_BUNDLE_TYPE_VAR_MSI" };

run(async () => {
  const { positional } = parseArgs(process.argv.slice(2), {});
  const [file, type] = positional;
  if (!file || !TYPES[type]) throw new UsageError("expected <app.exe> <nsis|msi>");
  // The typed markers (…_NSS, …_MSI) also exist in every binary as match arms in tauri-utils,
  // so only the `…_UNK` initial value tells an unpatched file apart.
  const data = readFileSync(file);
  const at = data.indexOf(UNKNOWN);
  if (at < 0) throw new Error(`${file} has no unpatched marker (already patched, or not Tauri 2)`);
  if (data.indexOf(UNKNOWN, at + 1) >= 0) throw new Error(`${file} has more than one marker`);
  data.write(TYPES[type], at, "latin1");
  writeFileSync(file, data);
  console.log(`marked ${file} as ${type}`);
}, USAGE);
