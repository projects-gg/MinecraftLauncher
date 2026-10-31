// Tests of android-channel.mjs: the launcher's minisign signatures (the real signer's output and keys
// made per run), the channel file's rules, the APK tools' output, and `publish` with its clean-up
// against an in-memory S3 server that checks every request's SigV4 signature (as publish-r2.test.mjs).
// Run: `node --test "scripts/release/*.test.mjs"`.

import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash, sign as cryptoSign, generateKeyPairSync, randomBytes } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import {
  buildChannel,
  checkChannel,
  cleanNotes,
  launcherPublicKey,
  minisignPublicKey,
  minisignVerify,
  parseBadging,
  parseSigners,
  planPrune,
  RELEASE_CERT,
  readMakeNotes,
  verifyChannel,
} from "./android-channel.mjs";
import { canonicalJson, parseListing, signRequest } from "./lib.mjs";

const run = promisify(execFile);
const here = fileURLToPath(new URL(".", import.meta.url));
const DAY = 24 * 60 * 60 * 1000;

// The real `tauri signer sign --app-version 0.4.6` over this file with a throwaway key (its private
// half was never kept); the app's JVM tests use the same three.
const FIXTURE_JSON =
  '{\n  "apk": {\n    "sha256": "5f2b0c1d9e8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a3b2c",\n    "signerSha256": [\n      "0aab804584dfccff406c80f982d05b5ffec537a89132e2fb4fd3fedde8108ba3"\n    ],\n    "size": 159383552,\n    "urls": [\n      "https://cdn.projects.gg/v1/android/0.4.6/projects-launcher-0.4.6.apk"\n    ]\n  },\n  "applicationId": "gg.projects.mobile",\n  "channel": "stable",\n  "minSdk": 21,\n  "notes": {\n    "en": "The app now updates itself.\\nSee which version your friends are playing.",\n    "tr": "Uygulama art\u0131k kendini g\u00fcnceller.\\nArkada\u015flar\u0131n\u0131n hangi s\u00fcr\u00fcmde oynad\u0131\u011f\u0131n\u0131 g\u00f6r\u00fcrs\u00fcn."\n  },\n  "pubDate": "2026-11-02T12:00:00Z",\n  "schema": 1,\n  "source": {\n    "tag": "android-0.4.6",\n    "url": "https://github.com/projects-gg/projects-mobile/tree/android-0.4.6"\n  },\n  "versionCode": 22,\n  "versionName": "0.4.6"\n}\n';
const FIXTURE_SIG =
  "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUeFcwaXR0MThENloyS0NBcmdhZTVwUU00US8wditxYVZkLzdnQi9ML1lKV0NnZmRkTHcvdjlnWXA2WGY1czdaZWpWZk9paXAzUzhlV1E4ZXc2aVlLMGFIcTg5elR0T0FJPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkzMzYyNDUwCWZpbGU6YW5kcm9pZC1zdGFibGUuanNvbgl2ZXJzaW9uOjAuNC42CmpacjBUNWhNQmdROVllbEZ4MkYrMWE2U0tLMzUwTlhyM05EQUdiMnlWeTByMzh6am4xeEdiRC9wZWY3WjU2bmxFQjlZUkxnQXd1aSs4ZWlER3F1eEJ3PT0K";
const FIXTURE_KEY = "RWTxW0itt18D6d4Gh7zdmwUlGzxrxqHeF60E5E9A8dzjaWdI5ElFRnuB";
/** The launcher's updater key as the Android app embeds it (UpdateConfig.TRUSTED_KEYS). */
const APP_EMBEDDED_KEY = "RWRZUxpOQ3YGJrAPxah2aKj66geKiruOXpziNcYfp8i/Fr1Czb+XJa2N";

/** A minisign key made for this run, signing the way `tauri signer sign` does. */
function testKey() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const raw = Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url");
  const keyId = randomBytes(8);
  const line = Buffer.concat([Buffer.from("Ed"), keyId, raw]).toString("base64");
  const sigFile = (data, comment, prehashed = true) => {
    const message = prehashed ? createHash("blake2b512").update(data).digest() : data;
    const signature = cryptoSign(null, message, privateKey);
    const box = Buffer.concat([Buffer.from(prehashed ? "ED" : "Ed"), keyId, signature]);
    const global = cryptoSign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey);
    const text = `untrusted comment: signature from tauri secret key\n${box.toString("base64")}\ntrusted comment: ${comment}\n${global.toString("base64")}\n`;
    return Buffer.from(Buffer.from(text).toString("base64"));
  };
  return { line, sigFile };
}

