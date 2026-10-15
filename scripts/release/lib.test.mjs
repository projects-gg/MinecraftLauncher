// Self-tests for the release helpers: `node --test scripts/release/` (runs in CI).

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildPlatforms,
  canonicalJson,
  classifyBundle,
  compareVersions,
  ghAssetName,
  parseVersion,
  sha256Hex,
  signRequest,
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
