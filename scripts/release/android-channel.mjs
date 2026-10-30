#!/usr/bin/env node
// The Android app's release channel (docs/content-format.md, "Android release channel"): the
// signed v1/android/<channel>.json, its .sig and the APK it points at. The channel is signed with
// the launcher's updater key and scheme (minisign, as `tauri signer sign` writes it), so the app
// trusts the key in apps/launcher/src-tauri/tauri.conf.json and no other key exists for it.
//
//   make     Reads a signed release APK (aapt2 dump badging, apksigner), writes android-<channel>.json
//            and a copy of the APK into --out, signs the channel file with the updater key
//            (`tauri signer sign --app-version`, no password, stdin closed) and checks the signature
//            with the launcher's public key. The key is read by the signer only; nothing prints it.
//   verify   Checks a made folder (--dir) or the live channel (--url; the APK's bytes too with
//            --check-apk).
//   publish  Uploads a made folder to R2: the APK (immutable: the same bytes are skipped, other bytes
//            under an existing name are refused), the .sig, the channel file last, each read back. A
//            channel never moves to a lower versionCode. Then the clean-up: an APK folder no channel
//            names goes once a newer version's folder is 14 days old (storage cleans itself; nothing
//            is deleted when the listing or a channel file cannot be read).
//   show     Prints a live channel file.
//
//   node scripts/release/android-channel.mjs make --apk <release.apk> --notes-tr <txt> --notes-en <txt> \
//     --out <dir> [--channel stable|beta] [--key secrets/updater.key] [--pubkey <key>] [--cert <sha256>|any] \
//     [--package gg.projects.mobile] [--source-tag android-<v>] [--sdk <Android SDK>] [--allow-http]
//   node scripts/release/android-channel.mjs verify --dir <dir> | --url <channel url> [--check-apk] [--channel stable]
//   node scripts/release/android-channel.mjs publish --dir <dir> [--dry-run] [--allow-downgrade] [--no-prune]
//   node scripts/release/android-channel.mjs show [--channel stable]
//
// Environment: R2_ACCOUNT_ID, R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY, R2_BUCKET for publish and show
// (secrets/r2.env), CDN_BASE_URL (default https://cdn.projects.gg), R2_PREFIX (default v1),
// ANDROID_HOME or ANDROID_SDK_ROOT (or --sdk) for make.

import { spawnSync } from "node:child_process";
import { createHash, createPublicKey, verify as cryptoVerify } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  canonicalJson,
  compareVersions,
  md5Hex,
  parseArgs,
  parseVersion,
  R2Client,
  rfc3339,
  run,
  sha256Hex,
  UsageError,
} from "./lib.mjs";

const USAGE = `usage: node scripts/release/android-channel.mjs <make|verify|publish|show> [options]
  make    --apk <apk> --notes-tr <file> --notes-en <file> --out <dir> [--channel stable|beta]
          [--key <minisign secret key>] [--pubkey <public key>] [--cert <sha256>|any]
          [--package <application id>] [--source-tag <tag>] [--sdk <dir>] [--tauri <tauri.js>] [--allow-http]
  verify  --dir <dir> | --url <channel url> [--check-apk] [--channel stable|beta] [--pubkey <public key>]
  publish --dir <dir> [--dry-run] [--allow-downgrade] [--no-prune] [--keep-days 14] [--pubkey <public key>]
  show    [--channel stable|beta]`;

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const CDN = (process.env.CDN_BASE_URL || "https://cdn.projects.gg").replace(/\/+$/, "");
const PREFIX = (process.env.R2_PREFIX || "v1").replace(/^\/+|\/+$/g, "");
const CHANNELS = ["stable", "beta"];
const IMMUTABLE = "public, max-age=31536000, immutable";
const MUTABLE = "public, max-age=60";
const JSON_TYPE = "application/json; charset=utf-8";
const SIG_TYPE = "text/plain; charset=utf-8";
const APK_TYPE = "application/vnd.android.package-archive";
/** The release key's certificate (docs/mobile.md, "Signing and releasing"). */
export const RELEASE_CERT = "0aab804584dfccff406c80f982d05b5ffec537a89132e2fb4fd3fedde8108ba3";
export const DEFAULT_PACKAGE = "gg.projects.mobile";
export const SCHEMA = 1;
const MAX_NOTES = 1500;
const MAX_NOTE_LINES = 20;
const MAX_APK = 512 * 1024 * 1024;
const DAY_MS = 24 * 60 * 60 * 1000;