const sampleFacts = (overrides = {}) => ({
  packageName: "gg.projects.mobile",
  versionCode: 22,
  versionName: "0.4.6",
  minSdk: 21,
  signers: [RELEASE_CERT],
  ...overrides,
});

test("the real signer's channel file verifies with its key, and nothing else does", () => {
  const key = minisignPublicKey(FIXTURE_KEY);
  assert.equal(key.id, "E9035FB7AD485BF1");
  const json = Buffer.from(FIXTURE_JSON);
  const { doc } = verifyChannel(json, Buffer.from(FIXTURE_SIG), "stable", [key]);
  assert.equal(doc.versionCode, 22);
  const signed = minisignVerify(json, Buffer.from(FIXTURE_SIG), [key]);
  assert.equal(signed.fields.file, "android-stable.json");
  assert.equal(signed.fields.version, "0.4.6");
  const changed = Buffer.from(FIXTURE_JSON.replace('"minSdk": 21', '"minSdk": 22'));
  assert.throws(() => minisignVerify(changed, Buffer.from(FIXTURE_SIG), [key]), /does not verify/);
  assert.throws(
    () => minisignVerify(json, Buffer.from(FIXTURE_SIG), [minisignPublicKey(testKey().line)]),
    /untrusted key/,
  );
  // The fixture is canonical JSON, as `make` writes it.
  assert.equal(canonicalJson(JSON.parse(FIXTURE_JSON)), FIXTURE_JSON);
});

test("content-format.md's example is the signed fixture and keeps the rules", () => {
  const spec = readFileSync(join(here, "../../docs/content-format.md"), "utf8");
  const at = spec.indexOf("<!-- android-channel example");
  assert.ok(at > 0, "the example's marker");
  const fence = "```json";
  const start = spec.indexOf(fence, at) + fence.length + 1;
  const example = spec.slice(start, spec.indexOf("```", start)).replace(/\r\n/g, "\n");
  assert.equal(example, FIXTURE_JSON);
  checkChannel(JSON.parse(example), "stable");
});

test("the app trusts exactly the launcher's updater key", () => {
  const launcher = launcherPublicKey();
  assert.equal(launcher.id, "260676434E1A5359");
  const embedded = minisignPublicKey(APP_EMBEDDED_KEY);
  assert.ok(launcher.key.equals(embedded.key));
  assert.ok(launcher.keyId.equals(embedded.keyId));
  assert.throws(() => minisignPublicKey("RWQ"), /not a minisign public key/);
  assert.throws(() => minisignPublicKey(Buffer.alloc(42).toString("base64")), /not an Ed25519/);
});

