// The macOS bundle version in the committed configs, set-version.mjs and check-mac-bundle.mjs.
// Run: `node --test "scripts/release/*.test.mjs"`.

import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { macBundleVersion } from "./lib.mjs";

const run = promisify(execFile);
const here = fileURLToPath(new URL(".", import.meta.url));
const root = fileURLToPath(new URL("../../", import.meta.url));
const configs = ["launcher", "studio"].map((app) => `apps/${app}/src-tauri/tauri.conf.json`);

function workspaceVersion() {
  const cargo = readFileSync(join(root, "Cargo.toml"), "utf8");
  return /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]*)"/ms.exec(cargo)[1];
}

test("the committed Tauri configs carry the workspace version's bundle version", () => {
  const version = workspaceVersion();
  for (const path of configs) {
    const config = JSON.parse(readFileSync(join(root, path), "utf8"));
    // The bundle's version comes from Cargo.toml only while the config has none of its own.
    assert.equal(config.version, undefined, `${path} has its own version`);
    assert.equal(
      config.bundle.macOS.bundleVersion,
      macBundleVersion(version),
      `${path}: run \`node scripts/release/set-version.mjs ${version}\``,
    );
  }
});

test("set-version.mjs stamps the macOS bundle version of both apps", async () => {
  const before = configs.map((path) => readFileSync(join(root, path), "utf8"));
  const { stdout } = await run(process.execPath, [
    join(here, "set-version.mjs"),
    "v2.1.0-beta.2",
    "--dry-run",
  ]);
  for (const path of configs) {
    assert.ok(stdout.includes(`would set 2.1.202 (macOS bundle version) in ${path}`), stdout);
  }
  assert.ok(stdout.includes("would set 2.1.0-beta.2 in Cargo.toml"), stdout);

  await assert.rejects(
    run(process.execPath, [join(here, "set-version.mjs"), "2.1.0-dev.1", "--dry-run"]),
    (err) => err.code === 1 && /has no macOS bundle version/.test(err.stderr),
  );
  assert.deepEqual(
    configs.map((path) => readFileSync(join(root, path), "utf8")),
    before,
  );
});

test("check-mac-bundle.mjs passes the stamped bundle and stops others", async () => {
  const dir = mkdtempSync(join(tmpdir(), "mac-bundle-"));
  try {
    const app = (name, short, bundle) => {
      const path = join(dir, `${name}.app`);
      mkdirSync(join(path, "Contents"), { recursive: true });
      writeFileSync(
        join(path, "Contents", "Info.plist"),
        `<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
\t<key>CFBundleShortVersionString</key>
\t<string>${short}</string>
\t<key>CFBundleVersion</key>
\t<string>${bundle}</string>
</dict>
</plist>
`,
      );
      return path;
    };
    const good = app("Good", "2.0.1-beta.2", "2.0.1202");
    const unstamped = app("Unstamped", "2.0.1-beta.2", "2.0.1-beta.2");
    const tarball = join(dir, "Good.app.tar.gz");
    writeFileSync(tarball, "");
    const check = (paths) =>
      run(process.execPath, [join(here, "check-mac-bundle.mjs"), "--version", "2.0.1-beta.2"], {
        env: { ...process.env, ARTIFACT_PATHS: JSON.stringify(paths) },
      });

    const { stdout } = await check([good, tarball]);
    assert.match(stdout, /Good\.app: 2\.0\.1-beta\.2 \(CFBundleVersion 2\.0\.1202\)/);
    await assert.rejects(check([good, unstamped]), (err) =>
      /CFBundleVersion is 2\.0\.1-beta\.2, not 2\.0\.1202/.test(err.stderr),
    );
    await assert.rejects(check([tarball]), (err) => /no \.app bundle/.test(err.stderr));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
