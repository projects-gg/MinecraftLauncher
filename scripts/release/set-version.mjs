#!/usr/bin/env node
// Stamps the release version into the working tree before a CI build (never committed).
// The tag is the single source of the version (todo.md §0): this writes it to
// `[workspace.package] version` in Cargo.toml (Rust crates, Tauri bundles, updater) and to the
// launcher's package.json, and its numeric macOS bundle version (CFBundleVersion; macBundleVersion
// in lib.mjs) to `bundle.macOS.bundleVersion` in the launcher's tauri.conf.json. A local macOS
// build of another version than the committed one runs this first, too.
//
//   node scripts/release/set-version.mjs 2.1.0 [--dry-run]

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  macBundleVersion,
  parseArgs,
  parseVersion,
  run,
  setMacBundleVersion,
  UsageError,
} from "./lib.mjs";

const USAGE = "usage: node scripts/release/set-version.mjs <version> [--dry-run]";
const root = fileURLToPath(new URL("../../", import.meta.url));

run(async () => {
  const { flags, positional } = parseArgs(process.argv.slice(2), { "dry-run": "boolean" });
  if (positional.length !== 1) throw new UsageError("expected exactly one version");
  const version = parseVersion(positional[0]).raw;
  const bundleVersion = macBundleVersion(version);

  const cargoPath = `${root}Cargo.toml`;
  const cargo = readFileSync(cargoPath, "utf8");
  const start = cargo.search(/^\[workspace\.package\][ \t]*$/m);
  const next = start < 0 ? -1 : cargo.slice(start + 1).search(/^\[/m);
  const end = next < 0 ? cargo.length : start + 1 + next;
  const versionLine = /^version\s*=\s*"[^"]*"/m;
  if (start < 0 || !versionLine.test(cargo.slice(start, end))) {
    throw new Error("no version in [workspace.package] of Cargo.toml");
  }
  const block = cargo.slice(start, end).replace(versionLine, `version = "${version}"`);
  const nextCargo = cargo.slice(0, start) + block + cargo.slice(end);

  // Everything is worked out before the first write, so a failure leaves the tree untouched.
  const writes = [[cargoPath, nextCargo, version]];
  for (const app of ["launcher"]) {
    const path = `${root}apps/${app}/package.json`;
    const pkg = JSON.parse(readFileSync(path, "utf8"));
    pkg.version = version;
    writes.push([path, `${JSON.stringify(pkg, null, 2)}\n`, version]);

    const configPath = `${root}apps/${app}/src-tauri/tauri.conf.json`;
    const config = readFileSync(configPath, "utf8");
    const name = configPath.slice(root.length);
    writes.push([
      configPath,
      setMacBundleVersion(config, bundleVersion, name),
      `${bundleVersion} (macOS bundle version)`,
    ]);
  }

  for (const [path, content, value] of writes) {
    if (!flags["dry-run"]) writeFileSync(path, content);
    console.log(`${flags["dry-run"] ? "would set" : "set"} ${value} in ${path.slice(root.length)}`);
  }
}, USAGE);