const channelKey = (channel) => `${PREFIX}/android/${channel}.json`;
const apkName = (versionName) => `projects-launcher-${versionName}.apk`;
const apkKey = (versionName) => `${PREFIX}/android/${versionName}/${apkName(versionName)}`;
export const signedName = (channel) => `android-${channel}.json`;

function checkChannelName(name) {
  if (!CHANNELS.includes(name)) throw new UsageError(`unknown channel "${name}" (${CHANNELS})`);
  return name;
}

// --- minisign (the launcher's updater signatures) -------------------------------------------

const ED25519_SPKI = Buffer.from("302a300506032b6570032100", "hex");

function strictBase64(text) {
  if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(text) || !text) {
    return null;
  }
  return Buffer.from(text, "base64");
}

/**
 * A minisign public key: the key line (`RWQ...`), the two-line text, or the base64 of that text
 * (tauri.conf.json's `plugins.updater.pubkey`).
 */
export function minisignPublicKey(text) {
  let value = String(text).trim();
  if (!value.startsWith("untrusted comment:")) {
    const raw = strictBase64(value);
    if (raw?.length === 42) return keyFromBox(raw);
    const decoded = raw?.toString("utf8").trim() ?? "";
    if (!decoded.startsWith("untrusted comment:")) throw new Error("not a minisign public key");
    value = decoded;
  }
  const line = value.split("\n")[1]?.trim() ?? "";
  const raw = strictBase64(line);
  if (raw?.length !== 42) throw new Error("not a minisign public key");
  return keyFromBox(raw);
}

function keyFromBox(raw) {
  if (raw.subarray(0, 2).toString("latin1") !== "Ed")
    throw new Error("not an Ed25519 minisign key");
  const keyId = raw.subarray(2, 10);
  return {
    keyId,
    key: raw.subarray(10, 42),
    id: Buffer.from(keyId).reverse().toString("hex").toUpperCase(),
  };
}

function ed25519(signature, key, message) {
  const publicKey = createPublicKey({
    key: Buffer.concat([ED25519_SPKI, key]),
    format: "der",
    type: "spki",
  });
  return cryptoVerify(null, message, publicKey, signature);
}

/**
 * Verifies a `.sig` (the base64 of minisign's text, as Tauri writes it, or the text itself) over
 * `data`: prehashed "ED" only, the signature and the trusted comment's signature. Returns the
 * trusted comment and its tab-separated `name:value` fields.
 */
export function minisignVerify(data, sigFile, keys) {
  let text = Buffer.from(sigFile).toString("utf8").trim();
  if (!text.startsWith("untrusted comment:")) {
    const decoded = strictBase64(text);
    if (!decoded) throw new Error("the signature file is neither minisign text nor its base64");
    text = decoded.toString("utf8");
  }
  const lines = text.split("\n").map((l) => l.replace(/\r$/, ""));
  if (
    lines.length < 4 ||
    !lines[0].startsWith("untrusted comment:") ||
    !lines[2].startsWith("trusted comment: ")
  ) {
    throw new Error("the signature file does not have minisign's four lines");
  }
  const box = strictBase64(lines[1].trim());
  if (box?.length !== 74) throw new Error("the signature line is not base64 of 74 bytes");
  if (box.subarray(0, 2).toString("latin1") !== "ED") {
    throw new Error("only prehashed (ED) signatures are accepted");
  }
  const keyId = box.subarray(2, 10);
  const key = keys.find((k) => k.keyId.equals(keyId));
  if (!key) throw new Error("signed by an untrusted key");
  const signature = box.subarray(10);
  const comment = lines[2].slice("trusted comment: ".length);
  const global = strictBase64(lines[3].trim());
  if (global?.length !== 64)
    throw new Error("the trusted comment's signature is not base64 of 64 bytes");
  const digest = createHash("blake2b512").update(data).digest();
  if (!ed25519(signature, key.key, digest)) throw new Error("the signature does not verify");
  if (!ed25519(global, key.key, Buffer.concat([signature, Buffer.from(comment, "utf8")]))) {
    throw new Error("the trusted comment's signature does not verify");
  }
  const fields = {};
  for (const part of comment.split("\t")) {
    const colon = part.indexOf(":");
    if (colon > 0 && !(part.slice(0, colon) in fields)) {
      fields[part.slice(0, colon)] = part.slice(colon + 1);
    }
  }
  return { comment, fields, key };
}