test("signatures: prehashed only, both parts valid, the trusted comment binds file and version", () => {
  const key = testKey();
  const keys = [minisignPublicKey(key.line)];
  const json = Buffer.from(
    canonicalJson(
      buildChannel({
        facts: sampleFacts(),
        size: 10,
        sha256: "a".repeat(64),
        channel: "stable",
        notesTr: "Yeni",
        notesEn: "New",
        cdn: "https://cdn.projects.gg",
        pubDate: "2026-11-02T12:00:00Z",
      }),
    ),
  );
  const good = key.sigFile(json, "timestamp:1\tfile:android-stable.json\tversion:0.4.6");
  assert.equal(verifyChannel(json, good, "stable", keys).doc.versionName, "0.4.6");
  const legacy = key.sigFile(json, "timestamp:1\tfile:android-stable.json\tversion:0.4.6", false);
  assert.throws(() => verifyChannel(json, legacy, "stable", keys), /prehashed/);
  const otherFile = key.sigFile(
    json,
    "timestamp:1\tfile:Projects.Launcher_2.1.0_x64-setup.exe\tversion:0.4.6",
  );
  assert.throws(() => verifyChannel(json, otherFile, "stable", keys), /names/);
  const otherVersion = key.sigFile(json, "timestamp:1\tfile:android-stable.json\tversion:0.4.5");
  assert.throws(() => verifyChannel(json, otherVersion, "stable", keys), /for version/);
  assert.throws(() => verifyChannel(json, good, "beta", keys), /channel is stable/);
  // A trusted comment edited after signing.
  const text = Buffer.from(good.toString(), "base64")
    .toString()
    .replace("version:0.4.6", "version:0.4.7");
  assert.throws(() => minisignVerify(json, Buffer.from(text), keys), /trusted comment's signature/);
});

test("the channel file's rules", () => {
  const doc = () =>
    buildChannel({
      facts: sampleFacts(),
      size: 159383552,
      sha256: "b".repeat(64),
      channel: "stable",
      notesTr: "- Yeni\n\n- Daha hızlı\r\n",
      notesEn: "- New\n- Faster\n",
      cdn: "https://cdn.projects.gg",
      pubDate: "2026-11-02T12:00:00Z",
    });
  const made = doc();
  assert.deepEqual(made.apk.urls, [
    "https://cdn.projects.gg/v1/android/0.4.6/projects-launcher-0.4.6.apk",
  ]);
  assert.equal(made.notes.tr, "- Yeni\n- Daha hızlı");
  assert.deepEqual(made.source, {
    tag: "android-0.4.6",
    url: "https://github.com/projects-gg/projects-mobile/tree/android-0.4.6",
  });
  checkChannel(made, "stable");
  const broken = [
    (d) => (d.schema = 2),
    (d) => (d.applicationId = "Gg.projects"),
    (d) => (d.versionName = "0.4.6+abc"),
    (d) => (d.versionCode = 0),
    (d) => (d.minSdk = 0),
    (d) => (d.pubDate = "soon"),
    (d) => (d.apk.size = 0),
    (d) => (d.apk.sha256 = "B".repeat(64)),
    (d) => (d.apk.urls = ["http://cdn.projects.gg/a.apk"]),
    (d) => (d.apk.urls = []),
    (d) => (d.apk.signerSha256 = ["0A:AB"]),
    (d) => (d.notes.en = ""),
    (d) => (d.source.url = "http://github.com/x"),
  ];
  for (const breakIt of broken) {
    const d = doc();
    breakIt(d);
    assert.throws(() => checkChannel(d, "stable"), Error, String(breakIt));
  }
  const local = doc();
  local.apk.urls = ["http://10.0.2.2:8791/v1/android/0.4.6/projects-launcher-0.4.6.apk"];
  checkChannel(local, "stable", { allowHttp: true });
  assert.throws(() => cleanNotes("a\u0007b", "x"), /control characters/);
  assert.throws(() => cleanNotes("x".repeat(1501), "x"), /1500 characters/);
  assert.throws(() => cleanNotes(Array(21).fill("x").join("\n"), "x"), /20 lines/);
});

test("make's notes: two files, or one with a --- en --- line as a launcher release's", () => {
  const dir = mkdtempSync(join(tmpdir(), "pl-android-notes-"));
  try {
    const file = (name, text) => {
      writeFileSync(join(dir, name), text);
      return join(dir, name);
    };
    const tr = file("notes-0.4.6-tr.txt", "- Yeni\n- Daha hızlı\n");
    const en = file("notes-0.4.6-en.txt", "- New\n- Faster\n");
    const both = file(
      "notes-0.4.6.txt",
      "- Yeni\n- Daha hızlı\r\n\r\n--- en ---\r\n- New\n- Faster\n",
    );
    const fromTwo = readMakeNotes({ "notes-tr": tr, "notes-en": en });
    const fromOne = readMakeNotes({ notes: both });
    const channel = (notes) =>
      buildChannel({
        facts: sampleFacts(),
        size: 1,
        sha256: "b".repeat(64),
        channel: "stable",
        cdn: "https://cdn.projects.gg",
        pubDate: "2026-11-02T12:00:00Z",
        ...notes,
      });
    assert.deepEqual(channel(fromOne).notes, { tr: "- Yeni\n- Daha hızlı", en: "- New\n- Faster" });
    assert.deepEqual(channel(fromOne), channel(fromTwo));
    // The app needs both languages, whichever way they come.
    const turkishOnly = readMakeNotes({ notes: file("tr-only.txt", "- Yeni\n") });
    assert.throws(() => channel(turkishOnly), /notes\.en: the notes are empty/);
    assert.throws(() => readMakeNotes({ notes: both, "notes-tr": tr }), /not both/);
    assert.throws(() => readMakeNotes({ "notes-tr": tr }), /needs --notes-tr and --notes-en/);
    assert.throws(
      () => readMakeNotes({ notes: file("bad.txt", "- Yeni\n--- English ---\n- New\n") }),
      /not a language line/,
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("reading aapt2's badging and apksigner's certificates", () => {
  const badging = [
    "package: name='gg.projects.mobile' versionCode='22' versionName='0.4.6' platformBuildVersionName='16' compileSdkVersion='37'",
    "minSdkVersion:'21'",
    "targetSdkVersion:'34'",
    "uses-permission: name='android.permission.REQUEST_INSTALL_PACKAGES'",
  ].join("\n");
  assert.deepEqual(parseBadging(badging), {
    packageName: "gg.projects.mobile",
    versionCode: 22,
    versionName: "0.4.6",
    minSdk: 21,
  });
  assert.equal(
    parseBadging("package: name='a.b' versionCode='3' versionName='1.0.0'\nsdkVersion:'26'\n")
      .minSdk,
    26,
  );
  assert.throws(() => parseBadging("nothing"), /aapt2/);
  const signers = [
    "Signer #1 certificate DN: CN=Projects",
    `Signer #1 certificate SHA-256 digest: ${RELEASE_CERT}`,
    "Signer #1 certificate SHA-1 digest: 1234",
  ].join("\r\n");
  assert.deepEqual(parseSigners(signers), [RELEASE_CERT]);
  assert.deepEqual(parseSigners("Signer #1 certificate SHA-256 digest: 0A:AB:80\n"), ["0aab80"]);
  assert.throws(() => parseSigners("Verifies\n"), /no signer/);
});

test("the clean-up's plan", () => {
  const now = Date.parse("2026-12-01T00:00:00Z");
  const object = (key, daysOld, size = 100) => ({ key, size, lastModified: now - daysOld * DAY });
  const objects = [
    object("v1/android/stable.json", 1),
    object("v1/android/stable.json.sig", 1),
    object("v1/android/0.4.4/projects-launcher-0.4.4.apk", 60),
    object("v1/android/0.4.5/projects-launcher-0.4.5.apk", 30),
    object("v1/android/0.4.6/projects-launcher-0.4.6.apk", 10),
    object("v1/android/0.4.10/projects-launcher-0.4.10.apk", 1),
    object("v1/android/notes/readme.txt", 90),
    object("v1/releases/2.0.0/x.exe", 90),
  ];
  const plan = planPrune(objects, new Set(["0.4.10"]), "v1/", now, 14 * DAY);
  // 0.4.4: 0.4.5 is 30 days old. 0.4.5: 0.4.6 is only 10 days old. 0.4.6: 0.4.10 is a day old.
  assert.deepEqual(
    plan.remove.map((r) => r.name),
    ["0.4.4"],
  );
  assert.deepEqual(
    plan.waiting.map((w) => w.name),
    ["0.4.5", "0.4.6"],
  );
  assert.deepEqual(plan.unknown, ["notes"]);
  // A folder newer than every channel's (an upload whose channel was never written) stays.
  const orphan = planPrune(
    [object("v1/android/0.4.6/a.apk", 90), object("v1/android/0.4.7/a.apk", 90)],
    new Set(["0.4.6"]),
    "v1/",
    now,
    14 * DAY,
  );
  assert.deepEqual(orphan.remove, []);
  assert.deepEqual(orphan.waiting, [{ name: "0.4.7", reason: "no newer version" }]);
});

test("ListObjectsV2 pages", () => {
  const page = parseListing(
    "<ListBucketResult><IsTruncated>true</IsTruncated><NextContinuationToken>a&amp;b</NextContinuationToken>" +
      "<Contents><Key>v1/android/0.4.6/a &amp; b.apk</Key><LastModified>2026-11-02T12:00:00.000Z</LastModified>" +
      "<ETag>&quot;x&quot;</ETag><Size>12</Size></Contents></ListBucketResult>",
  );
  assert.deepEqual(page, {
    objects: [
      {
        key: "v1/android/0.4.6/a & b.apk",
        size: 12,
        lastModified: Date.parse("2026-11-02T12:00:00Z"),
      },
    ],
    truncated: true,
    next: "a&b",
  });
  assert.throws(() => parseListing("<Error/>"), /ListObjectsV2/);
  assert.throws(
    () => parseListing("<ListBucketResult><Contents><Key>a</Key></Contents></ListBucketResult>"),
    /without/,
  );
});

// --- publish against an in-memory S3 ----------------------------------------------------------

const S3 = { accessKeyId: "test-key", secretAccessKey: "test-secret" };
const objects = new Map();
let server;
let env;
const temp = [];

function startServer() {
  return new Promise((resolve) => {
    server = createServer((req, res) => {
      const chunks = [];
      req.on("data", (c) => chunks.push(c));
      req.on("end", () => {
        const body = Buffer.concat(chunks);
        const url = new URL(req.url, "http://x");
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
          path: decodeURIComponent(url.pathname),
          query: url.search.slice(1),
          headers,
          payloadHash: req.headers["x-amz-content-sha256"],
          ...S3,
          region: "auto",
          date,
        }).authorization;
        if (auth !== expected) {
          res.writeHead(403).end("SignatureDoesNotMatch");
          return;
        }
        if (url.pathname === "/bucket" && req.method === "GET") {
          const prefix = url.searchParams.get("prefix") ?? "";
          const keys = [...objects.keys()].filter((k) => k.startsWith(prefix)).sort();
          const start = Number(url.searchParams.get("continuation-token") ?? 0);
          const pageKeys = keys.slice(start, start + 2);
          const more = start + 2 < keys.length;
          const contents = pageKeys
            .map((k) => {
              const o = objects.get(k);
              return `<Contents><Key>${k}</Key><LastModified>${new Date(o.lastModified).toISOString()}</LastModified><Size>${o.body.length}</Size></Contents>`;
            })
            .join("");
          res
            .writeHead(200)
            .end(
              `<ListBucketResult><IsTruncated>${more}</IsTruncated>${more ? `<NextContinuationToken>${start + 2}</NextContinuationToken>` : ""}${contents}</ListBucketResult>`,
            );
          return;
        }
        const key = decodeURIComponent(url.pathname).replace(/^\/bucket\//, "");
        if (req.method === "PUT") {
          objects.set(key, {
            body,
            md5: createHash("md5").update(body).digest("hex"),
            sha256: req.headers["x-amz-meta-sha256"],
            cacheControl: req.headers["cache-control"],
            contentType: req.headers["content-type"],
            lastModified: Date.now(),
          });
          res.writeHead(200).end();
          return;
        }
        if (req.method === "DELETE") {
          objects.delete(key);
          res.writeHead(204).end();
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

const key = testKey();

/** A made folder as `make` writes it, signed with this run's key. */
function madeFolder(versionName, versionCode, apkBytes) {
  const dir = mkdtempSync(join(tmpdir(), "pl-android-"));
  temp.push(dir);
  const doc = buildChannel({
    facts: sampleFacts({ versionName, versionCode }),
    size: apkBytes.length,
    sha256: createHash("sha256").update(apkBytes).digest("hex"),
    channel: "stable",
    notesTr: "Yeni",
    notesEn: "New",
    cdn: "https://cdn.example.test",
  });
  const json = Buffer.from(canonicalJson(doc));
  writeFileSync(join(dir, "android-stable.json"), json);
  writeFileSync(
    join(dir, "android-stable.json.sig"),
    key.sigFile(json, `timestamp:1\tfile:android-stable.json\tversion:${versionName}`),
  );
  writeFileSync(join(dir, `projects-launcher-${versionName}.apk`), apkBytes);
  return dir;
}

async function channelTool(...args) {
  return run(process.execPath, [join(here, "android-channel.mjs"), ...args, "--pubkey", key.line], {
    env,
  });
}

before(async () => {
  const port = await startServer();
  env = {
    ...process.env,
    R2_ACCOUNT_ID: "test",
    R2_ACCESS_KEY_ID: S3.accessKeyId,
    R2_SECRET_ACCESS_KEY: S3.secretAccessKey,
    R2_BUCKET: "bucket",
    R2_ENDPOINT: `http://127.0.0.1:${port}`,
    CDN_BASE_URL: "https://cdn.example.test",
  };
});

after(() => {
  server.close();
  for (const dir of temp) rmSync(dir, { recursive: true, force: true });
});

test("publish: the APK, the signature, the channel last; the same again is a no-op", async () => {
  const apk = randomBytes(4096);
  const dir = madeFolder("0.4.6", 22, apk);
  await channelTool("verify", "--dir", dir);
  const { stdout } = await channelTool("publish", "--dir", dir);
  assert.match(stdout, /uploaded v1\/android\/0\.4\.6\/projects-launcher-0\.4\.6\.apk/);
  assert.match(stdout, /wrote https:\/\/cdn\.example\.test\/v1\/android\/stable\.json/);
  const stored = objects.get("v1/android/0.4.6/projects-launcher-0.4.6.apk");
  assert.ok(stored.body.equals(apk));
  assert.equal(stored.cacheControl, "public, max-age=31536000, immutable");
  assert.equal(stored.contentType, "application/vnd.android.package-archive");
  assert.equal(objects.get("v1/android/stable.json").cacheControl, "public, max-age=60");
  assert.equal(objects.get("v1/android/stable.json.sig").cacheControl, "public, max-age=60");
  const again = await channelTool("publish", "--dir", dir);
  assert.match(again.stdout, /same {5}v1\/android\/stable\.json/);
  assert.match(again.stdout, /same {5}v1\/android\/0\.4\.6\//);
  // Other bytes under a published version: refused.
  const tampered = madeFolder("0.4.6", 22, randomBytes(4096));
  await assert.rejects(
    channelTool("publish", "--dir", tampered),
    /raise the versionCode|immutable/,
  );
  // A lower version: refused unless asked for.
  const older = madeFolder("0.4.5", 21, randomBytes(2048));
  await assert.rejects(channelTool("publish", "--dir", older), /refusing to move it back/);
});

test("publish refuses a folder whose signature or APK does not match", async () => {
  const dir = madeFolder("0.4.9", 25, randomBytes(1024));
  writeFileSync(join(dir, "projects-launcher-0.4.9.apk"), randomBytes(1024));
  await assert.rejects(
    channelTool("publish", "--dir", dir, "--dry-run"),
    /not the file the channel announces/,
  );
  const other = madeFolder("0.4.9", 25, randomBytes(1024));
  writeFileSync(
    join(other, "android-stable.json.sig"),
    testKey().sigFile(Buffer.from("x"), "file:android-stable.json"),
  );
  await assert.rejects(channelTool("publish", "--dir", other, "--dry-run"), /untrusted key/);
});

test("publish cleans up folders no channel names, 14 days after a newer one", async () => {
  // State before: 0.4.6 live (above). Old folders: 0.4.4 (superseded by 0.4.5 30 days ago), 0.4.5.
  const put = (k, daysOld) =>
    objects.set(k, {
      body: Buffer.from("old"),
      md5: createHash("md5").update("old").digest("hex"),
      sha256: createHash("sha256").update("old").digest("hex"),
      cacheControl: "public, max-age=31536000, immutable",
      lastModified: Date.now() - daysOld * DAY,
    });
  put("v1/android/0.4.4/projects-launcher-0.4.4.apk", 40);
  put("v1/android/0.4.5/projects-launcher-0.4.5.apk", 30);
  put("v1/android/odd/readme.txt", 90);
  objects.get("v1/android/0.4.6/projects-launcher-0.4.6.apk").lastModified = Date.now() - 20 * DAY;
  const f047 = madeFolder("0.4.7", 23, randomBytes(512));
  const dry = await channelTool("publish", "--dir", f047, "--dry-run");
  assert.match(dry.stdout, /\[dry-run\] would delete v1\/android\/0\.4\.4\//);
  assert.match(dry.stdout, /\[dry-run\] would delete v1\/android\/0\.4\.5\//);
  assert.ok(objects.has("v1/android/0.4.4/projects-launcher-0.4.4.apk"));
  const { stdout } = await channelTool("publish", "--dir", f047);
  // 0.4.4 and 0.4.5 go (their newer folders are 30 and 20 days old); 0.4.6 waits for 0.4.7 to age.
  assert.ok(!objects.has("v1/android/0.4.4/projects-launcher-0.4.4.apk"));
  assert.ok(!objects.has("v1/android/0.4.5/projects-launcher-0.4.5.apk"));
  assert.ok(objects.has("v1/android/0.4.6/projects-launcher-0.4.6.apk"));
  assert.ok(objects.has("v1/android/0.4.7/projects-launcher-0.4.7.apk"));
  assert.ok(objects.has("v1/android/odd/readme.txt"));
  assert.match(stdout, /0\.4\.6 waits/);
  // A channel file that does not verify stops the clean-up: nothing is deleted.
  put("v1/android/0.4.3/projects-launcher-0.4.3.apk", 90);
  objects.set("v1/android/beta.json", {
    ...objects.get("v1/android/stable.json"),
    body: Buffer.from("{}"),
  });
  const skipped = await channelTool("publish", "--dir", f047, "--no-prune");
  assert.doesNotMatch(skipped.stdout, /clean-up/);
  const blocked = await channelTool("publish", "--dir", madeFolder("0.4.8", 24, randomBytes(512)));
  assert.match(blocked.stdout, /clean-up skipped: v1\/android\/beta\.json does not verify/);
  assert.ok(objects.has("v1/android/0.4.3/projects-launcher-0.4.3.apk"));
});
