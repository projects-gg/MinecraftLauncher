// Self-tests for the release helpers: `node --test scripts/release/` (runs in CI).

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildPlatforms,
  canonicalJson,
  classifyBundle,
  compareVersions,
  ghAssetName,
  macBundleVersion,
  parseVersion,
  plistString,
  setMacBundleVersion,
  sha256Hex,
  signRequest,
  versionStamps,
} from "./lib.mjs";

const AWS_EXAMPLE_KEYS = {
  accessKeyId: "AKIAIOSFODNN7EXAMPLE",
  secretAccessKey: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
  region: "us-east-1",
  date: new Date("2013-05-24T00:00:00Z"),
};

// Test vectors from the Amazon S3 documentation, "Signature Calculations for the Authorization
// Header: Transferring Payload in a Single Chunk (AWS Signature Version 4)".
test("SigV4 matches the AWS GET Object example", () => {
  const headers = signRequest({
    ...AWS_EXAMPLE_KEYS,
    method: "GET",
    path: "/test.txt",
    headers: { host: "examplebucket.s3.amazonaws.com", range: "bytes=0-9" },
    payloadHash: sha256Hex(""),
  });
  assert.match(
    headers.authorization,
    /SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41$/,
  );
});

test("SigV4 matches the AWS PUT Object example (encoded key)", () => {
  const body = "Welcome to Amazon S3.";
  const headers = signRequest({
    ...AWS_EXAMPLE_KEYS,
    method: "PUT",
    path: "/test$file.text",
    headers: {
      host: "examplebucket.s3.amazonaws.com",
      date: "Fri, 24 May 2013 00:00:00 GMT",
      "x-amz-storage-class": "REDUCED_REDUNDANCY",
    },
    payloadHash: sha256Hex(body),
  });
  assert.match(
    headers.authorization,
    /Signature=98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd$/,
  );
});

test("versions follow semver precedence", () => {
  assert.throws(() => parseVersion("2.1"));
  assert.throws(() => parseVersion("2.1.0+build.5"));
  assert.equal(parseVersion("v2.1.0").raw, "2.1.0");
  const ordered = ["2.0.0-alpha", "2.0.0-alpha.1", "2.0.0-beta", "2.0.0-beta.2", "2.0.0-beta.11"];
  ordered.push("2.0.0-rc.1", "2.0.0", "2.0.1", "2.1.0", "10.0.0");
  for (let i = 1; i < ordered.length; i++) {
    assert.ok(compareVersions(ordered[i - 1], ordered[i]) < 0, `${ordered[i - 1]} < ${ordered[i]}`);
    assert.ok(compareVersions(ordered[i], ordered[i - 1]) > 0);
  }
  assert.equal(compareVersions("2.0.0", "v2.0.0"), 0);
});

test("asset names match GitHub's sanitising", () => {
  assert.equal(
    ghAssetName("Projects Launcher_2.1.0_x64-setup.exe"),
    "Projects.Launcher_2.1.0_x64-setup.exe",
  );
  assert.equal(
    ghAssetName("Projects Launcher-2.1.0-1.x86_64.rpm"),
    "Projects.Launcher-2.1.0-1.x86_64.rpm",
  );
});