/** The launcher's updater public key from tauri.conf.json, or `override` (a key in any spelling). */
export function launcherPublicKey(override) {
  if (override) return minisignPublicKey(override);
  const conf = JSON.parse(
    readFileSync(join(REPO, "apps/launcher/src-tauri/tauri.conf.json"), "utf8"),
  );
  return minisignPublicKey(conf.plugins.updater.pubkey);
}

// --- the channel file ----------------------------------------------------------------------

const APPLICATION_ID = /^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$/;
const HEX64 = /^[0-9a-f]{64}$/;
const TAG = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/;

function isUrl(text, allowHttp) {
  try {
    const url = new URL(text);
    return (url.protocol === "https:" || (allowHttp && url.protocol === "http:")) && !!url.hostname;
  } catch {
    return false;
  }
}

/** Release notes as the channel carries them: one line per item, at most 20 lines, 1,500 characters. */
export function cleanNotes(text, what) {
  const lines = String(text)
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  // biome-ignore lint/suspicious/noControlCharactersInRegex: control characters are what it refuses
  if (lines.some((line) => /[\u0000-\u001f\u007f]/.test(line))) {
    throw new Error(`${what}: control characters in the notes`);
  }
  if (lines.length === 0) throw new Error(`${what}: the notes are empty`);
  if (lines.length > MAX_NOTE_LINES) throw new Error(`${what}: more than ${MAX_NOTE_LINES} lines`);
  const joined = lines.join("\n");
  if ([...joined].length > MAX_NOTES) throw new Error(`${what}: more than ${MAX_NOTES} characters`);
  return joined;
}

/**
 * The format's rules for a channel file (the app's UpdateManifest refuses the same ones), plus the
 * writer's own: a semver versionName (it is a mirror path) and notes in both languages.
 */
export function checkChannel(doc, channel, { allowHttp = false } = {}) {
  const fail = (message) => {
    throw new Error(`channel file: ${message}`);
  };
  if (!doc || typeof doc !== "object" || Array.isArray(doc)) fail("not an object");
  if (doc.schema !== SCHEMA) fail(`schema must be ${SCHEMA}`);
  if (doc.channel !== channel) fail(`channel is ${doc.channel}, not ${channel}`);
  if (typeof doc.applicationId !== "string" || !APPLICATION_ID.test(doc.applicationId)) {
    fail("applicationId is not valid");
  }
  if (typeof doc.versionName !== "string" || doc.versionName.length > 32)
    fail("versionName is not valid");
  parseVersion(doc.versionName);
  if (
    !Number.isInteger(doc.versionCode) ||
    doc.versionCode < 1 ||
    doc.versionCode > 2_100_000_000
  ) {
    fail("versionCode is not valid");
  }
  if (!Number.isInteger(doc.minSdk) || doc.minSdk < 1 || doc.minSdk > 1000)
    fail("minSdk is not valid");
  if (typeof doc.pubDate !== "string" || Number.isNaN(Date.parse(doc.pubDate))) {
    fail("pubDate is not RFC 3339");
  }
  const apk = doc.apk;
  if (!apk || typeof apk !== "object") fail("apk is missing");
  if (!Number.isInteger(apk.size) || apk.size < 1 || apk.size > MAX_APK)
    fail("apk.size is not valid");
  if (typeof apk.sha256 !== "string" || !HEX64.test(apk.sha256)) fail("apk.sha256 is not valid");
  if (!Array.isArray(apk.urls) || apk.urls.length < 1 || apk.urls.length > 4) {
    fail("apk.urls needs 1 to 4 addresses");
  }
  for (const url of apk.urls) {
    if (typeof url !== "string" || url.length > 2048 || !isUrl(url, allowHttp)) {
      fail(`apk.urls has an address that is not https: ${url}`);
    }
  }
  if (
    !Array.isArray(apk.signerSha256) ||
    apk.signerSha256.length < 1 ||
    apk.signerSha256.length > 4
  ) {
    fail("apk.signerSha256 needs 1 to 4 certificates");
  }
  for (const cert of apk.signerSha256) {
    if (typeof cert !== "string" || !HEX64.test(cert))
      fail("apk.signerSha256 has an invalid entry");
  }
  for (const lang of ["tr", "en"]) {
    cleanNotes(doc.notes?.[lang] ?? "", `notes.${lang}`);
  }
  if (doc.source !== undefined) {
    if (typeof doc.source.tag !== "string" || !TAG.test(doc.source.tag))
      fail("source.tag is not valid");
    if (!isUrl(doc.source.url, false)) fail("source.url is not https");
  }
  return doc;
}

