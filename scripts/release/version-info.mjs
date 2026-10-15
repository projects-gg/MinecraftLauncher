#!/usr/bin/env node
// Prints `version=`, `tag=` and `prerelease=` lines for $GITHUB_OUTPUT from a tag or version.
//
//   node scripts/release/version-info.mjs v2.1.0-beta.1 >> "$GITHUB_OUTPUT"

import { parseArgs, parseVersion, run, UsageError } from "./lib.mjs";

run(async () => {
  const { positional } = parseArgs(process.argv.slice(2), {});
  if (positional.length !== 1) throw new UsageError("expected one version or tag");
  const version = parseVersion(positional[0]);
  console.log(`version=${version.raw}`);
  console.log(`tag=v${version.raw}`);
  console.log(`prerelease=${version.pre.length > 0}`);
}, "usage: node scripts/release/version-info.mjs <version|tag>");