test("bundles map to updater platform keys", () => {
  assert.deepEqual(classifyBundle("Projects.Launcher_2.1.0_x64-setup.exe"), {
    os: "windows",
    arch: "x86_64",
    installer: "nsis",
  });
  assert.equal(classifyBundle("Projects.Launcher_2.1.0_x64-setup.exe.sig"), null);
  assert.equal(classifyBundle("Projects.Launcher_2.1.0_universal.dmg"), null);

  const base = "https://cdn.projects.gg/v1/releases/2.1.0/";
  const platforms = buildPlatforms(
    [
      { name: "Projects.Launcher_2.1.0_x64-setup.exe", signature: "W" },
      { name: "Projects.Launcher_2.1.0_universal.app.tar.gz", signature: "M" },
      { name: "Projects.Launcher_2.1.0_amd64.AppImage", signature: "A" },
      { name: "Projects.Launcher_2.1.0_amd64.deb", signature: "D" },
      { name: "Projects.Launcher-2.1.0-1.x86_64.rpm", signature: "R" },
    ],
    base,
  );
  assert.deepEqual(Object.keys(platforms).sort(), [
    "darwin-aarch64",
    "darwin-aarch64-app",
    "darwin-x86_64",
    "darwin-x86_64-app",
    "linux-x86_64",
    "linux-x86_64-appimage",
    "linux-x86_64-deb",
    "linux-x86_64-rpm",
    "windows-x86_64",
    "windows-x86_64-nsis",
  ]);
  assert.equal(platforms["windows-x86_64"].url, `${base}Projects.Launcher_2.1.0_x64-setup.exe`);
  assert.equal(platforms["darwin-x86_64"].signature, "M");

  const native = buildPlatforms(
    [
      { name: "P_2.1.0_universal.app.tar.gz", signature: "U" },
      { name: "P_2.1.0_aarch64.app.tar.gz", signature: "N" },
    ],
    base,
  );
  assert.equal(native["darwin-aarch64"].signature, "N");
  assert.equal(native["darwin-x86_64"].signature, "U");
  assert.throws(() =>
    buildPlatforms(
      [
        { name: "A_1.0.0_x64-setup.exe", signature: "1" },
        { name: "B_1.0.0_x64-setup.exe", signature: "2" },
      ],
      base,
    ),
  );
});

test("canonical JSON sorts keys and ends with a newline", () => {
  assert.equal(
    canonicalJson({ b: 1, a: { d: 2, c: [3] }, x: undefined }),
    '{\n  "a": {\n    "c": [\n      3\n    ],\n    "d": 2\n  },\n  "b": 1\n}\n',
  );
});

// How macOS orders CFBundleVersion: number by number, a missing number counts as 0.
function compareBundleVersions(a, b) {
  const x = a.split(".").map(Number);
  const y = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0);
  }
  return 0;
}

test("macOS bundle versions are three numbers", () => {
  const cases = {
    "2.0.0": "2.0.999",
    "2.0.1-alpha": "2.0.1100",
    "2.0.1-beta": "2.0.1200",
    "2.0.1-beta.1": "2.0.1201",
    "v2.0.1-beta.2": "2.0.1202",
    "2.0.1-beta.11": "2.0.1211",
    "2.0.1-rc.1": "2.0.1301",
    "2.0.1-rc.99": "2.0.1399",
    "2.0.1": "2.0.1999",
    "2.0.12": "2.0.12999",
    "2.1.0-beta.1": "2.1.201",
    "2.1.0": "2.1.999",
    "10.0.0": "10.0.999",
    "0.0.0-alpha": "0.0.100",
  };
  for (const [version, expected] of Object.entries(cases)) {
    assert.equal(macBundleVersion(version), expected, version);
  }
  assert.equal(macBundleVersion("2.0.2147482"), "2.0.2147482999");
});

test("macOS bundle versions sort like the versions", () => {
  const pre = ["", "-alpha", "-alpha.1", "-alpha.99", "-beta", "-beta.1", "-beta.2", "-beta.10"];
  pre.push("-beta.99", "-rc", "-rc.1", "-rc.99");
  const versions = [];
  for (const major of [0, 1, 2, 10]) {
    for (const minor of [0, 1, 9, 10]) {
      for (const patch of [0, 1, 2, 9, 10, 999]) {
        for (const p of pre) versions.push(`${major}.${minor}.${patch}${p}`);
      }
    }
  }
  versions.sort(compareVersions);
  for (let i = 1; i < versions.length; i++) {
    const [a, b] = [versions[i - 1], versions[i]];
    assert.ok(compareVersions(a, b) < 0, `${a} < ${b}`);
    const [x, y] = [macBundleVersion(a), macBundleVersion(b)];
    assert.match(y, /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/);
    assert.ok(compareBundleVersions(x, y) < 0, `${a} (${x}) < ${b} (${y})`);
  }
  // A beta updates to its release. The shipped 2.0.1-beta.1 carried "2.0.1-beta.1", whose
  // numbers macOS can read as 2.0.1 at most: below every later build.
  assert.ok(compareBundleVersions(macBundleVersion("2.0.1-beta.9"), macBundleVersion("2.0.1")) < 0);
  assert.ok(compareBundleVersions("2.0.1", macBundleVersion("2.0.1-beta.2")) < 0);
});