/** Verifies a channel file and its `.sig`: signature, the trusted comment's file and version, the rules. */
export function verifyChannel(json, sig, channel, keys, options = {}) {
  const signed = minisignVerify(json, sig, keys);
  const doc = JSON.parse(Buffer.from(json).toString("utf8"));
  checkChannel(doc, channel, options);
  if (signed.fields.file !== signedName(channel)) {
    throw new Error(`the signature names ${signed.fields.file}, not ${signedName(channel)}`);
  }
  if (signed.fields.version !== doc.versionName) {
    throw new Error(
      `the signature is for version ${signed.fields.version}, not ${doc.versionName}`,
    );
  }
  return { doc, key: signed.key };
}

/** The channel file for an APK's facts. */
export function buildChannel({
  facts,
  size,
  sha256,
  channel,
  notesTr,
  notesEn,
  cdn,
  sourceTag,
  pubDate,
}) {
  const tag = sourceTag ?? `android-${facts.versionName}`;
  return {
    schema: SCHEMA,
    channel,
    applicationId: facts.packageName,
    versionName: facts.versionName,
    versionCode: facts.versionCode,
    minSdk: facts.minSdk,
    pubDate: pubDate ?? rfc3339(),
    apk: {
      urls: [`${cdn}/${PREFIX}/android/${facts.versionName}/${apkName(facts.versionName)}`],
      size,
      sha256,
      signerSha256: facts.signers,
    },
    notes: { tr: cleanNotes(notesTr, "notes.tr"), en: cleanNotes(notesEn, "notes.en") },
    source: { tag, url: `https://github.com/projects-gg/projects-mobile/tree/${tag}` },
  };
}

// --- reading the APK (Android SDK build tools) ---------------------------------------------

