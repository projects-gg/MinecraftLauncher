#!/usr/bin/env node
// Copies Tauri bundles into one flat folder under the names GitHub Releases gives them, so the
// GitHub release, latest.json and the R2 copy (`v1/releases/<version>/`) all use the same names.
// Mirrors tauri-action v1 naming: CLI file names with spaces turned into dots, and the macOS
// updater tarball gets `_<version>_<arch>` (Tauri names it just `<Product>.app.tar.gz`).
// Folders (the macOS .app) are skipped; the .app.tar.gz carries the app.
//
//   node scripts/release/collect-artifacts.mjs --out <dir> --version <v> --product "<Name>" [paths...]
//
// Without paths it reads the JSON array in $ARTIFACT_PATHS (tauri-action's `artifactPaths`).

import { copyFileSync, mkdirSync, statSync } from "node:fs";
import { basename, join } from "node:path";

import { ghAssetName, parseArgs, parseVersion, run, UsageError } from "./lib.mjs";

const USAGE =
  'usage: node scripts/release/collect-artifacts.mjs --out <dir> --version <v> --product "<Product Name>" [paths...]';

function macArch(path) {
  const p = path.replaceAll("\\", "/");
  if (p.includes("/universal-apple-darwin/")) return "universal";
  if (p.includes("/aarch64-apple-darwin/")) return "aarch64";
  if (p.includes("/x86_64-apple-darwin/")) return "x64";
  return process.arch === "arm64" ? "aarch64" : "x64";
}

run(async () => {
  const { flags, positional } = parseArgs(process.argv.slice(2), {
    out: "string",
    version: "string",
    product: "string",
  });
  if (!flags.out || !flags.version || !flags.product) throw new UsageError("missing options");
  const version = parseVersion(flags.version).raw;
  let paths = positional;
  if (!paths.length) {
    if (!process.env.ARTIFACT_PATHS) throw new UsageError("no paths and no $ARTIFACT_PATHS");
    paths = JSON.parse(process.env.ARTIFACT_PATHS);
  }

  mkdirSync(flags.out, { recursive: true });
  const seen = new Map();
  for (const path of paths) {
    if (statSync(path).isDirectory()) {
      console.log(`skip folder ${path}`);
      continue;
    }
    let name = basename(path);
    const tarball = name.match(/\.app\.tar\.gz(\.sig)?$/);
    if (tarball) name = `${flags.product}_${version}_${macArch(path)}${tarball[0]}`;
    name = ghAssetName(name);
    if (seen.has(name)) throw new Error(`${path} and ${seen.get(name)} both become ${name}`);
    seen.set(name, path);
    copyFileSync(path, join(flags.out, name));
    console.log(`${name}  <-  ${path}`);
  }
  if (!seen.size) throw new Error("no files collected");
}, USAGE);
