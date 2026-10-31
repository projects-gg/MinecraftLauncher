// End-to-end test of publish-r2.mjs and channel.mjs against an in-memory S3 server that checks
// every request's SigV4 signature. Run: `node --test "scripts/release/*.test.mjs"`.

import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { signRequest } from "./lib.mjs";

const run = promisify(execFile);
const here = fileURLToPath(new URL(".", import.meta.url));
const KEYS = { accessKeyId: "test-key", secretAccessKey: "test-secret" };
const objects = new Map();
let server;
let env;
let dir;
let notes;
let bilingual;

function startServer() {
  return new Promise((resolve) => {
    server = createServer((req, res) => {
      const chunks = [];
      req.on("data", (c) => chunks.push(c));
      req.on("end", () => {
        const body = Buffer.concat(chunks);
        const auth = req.headers.authorization ?? "";
        const signedNames = /SignedHeaders=([^,]+)/.exec(auth)?.[1].split(";") ?? [];
        const headers = Object.fromEntries(signedNames.map((n) => [n, req.headers[n]]));
        delete headers["x-amz-content-sha256"];
        delete headers["x-amz-date"];
        const amz = req.headers["x-amz-date"];
        const date = new Date(
          `${amz.slice(0, 4)}-${amz.slice(4, 6)}-${amz.slice(6, 8)}T${amz.slice(9, 11)}:${amz.slice(11, 13)}:${amz.slice(13, 15)}Z`,
        );
        const expected = signRequest({
          method: req.method,
          path: decodeURIComponent(req.url),
          headers,
          payloadHash: req.headers["x-amz-content-sha256"],
          ...KEYS,
          region: "auto",
          date,
        }).authorization;
        const payloadOk =
          createHash("sha256").update(body).digest("hex") === req.headers["x-amz-content-sha256"];
        if (auth !== expected || !payloadOk) {
          res.writeHead(403).end("SignatureDoesNotMatch");
          return;
        }
        const key = decodeURIComponent(req.url).replace(/^\/bucket\//, "");
        if (req.method === "PUT") {
          objects.set(key, {
            body,
            md5: createHash("md5").update(body).digest("hex"),
            sha256: req.headers["x-amz-meta-sha256"],
            cacheControl: req.headers["cache-control"],
          });
          res.writeHead(200).end();
          return;
        }
        const obj = objects.get(key);
        if (!obj) {
          res.writeHead(404).end();
          return;
        }
        res.writeHead(200, {
          "content-length": obj.body.length,
          etag: `"${obj.md5}"`,
          "x-amz-meta-sha256": obj.sha256,
        });
        res.end(req.method === "GET" ? obj.body : undefined);
      });
    });
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
}

async function publishR2(...args) {
  return run(process.execPath, [join(here, "publish-r2.mjs"), ...args], { env });
}

before(async () => {
  const port = await startServer();
  env = {
    ...process.env,
    R2_ACCOUNT_ID: "test",
    R2_ACCESS_KEY_ID: KEYS.accessKeyId,
    R2_SECRET_ACCESS_KEY: KEYS.secretAccessKey,
    R2_BUCKET: "bucket",
    R2_ENDPOINT: `http://127.0.0.1:${port}`,
    CDN_BASE_URL: "https://cdn.example.test",
  };
  dir = mkdtempSync(join(tmpdir(), "pl-release-"));
  const files = {
    "Projects.Launcher_2.1.0_x64-setup.exe": "windows installer",
    "Projects.Launcher_2.1.0_x64-setup.exe.sig": "c2lnLXdpbg==",
    "Projects.Launcher_2.1.0_universal.app.tar.gz": "mac tarball",
    "Projects.Launcher_2.1.0_universal.app.tar.gz.sig": "c2lnLW1hYw==",
    "Projects.Launcher_2.1.0_universal.dmg": "mac dmg",
    "Projects.Launcher_2.1.0_amd64.AppImage": "appimage",
    "Projects.Launcher_2.1.0_amd64.AppImage.sig": "c2lnLWFwcGltYWdl",
  };
  for (const [name, content] of Object.entries(files)) writeFileSync(join(dir, name), content);
  notes = join(mkdtempSync(join(tmpdir(), "pl-notes-")), "notes.md");
  writeFileSync(notes, "Yeni ana ekran.\n");
  // A tag message with English notes under a "--- en ---" line (CRLF, as an editor may save it).
  bilingual = join(dirname(notes), "notes-both.md");
  writeFileSync(bilingual, "Yeni ana ekran.\r\n\r\n--- en ---\r\n\r\nA new home screen.\r\n");
});

after(() => {
  server.close();
  rmSync(dir, { recursive: true, force: true });
  rmSync(dirname(notes), { recursive: true, force: true });
});

test("publish uploads immutable files, then the channel file", async () => {
  await publishR2(
    "publish",
    "--dir",
    dir,
    "--version",
    "2.1.0",
    "--channel",
    "beta",
    "--notes-file",
    notes,
  );
  const setup = objects.get("v1/releases/2.1.0/Projects.Launcher_2.1.0_x64-setup.exe");
  assert.equal(setup.body.toString(), "windows installer");
  assert.equal(setup.cacheControl, "public, max-age=31536000, immutable");
  const beta = JSON.parse(objects.get("v1/releases/beta.json").body);
  assert.equal(objects.get("v1/releases/beta.json").cacheControl, "public, max-age=60");
  assert.equal(beta.version, "2.1.0");
  assert.equal(beta.channel, "beta");
  assert.equal(beta.rollout, 100);
  assert.equal(beta.notes, "Yeni ana ekran.");
  // Turkish-only notes write the channel file as before per-language notes.
  assert.equal(beta.localizedNotes, undefined);
  assert.equal(beta.platforms["windows-x86_64"].signature, "c2lnLXdpbg==");
  assert.equal(
    beta.platforms["darwin-aarch64"].url,
    "https://cdn.example.test/v1/releases/2.1.0/Projects.Launcher_2.1.0_universal.app.tar.gz",
  );
});

test("publish carries English notes next to the Turkish ones", async () => {
  const { stdout } = await publishR2(
    "publish",
    "--dir",
    dir,
    "--version",
    "2.1.0",
    "--channel",
    "beta",
    "--notes-file",
    bilingual,
  );
  assert.match(stdout, /beta already announces 2\.1\.0; rewriting it/);
  const beta = JSON.parse(objects.get("v1/releases/beta.json").body);
  // Older launchers (and Tauri) read `notes`: still the Turkish notes.
  assert.equal(beta.notes, "Yeni ana ekran.");
  assert.deepEqual(beta.localizedNotes, { en: "A new home screen.", tr: "Yeni ana ekran." });

  // A mistyped marker stops the publish before anything is written.
  const before = objects.get("v1/releases/beta.json").body;
  const mistyped = join(dirname(notes), "notes-mistyped.md");
  writeFileSync(mistyped, "Yeni ana ekran.\n\n--- English ---\n\nA new home screen.\n");
  await assert.rejects(
    publishR2("publish", "--dir", dir, "--version", "2.1.0", "--notes-file", mistyped),
    /"--- English ---" is not a language line/,
  );
  assert.equal(objects.get("v1/releases/beta.json").body, before);
});

test("re-publishing identical files is a no-op, changed bytes are refused", async () => {
  const { stdout } = await publishR2(
    "publish",
    "--dir",
    dir,
    "--version",
    "2.1.0",
    "--channel",
    "none",
  );
  assert.match(stdout, /same {5}v1\/releases\/2\.1\.0\/Projects\.Launcher_2\.1\.0_amd64\.AppImage/);
  writeFileSync(join(dir, "Projects.Launcher_2.1.0_universal.dmg"), "tampered");
  await assert.rejects(
    publishR2("publish", "--dir", dir, "--version", "2.1.0", "--channel", "none"),
    /immutable/,
  );
  writeFileSync(join(dir, "Projects.Launcher_2.1.0_universal.dmg"), "mac dmg");
});

test("promote copies beta to stable with a staged rollout and keeps channels monotonic", async () => {
  await publishR2("promote", "--version", "2.1.0", "--rollout", "25", "--min-version", "2.0.3");
  const stable = JSON.parse(objects.get("v1/releases/stable.json").body);
  assert.equal(stable.channel, "stable");
  assert.equal(stable.rollout, 25);
  assert.equal(stable.minVersion, "2.0.3");
  assert.equal(stable.mandatory, false);
  assert.equal(stable.notes, "Yeni ana ekran.");
  assert.deepEqual(stable.localizedNotes, { en: "A new home screen.", tr: "Yeni ana ekran." });

  await publishR2(
    "promote",
    "--version",
    "2.1.0",
    "--from",
    "stable",
    "--to",
    "stable",
    "--rollout",
    "100",
  );
  const widened = JSON.parse(objects.get("v1/releases/stable.json").body);
  assert.equal(widened.rollout, 100);
  assert.equal(widened.minVersion, "2.0.3");
  assert.equal(widened.pub_date, stable.pub_date);
  assert.deepEqual(widened.localizedNotes, stable.localizedNotes);

  await assert.rejects(
    publishR2("publish", "--dir", dir, "--version", "2.0.9", "--channel", "beta"),
    /refusing to move it back/,
  );
  await assert.rejects(publishR2("promote", "--version", "2.2.0"), /announces 2\.1\.0/);
});

test("channel.mjs writes latest.json for the GitHub release", async () => {
  const out = join(dir, "latest.json");
  await run(process.execPath, [
    join(here, "channel.mjs"),
    "--dir",
    dir,
    "--version",
    "2.1.0",
    "--base-url",
    "https://github.com/projects-gg/MinecraftLauncher/releases/download/v2.1.0",
    "--require",
    "windows-x86_64,darwin-aarch64,darwin-x86_64,linux-x86_64",
    "--notes-file",
    bilingual,
    "--out",
    out,
  ]);
  const latest = JSON.parse(readFileSync(out, "utf8"));
  assert.equal(latest.channel, undefined);
  assert.equal(latest.notes, "Yeni ana ekran.");
  assert.deepEqual(latest.localizedNotes, { en: "A new home screen.", tr: "Yeni ana ekran." });
  assert.equal(
    latest.platforms["linux-x86_64"].url,
    "https://github.com/projects-gg/MinecraftLauncher/releases/download/v2.1.0/Projects.Launcher_2.1.0_amd64.AppImage",
  );
  await assert.rejects(
    run(process.execPath, [
      join(here, "channel.mjs"),
      "--dir",
      dir,
      "--version",
      "2.1.0",
      "--base-url",
      "https://example.test/",
      "--require",
      "windows-aarch64",
    ]),
    /missing updater platforms: windows-aarch64/,
  );
});
