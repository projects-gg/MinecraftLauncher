#!/usr/bin/env node
// Prints `version=`, `tag=`, `prerelease=` and `mac_bundle_version=` lines for $GITHUB_OUTPUT
// from a tag or version. Fails, printing nothing, for a version the macOS bundles cannot carry
// (macBundleVersion in lib.mjs), so such a tag stops before any build starts.
//
//   node scripts/release/version-info.mjs v2.1.0-beta.1 >> "$GITHUB_OUTPUT"

import { macBundleVersion, parseArgs, parseVersion, run, UsageError } from "./lib.mjs";

run(async () => {
  const { positional } = parseArgs(process.argv.slice(2), {});
  if (positional.length !== 1) throw new UsageError("expected one version or tag");
  const version = parseVersion(positional[0]);
  const bundleVersion = macBundleVersion(version.raw);
  console.log(`version=${version.raw}`);
  console.log(`tag=v${version.raw}`);
  console.log(`prerelease=${version.pre.length > 0}`);
  console.log(`mac_bundle_version=${bundleVersion}`);
}, "usage: node scripts/release/version-info.mjs <version|tag>");