/** `aapt2 dump badging`: the package name, the version and the minimum Android. */
export function parseBadging(text) {
  const pkg = /^package: name='([^']+)' versionCode='(\d+)' versionName='([^']*)'/m.exec(text);
  const sdk = /^(?:sdkVersion|minSdkVersion):'(\d+)'/m.exec(text);
  if (!pkg || !sdk) throw new Error("aapt2 did not name the package, its version or its minSdk");
  return {
    packageName: pkg[1],
    versionCode: Number(pkg[2]),
    versionName: pkg[3],
    minSdk: Number(sdk[1]),
  };
}

/** `apksigner verify --print-certs`: each signer's certificate SHA-256 (lowercase hex). */
export function parseSigners(text) {
  const certs = [
    ...text.matchAll(/^Signer #\d+ certificate SHA-256 digest: ([0-9a-fA-F:]+)\s*$/gm),
  ].map((m) => m[1].replace(/:/g, "").toLowerCase());
  if (certs.length === 0) throw new Error("apksigner printed no signer");
  return [...new Set(certs)];
}

function buildTools(sdkFlag) {
  const sdk = sdkFlag || process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT;
  if (!sdk) throw new Error("set ANDROID_HOME (or --sdk) to read the APK");
  const root = join(sdk, "build-tools");
  const versions = readdirSync(root)
    .filter((v) => /^\d+\.\d+\.\d+/.test(v))
    .sort((a, b) => b.localeCompare(a, "en", { numeric: true }));
  for (const version of versions) {
    const dir = join(root, version);
    const aapt2 = ["aapt2.exe", "aapt2"].map((n) => join(dir, n)).find((p) => existsSync(p));
    const apksigner = join(dir, "lib", "apksigner.jar");
    if (aapt2 && existsSync(apksigner)) return { aapt2, apksigner };
  }
  throw new Error(`no build-tools with aapt2 and apksigner under ${root}`);
}

function tool(command, args) {
  const out = spawnSync(command, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    timeout: 120_000,
    maxBuffer: 64 * 1024 * 1024,
  });
  if (out.error) throw new Error(`${command}: ${out.error.message}`);
  if (out.status !== 0) throw new Error(`${command} failed: ${(out.stderr || out.stdout).trim()}`);
  return out.stdout;
}

function apkFacts(apk, sdkFlag) {
  const { aapt2, apksigner } = buildTools(sdkFlag);
  const facts = parseBadging(tool(aapt2, ["dump", "badging", apk]));
  // apksigner verifies every scheme the APK carries before it prints the certificates.
  facts.signers = parseSigners(tool("java", ["-jar", apksigner, "verify", "--print-certs", apk]));
  return facts;
}

function fileFacts(path) {
  const bytes = readFileSync(path);
  return { bytes, size: bytes.length, sha256: sha256Hex(bytes) };
}

// --- commands ------------------------------------------------------------------------------

function sign(file, version, keyPath, tauriJs) {
  if (!existsSync(keyPath)) throw new Error(`no signing key at ${keyPath}`);
  if (!existsSync(tauriJs)) throw new Error(`no Tauri CLI at ${tauriJs} (pnpm install first)`);
  // The key comes from the file only: no key or password from the environment, no prompt.
  const env = { ...process.env };
  for (const name of Object.keys(env)) {
    if (name.startsWith("TAURI_SIGNING_PRIVATE_KEY")) delete env[name];
  }
  const out = spawnSync(
    process.execPath,
    [
      tauriJs,
      "signer",
      "sign",
      "--private-key-path",
      keyPath,
      "--password",
      "",
      "--app-version",
      version,
      file,
    ],
    { stdio: ["ignore", "pipe", "pipe"], timeout: 120_000, env },
  );
  if (out.error || out.status !== 0) {
    throw new Error(`tauri signer sign failed (exit ${out.status ?? out.error?.message})`);
  }
}

async function make(flags) {
  for (const name of ["apk", "notes-tr", "notes-en", "out"]) {
    if (!flags[name]) throw new UsageError(`make needs --${name}`);
  }
  const channel = checkChannelName(flags.channel ?? "stable");
  const facts = apkFacts(flags.apk, flags.sdk);
  const wantPackage = flags.package ?? DEFAULT_PACKAGE;
  if (facts.packageName !== wantPackage) {
    throw new Error(`the APK is ${facts.packageName}, not ${wantPackage}`);
  }
  parseVersion(facts.versionName);
  const cert = (flags.cert ?? RELEASE_CERT).toLowerCase();
  if (cert !== "any" && (facts.signers.length !== 1 || facts.signers[0] !== cert)) {
    throw new Error(
      `the APK is signed by ${facts.signers.join(", ")}, not the release key ${cert}`,
    );
  }
  const apk = fileFacts(flags.apk);
  const doc = buildChannel({
    facts,
    size: apk.size,
    sha256: apk.sha256,
    channel,
    notesTr: readFileSync(flags["notes-tr"], "utf8"),
    notesEn: readFileSync(flags["notes-en"], "utf8"),
    cdn: CDN,
    sourceTag: flags["source-tag"],
  });
  checkChannel(doc, channel, { allowHttp: Boolean(flags["allow-http"]) });
  mkdirSync(flags.out, { recursive: true });
  const jsonPath = join(flags.out, signedName(channel));
  const json = canonicalJson(doc);
  // Exact bytes (LF): the signature covers them.
  writeFileSync(jsonPath, Buffer.from(json, "utf8"));
  copyFileSync(flags.apk, join(flags.out, apkName(facts.versionName)));
  sign(
    jsonPath,
    facts.versionName,
    flags.key ?? join(REPO, "secrets/updater.key"),
    flags.tauri ?? join(REPO, "apps/launcher/node_modules/@tauri-apps/cli/tauri.js"),
  );
  const keys = [launcherPublicKey(flags.pubkey)];
  const { key } = verifyChannel(Buffer.from(json), readFileSync(`${jsonPath}.sig`), channel, keys, {
    allowHttp: Boolean(flags["allow-http"]),
  });
  console.log(`made ${jsonPath} (+ .sig, signed by ${key.id}) and ${apkName(facts.versionName)}`);
  console.log(
    `${doc.applicationId} ${doc.versionName} (versionCode ${doc.versionCode}, minSdk ${doc.minSdk}), ${apk.size} bytes, sha256 ${apk.sha256}`,
  );
}

/** The made folder's channel file, its `.sig` and APK, all checked. */
function readMade(dir, options = {}) {
  const names = readdirSync(dir).filter((n) => /^android-[a-z]+\.json$/.test(n));
  if (names.length !== 1) throw new Error(`expected one android-<channel>.json in ${dir}`);
  const channel = checkChannelName(names[0].slice("android-".length, -".json".length));
  const json = readFileSync(join(dir, names[0]));
  const sig = readFileSync(join(dir, `${names[0]}.sig`));
  const keys = [launcherPublicKey(options.pubkey)];
  const { doc, key } = verifyChannel(json, sig, channel, keys, options);
  const apkPath = join(dir, apkName(doc.versionName));
  const apk = fileFacts(apkPath);
  if (apk.size !== doc.apk.size || apk.sha256 !== doc.apk.sha256) {
    throw new Error(`${apkPath} is not the file the channel announces`);
  }
  return { channel, json, sig, doc, apk, key };
}

async function fetchBytes(url) {
  const res = await fetch(url, { headers: { "accept-encoding": "identity" } });
  if (!res.ok) throw new Error(`GET ${url}: HTTP ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

async function verify(flags) {
  const options = { pubkey: flags.pubkey, allowHttp: Boolean(flags["allow-http"]) };
  if (flags.dir) {
    const made = readMade(flags.dir, options);
    console.log(
      `ok: ${made.channel} ${made.doc.versionName} (versionCode ${made.doc.versionCode}) signed by ${made.key.id}; the APK matches`,
    );
    return;
  }
  if (!flags.url) throw new UsageError("verify needs --dir or --url");
  const channel = checkChannelName(flags.channel ?? "stable");
  const stamp = `?t=${Date.now()}`;
  const json = await fetchBytes(flags.url + stamp);
  const sig = await fetchBytes(`${flags.url}.sig${stamp}`);
  const { doc, key } = verifyChannel(
    json,
    sig,
    channel,
    [launcherPublicKey(flags.pubkey)],
    options,
  );
  console.log(
    `ok: ${flags.url}: ${doc.versionName} (versionCode ${doc.versionCode}) signed by ${key.id}`,
  );
  if (flags["check-apk"]) {
    const bytes = await fetchBytes(doc.apk.urls[0]);
    if (bytes.length !== doc.apk.size || sha256Hex(bytes) !== doc.apk.sha256) {
      throw new Error(`${doc.apk.urls[0]} is not the announced file`);
    }
    console.log(`ok: ${doc.apk.urls[0]} (${bytes.length} bytes, sha256 matches)`);
  }
}

/**
 * The clean-up's plan for `objects` under `${prefix}android/`: a version folder no channel names
 * goes once the next newer version's folder is `keepMs` old (players read a fresh channel before
 * they download, so only a download that started before the switch can still want it). Channel
 * files, folders that are not versions and folders with no newer one are never touched.
 */
export function planPrune(objects, referenced, prefix, now, keepMs) {
  const base = `${prefix}android/`;
  const folders = new Map();
  for (const object of objects) {
    if (!object.key.startsWith(base)) continue;
    const rest = object.key.slice(base.length);
    const slash = rest.indexOf("/");
    if (slash <= 0) continue;
    const name = rest.slice(0, slash);
    if (!folders.has(name)) folders.set(name, []);
    folders.get(name).push(object);
  }
  const versions = [];
  const unknown = [];
  for (const name of folders.keys()) {
    try {
      parseVersion(name);
      versions.push(name);
    } catch {
      unknown.push(name);
    }
  }
  versions.sort(compareVersions);
  const remove = [];
  const waiting = [];
  versions.forEach((name, i) => {
    if (referenced.has(name)) return;
    const newer = versions[i + 1];
    if (!newer) {
      waiting.push({ name, reason: "no newer version" });
      return;
    }
    const since = Math.min(...folders.get(newer).map((o) => o.lastModified));
    if (now - since >= keepMs) remove.push({ name, objects: folders.get(name) });
    else
      waiting.push({ name, reason: `${newer} is ${Math.floor((now - since) / DAY_MS)} days old` });
  });
  return { remove, waiting, unknown };
}

function folderOf(url) {
  const base = `${CDN}/${PREFIX}/android/`;
  if (!url.startsWith(base)) return null;
  const rest = url.slice(base.length);
  const slash = rest.indexOf("/");
  return slash > 0 ? rest.slice(0, slash) : null;
}

async function prune(r2, keys, keepMs, dryRun) {
  const referenced = new Set();
  for (const channel of CHANNELS) {
    const json = await r2.get(channelKey(channel));
    if (!json) continue;
    const sig = await r2.get(`${channelKey(channel)}.sig`);
    let doc;
    try {
      ({ doc } = verifyChannel(json, sig ?? Buffer.alloc(0), channel, keys));
    } catch (err) {
      console.log(`clean-up skipped: ${channelKey(channel)} does not verify (${err.message})`);
      return;
    }
    for (const url of doc.apk.urls) {
      const folder = folderOf(url);
      if (folder) referenced.add(folder);
    }
  }
  let objects;
  try {
    objects = await r2.list(`${PREFIX}/android/`);
  } catch (err) {
    console.log(`clean-up skipped: the listing failed (${err.message})`);
    return;
  }
  const plan = planPrune(objects, referenced, `${PREFIX}/`, Date.now(), keepMs);
  for (const name of plan.unknown)
    console.log(`clean-up: ${name}/ is not a version folder, left alone`);
  for (const { name, reason } of plan.waiting) console.log(`clean-up: ${name} waits (${reason})`);
  let freed = 0;
  for (const { name, objects: files } of plan.remove) {
    for (const file of files) {
      if (dryRun) {
        console.log(`[dry-run] would delete ${file.key}`);
      } else {
        await r2.delete(file.key);
        console.log(`deleted ${file.key}`);
      }
      freed += file.size;
    }
    console.log(
      `clean-up: ${name} ${dryRun ? "would free" : "freed"} ${Math.round(freed / 1e6)} MB`,
    );
  }
  if (plan.remove.length === 0) console.log("clean-up: nothing to delete");
}

async function putChecked(r2, key, body, contentType, cacheControl) {
  const sha256 = sha256Hex(body);
  await r2.put(key, body, { contentType, cacheControl, sha256 });
  const check = await r2.head(key);
  const md5 = md5Hex(body);
  const etagOk = !check?.etag || check.etag.includes("-") || check.etag === md5;
  if (!check || check.size !== body.length || check.sha256 !== sha256 || !etagOk) {
    throw new Error(`verification failed for ${key}`);
  }
}

async function publish(flags, r2) {
  if (!flags.dir) throw new UsageError("publish needs --dir");
  const dryRun = Boolean(flags["dry-run"]);
  const keys = [launcherPublicKey(flags.pubkey)];
  const made = readMade(flags.dir, { pubkey: flags.pubkey });
  const { channel, doc } = made;
  const target = channelKey(channel);
  if (!r2) {
    console.log(`[dry-run] would upload ${apkKey(doc.versionName)}, ${target}.sig and ${target}`);
    return;
  }
  // The channel never moves back; the same version again must be the same bytes.
  const live = await r2.get(target);
  if (live) {
    let liveCode = 0;
    try {
      liveCode = JSON.parse(live.toString("utf8")).versionCode ?? 0;
    } catch {
      console.log(`note: the live ${target} cannot be read; it is replaced`);
    }
    if (live.equals(made.json)) {
      console.log(`same     ${target} (${doc.versionName} is live already)`);
    } else if (doc.versionCode === liveCode) {
      throw new Error(
        `${target} has versionCode ${liveCode} with other content; raise the versionCode`,
      );
    } else if (doc.versionCode < liveCode && !flags["allow-downgrade"]) {
      throw new Error(`${channel} announces versionCode ${liveCode}; refusing to move it back`);
    }
  }
  // 1. The APK (immutable).
  const key = apkKey(doc.versionName);
  const existing = await r2.head(key);
  if (existing) {
    const same =
      existing.sha256 === made.apk.sha256 ||
      (!existing.sha256 && existing.etag === md5Hex(made.apk.bytes));
    if (!same)
      throw new Error(`${key} already exists with other content; APK folders are immutable`);
    console.log(`same     ${key}`);
  } else if (dryRun) {
    console.log(`[dry-run] would upload ${key} (${made.apk.size} bytes)`);
  } else {
    await putChecked(r2, key, made.apk.bytes, APK_TYPE, IMMUTABLE);
    console.log(`uploaded ${key} (${made.apk.size} bytes)`);
  }
  // 2. The signature, then the channel file (the commit point), each read back.
  if (!live?.equals(made.json)) {
    if (dryRun) {
      console.log(
        `[dry-run] would write ${target}.sig and ${target}:\n${made.json.toString("utf8")}`,
      );
    } else {
      await putChecked(r2, `${target}.sig`, made.sig, SIG_TYPE, MUTABLE);
      await putChecked(r2, target, made.json, JSON_TYPE, MUTABLE);
      const back = await r2.get(target);
      if (!back?.equals(made.json)) throw new Error(`read-back of ${target} does not match`);
      console.log(`wrote ${CDN}/${target} (${doc.versionName}, versionCode ${doc.versionCode})`);
    }
  }
  // 3. The clean-up of folders no channel names.
  if (!flags["no-prune"]) {
    const days = Number(flags["keep-days"] ?? 14);
    if (!Number.isFinite(days) || days < 1) throw new UsageError("--keep-days must be at least 1");
    await prune(r2, keys, days * DAY_MS, dryRun);
  }
}

async function show(flags, r2) {
  const channel = checkChannelName(flags.channel ?? "stable");
  const json = r2
    ? await r2.get(channelKey(channel))
    : await fetchBytes(`${CDN}/${channelKey(channel)}`);
  if (!json) throw new Error(`${channelKey(channel)} does not exist`);
  process.stdout.write(json.toString("utf8"));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  run(async () => {
    const [command, ...rest] = process.argv.slice(2);
    const { flags, positional } = parseArgs(rest, {
      apk: "string",
      "check-apk": "boolean",
      "notes-tr": "string",
      "notes-en": "string",
      out: "string",
      channel: "string",
      key: "string",
      pubkey: "string",
      cert: "string",
      package: "string",
      "source-tag": "string",
      sdk: "string",
      tauri: "string",
      "allow-http": "boolean",
      dir: "string",
      url: "string",
      "dry-run": "boolean",
      "allow-downgrade": "boolean",
      "no-prune": "boolean",
      "keep-days": "string",
    });
    if (positional.length) throw new UsageError(`unexpected argument ${positional[0]}`);
    const commands = { make, verify, publish, show };
    if (!Object.hasOwn(commands, command ?? "")) throw new UsageError("expected a command");
    let r2 = null;
    if (command === "publish" || command === "show") {
      const { client, missing } = R2Client.fromEnv();
      if (!client && command === "publish" && !flags["dry-run"]) {
        throw new Error(`missing environment: ${missing.join(", ")}`);
      }
      r2 = client;
    }
    await commands[command](flags, r2);
  }, USAGE);
}