test("versions macOS could not order have no bundle version", () => {
  const refused = ["2.1.0-beta.0", "2.1.0-beta.100", "2.1.0-beta.1.1", "2.1.0-beta.x"];
  refused.push("2.1.0-Beta.1", "2.1.0-dev.1", "2.1.0-1", "2.1.0-rc-1", "2.0.2147483", "2.1");
  for (const version of refused) {
    assert.throws(() => macBundleVersion(version), /bundle version|not a valid version/, version);
  }
});

test("a local Windows or Linux build may carry a build number; CI may not", () => {
  // The tenth local build of beta.1 sorts like semver, so the updater takes it…
  assert.ok(compareVersions("2.0.1-beta.1.10", "2.0.1-beta.1.9") > 0);
  assert.ok(compareVersions("2.0.1-beta.1.10", "2.0.1-beta.1") > 0);
  assert.ok(compareVersions("2.0.1-beta.1.10", "2.0.1-beta.2") < 0);
  assert.ok(compareVersions("2.0.1-beta.1.10", "2.0.1") < 0);
  // …but macOS cannot number it: refused, or stamped without a macOS bundle version locally.
  assert.throws(() => versionStamps("2.0.1-beta.1.10"), /no macOS bundle version.*--local/);
  assert.deepEqual(versionStamps("v2.0.1-beta.1.10", { local: true }), {
    version: "2.0.1-beta.1.10",
    macBundleVersion: null,
  });
  // A version macOS can number is stamped in full either way.
  for (const local of [false, true]) {
    assert.deepEqual(versionStamps("v2.0.1-beta.2", { local }), {
      version: "2.0.1-beta.2",
      macBundleVersion: "2.0.1202",
    });
  }
  // Not a version at all: refused locally too (no build metadata in mirror paths).
  assert.throws(() => versionStamps("2.0.1-beta.1+10", { local: true }), /not a valid version/);
});

test("the macOS bundle version is stamped into a Tauri config in place", () => {
  const config = `{
  "bundle": {
    "targets": ["app", "dmg"],
    "macOS": {
      "bundleVersion": "2.0.999",
      "signingIdentity": "-"
    }
  }
}
`;
  assert.equal(setMacBundleVersion(config, "2.1.201"), config.replace('"2.0.999"', '"2.1.201"'));
  assert.throws(
    () => setMacBundleVersion('{ "bundle": { "macOS": {} } }', "2.1.201", "x.json"),
    /no bundle\.macOS\.bundleVersion in x\.json/,
  );
  const twice = config.replace('"targets"', '"iOS": { "bundleVersion": "1" },\n    "targets"');
  assert.throws(() => setMacBundleVersion(twice, "2.1.201"), /found 2/);
  assert.throws(() => setMacBundleVersion(config, "2.1.0-beta.1"), /not a macOS bundle version/);
});

test("Info.plist strings are read by key", () => {
  const plist = `<dict>
\t<key>CFBundleShortVersionString</key>
\t<string>2.0.1-beta.2</string>
\t<key>CFBundleVersion</key>
\t<string>2.0.1202</string>
</dict>`;
  assert.equal(plistString(plist, "CFBundleShortVersionString"), "2.0.1-beta.2");
  assert.equal(plistString(plist, "CFBundleVersion"), "2.0.1202");
  assert.equal(plistString(plist, "CFBundleName"), undefined);
});
