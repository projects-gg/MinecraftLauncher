// Shared helpers for the release scripts. Zero dependencies (Node 22+), so CI can run them
// right after checkout. See docs/release.md.

import { createHash, createHmac } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

// --- CLI -----------------------------------------------------------------------------------

/**
 * Minimal `--flag value` / `--switch` parser. Unknown flags are an error so typos fail loudly.
 * `spec` maps flag names to "string" | "boolean".
 */
export function parseArgs(argv, spec) {
  const flags = {};
  const positional = [];
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (!arg.startsWith("--")) {
      positional.push(arg);
      continue;
    }
    const [name, inline] = arg.slice(2).split(/=(.*)/s, 2);
    const kind = spec[name];
    if (!kind) throw new UsageError(`unknown option --${name}`);
    if (kind === "boolean") {
      flags[name] = inline === undefined ? true : inline !== "false";
    } else {
      const value = inline ?? argv[++i];
      if (value === undefined) throw new UsageError(`--${name} needs a value`);
      flags[name] = value;
    }
  }
  return { flags, positional };
}

export class UsageError extends Error {}

/** Runs `main`, printing errors without a stack for expected failures. */
export function run(main, usage) {
  main().catch((err) => {
    if (err instanceof UsageError) {
      console.error(`error: ${err.message}\n\n${usage}`);
      process.exit(2);
    }
    console.error(`error: ${err.message}`);
    if (process.env.RELEASE_DEBUG) console.error(err.stack);
    process.exit(1);
  });
}

// --- Versions ------------------------------------------------------------------------------

const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?$/;

/** Strict semver without build metadata (`+` is not allowed in mirror paths). */
export function parseVersion(text) {
  const m = SEMVER.exec(String(text).trim().replace(/^v/, ""));
  if (!m) throw new Error(`"${text}" is not a valid version (expected 2.1.0 or 2.1.0-beta.1)`);
  return {
    raw: m[0],
    core: [Number(m[1]), Number(m[2]), Number(m[3])],
    pre: m[4] ? m[4].split(".") : [],
  };
}

export function isPrerelease(text) {
  return parseVersion(text).pre.length > 0;
}

/** Semver precedence: negative when a < b. */
export function compareVersions(a, b) {
  const va = parseVersion(a);
  const vb = parseVersion(b);
  for (let i = 0; i < 3; i++) {
    if (va.core[i] !== vb.core[i]) return va.core[i] - vb.core[i];
  }
  if (!va.pre.length || !vb.pre.length) return vb.pre.length - va.pre.length;
  for (let i = 0; i < Math.max(va.pre.length, vb.pre.length); i++) {
    const x = va.pre[i];
    const y = vb.pre[i];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    if (x === y) continue;
    const nx = /^\d+$/.test(x);
    const ny = /^\d+$/.test(y);
    if (nx && ny) return Number(x) - Number(y);
    if (nx) return -1;
    if (ny) return 1;
    return x < y ? -1 : 1;
  }
  return 0;
}

// --- macOS bundle version ------------------------------------------------------------------

/** CFBundleVersion stage of each prerelease kind, in semver order; a release is MAC_RELEASE. */
const MAC_STAGES = new Map([
  ["alpha", 100],
  ["beta", 200],
  ["rc", 300],
]);
const MAC_RELEASE = 999;
const MAC_MAX_PART = 2 ** 31 - 1;
const MAC_BUNDLE_VERSION = /^(0|[1-9]\d*)(\.(0|[1-9]\d*)){0,2}$/;

/**
 * The macOS CFBundleVersion (`bundle.macOS.bundleVersion`) of a release version. Apple wants one
 * to three period-separated integers there, growing with every build; CFBundleShortVersionString
 * keeps the version players see (2.1.0-beta.1). MAJOR and MINOR stay, and the third number is
 * PATCH × 1000 + a stage: 100 + N for alpha.N, 200 + N for beta.N, 300 + N for rc.N (N 1–99,
 * 0 for a bare `beta`) and 999 for the release itself:
 *
 *   2.0.1-beta.1 → 2.0.1201   2.0.1-rc.2 → 2.0.1302   2.0.1 → 2.0.1999   2.1.0-beta.1 → 2.1.201
 *
 * So it sorts like semver: a prerelease above the release before it and below its own release.
 * Other prerelease forms are refused rather than given a number that could sort wrong.
 */
export function macBundleVersion(text) {
  const { raw, core, pre } = parseVersion(text);
  let stage = MAC_RELEASE;
  if (pre.length) {
    const [kind, number, ...rest] = pre;
    if (!MAC_STAGES.has(kind) || rest.length || !/^([1-9]\d?)?$/.test(number ?? "")) {
      throw new Error(
        `"${raw}" has no macOS bundle version: a prerelease is alpha, beta or rc, optionally ` +
          "followed by .1 to .99 (2.1.0-beta.2)",
      );
    }
    stage = MAC_STAGES.get(kind) + Number(number ?? 0);
  }
  const parts = [core[0], core[1], core[2] * 1000 + stage];
  if (parts.some((part) => part > MAC_MAX_PART)) {
    throw new Error(`"${raw}" is too large for a macOS bundle version`);
  }
  return parts.join(".");
}

/**
 * What set-version.mjs stamps for `text`: the version, and its macOS bundle version.
 *
 * A local Windows or Linux build may carry a build number after the prerelease (2.0.1-beta.1.10,
 * the tenth local build of beta.1). Semver orders it, so the updater takes it, but macOS's three
 * numbers cannot (macBundleVersion). With `local` such a version stamps no macOS bundle version
 * (null: the committed one stays, and the build is not for macOS); without it, as in CI, it is
 * refused, so no macOS bundle ever ships with a bundle version that does not grow.
 */
export function versionStamps(text, { local = false } = {}) {
  const version = parseVersion(text).raw;
  try {
    return { version, macBundleVersion: macBundleVersion(version) };
  } catch (error) {
    if (local) return { version, macBundleVersion: null };
    error.message += "; a local Windows or Linux build takes --local";
    throw error;
  }
}

/**
 * Writes `bundle.macOS.bundleVersion` into a Tauri config's text in place; the rest of the file
 * keeps its formatting. The key must be there already (the committed configs carry the value of
 * the repository's placeholder version).
 */
export function setMacBundleVersion(configText, bundleVersion, name = "the Tauri config") {
  if (!MAC_BUNDLE_VERSION.test(bundleVersion)) {
    throw new Error(`"${bundleVersion}" is not a macOS bundle version`);
  }
  if (typeof JSON.parse(configText).bundle?.macOS?.bundleVersion !== "string") {
    throw new Error(`no bundle.macOS.bundleVersion in ${name}`);
  }
  const key = /("bundleVersion"\s*:\s*)"[^"]*"/g;
  const found = (configText.match(key) ?? []).length;
  if (found !== 1) throw new Error(`expected one "bundleVersion" in ${name}, found ${found}`);
  const next = configText.replace(key, (_, prefix) => `${prefix}"${bundleVersion}"`);
  if (JSON.parse(next).bundle.macOS.bundleVersion !== bundleVersion) {
    throw new Error(`the "bundleVersion" in ${name} is not bundle.macOS.bundleVersion`);
  }
  return next;
}

/** The `<string>` value of a key in an XML Info.plist (the Tauri bundler writes XML). */
export function plistString(xml, key) {
  return new RegExp(`<key>${key}</key>\\s*<string>([^<]*)</string>`).exec(xml)?.[1];
}

// --- Bundles -------------------------------------------------------------------------------

/** GitHub's release asset sanitising, as tauri-action applies it (spaces become dots). */
export function ghAssetName(name) {
  return name
    .trim()
    .replace(/[^a-zA-Z0-9_-]/g, ".")
    .replace(/\.\./g, ".");
}

const ARCH = {
  x64: "x86_64",
  amd64: "x86_64",
  x86_64: "x86_64",
  arm64: "aarch64",
  aarch64: "aarch64",
  x86: "i686",
  i386: "i686",
  i686: "i686",
  armhf: "armv7",
  armhfp: "armv7",
  armv7: "armv7",
  universal: "universal",
};

/**
 * Recognises a Tauri updater bundle by its file name (Tauri CLI names, sanitised or not).
 * Returns `{ os, arch, installer }` in updater terms or null for other files.
 */
export function classifyBundle(fileName) {
  const rules = [
    [/[_.-](x64|x86|arm64)-setup\.exe$/i, "windows", "nsis"],
    [/[_.-](x64|x86|arm64)_[A-Za-z-]+\.msi$/i, "windows", "msi"],
    [/[_.-](universal|aarch64|x64|x86_64)\.app\.tar\.gz$/i, "darwin", "app"],
    [/[_.-](amd64|aarch64|i386|armhf)\.AppImage$/, "linux", "appimage"],
    [/[_.-](amd64|arm64|i386|armhf)\.deb$/, "linux", "deb"],
    [/\.(x86_64|aarch64|i686|armhfp)\.rpm$/, "linux", "rpm"],
  ];
  for (const [re, os, installer] of rules) {
    const m = re.exec(fileName);
    if (m) return { os, arch: ARCH[m[1].toLowerCase()], installer };
  }
  return null;
}

/**
 * Updater platform keys for one bundle. The plugin looks up `{os}-{arch}-{installer}` first and
 * falls back to `{os}-{arch}`, so the generic key goes to the default installer of each OS
 * (NSIS, the .app tarball, AppImage). A universal macOS tarball serves both architectures.
 */
export function platformKeys({ os, arch, installer }) {
  const arches = arch === "universal" ? ["aarch64", "x86_64"] : [arch];
  const primary = { windows: "nsis", darwin: "app", linux: "appimage" }[os] === installer;
  const keys = [];
  for (const a of arches) {
    if (primary) keys.push(`${os}-${a}`);
    keys.push(`${os}-${a}-${installer}`);
  }
  return keys;
}

/**
 * Builds the `platforms` object from `[{ name, signature }]` bundles. A native build wins over
 * a universal one for the same key; two bundles claiming the same key is an error.
 */
export function buildPlatforms(bundles, baseUrl) {
  const platforms = {};
  const owner = {};
  for (const bundle of bundles) {
    const info = classifyBundle(bundle.name);
    if (!info) throw new Error(`not an updater bundle: ${bundle.name}`);
    const universal = info.arch === "universal";
    for (const key of platformKeys(info)) {
      const prev = owner[key];
      if (prev) {
        // A native build replaces the universal fallback; a universal one never replaces native.
        if (universal && !prev.universal) continue;
        if (!(prev.universal && !universal)) {
          throw new Error(`${key} is provided by both ${prev.name} and ${bundle.name}`);
        }
      }
      platforms[key] = { signature: bundle.signature, url: new URL(bundle.name, baseUrl).href };
      owner[key] = { name: bundle.name, universal };
    }
  }
  return platforms;
}

/**
 * Updater bundles in `dir`: every `<bundle>.sig` next to its `<bundle>`. A signature whose
 * bundle is missing, or a bundle Tauri's updater cannot install, is an error.
 */
export function readBundles(dir) {
  const names = new Set(readdirSync(dir));
  const bundles = [];
  for (const sig of [...names].filter((n) => n.endsWith(".sig")).sort()) {
    const name = sig.slice(0, -4);
    if (!names.has(name)) throw new Error(`${sig} has no matching bundle in ${dir}`);
    if (!classifyBundle(name)) throw new Error(`${name} is signed but is not an updater bundle`);
    bundles.push({ name, signature: readFileSync(join(dir, sig), "utf8").trim() });
  }
  if (!bundles.length) throw new Error(`no signed updater bundles (*.sig) in ${dir}`);
  return bundles;
}

/**
 * Tauri's static updater JSON (`latest.json`), plus the channel fields of docs/content-format.md
 * when `channel` is given (`releases/<channel>.json`).
 */
export function buildManifest({ version, notes, pubDate, bundles, baseUrl, channel, ...extra }) {
  const base = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
  const manifest = {
    version: parseVersion(version).raw,
    notes: notes ?? "",
    pub_date: pubDate ?? rfc3339(),
    platforms: buildPlatforms(bundles, base),
  };
  if (channel) {
    const rollout = extra.rollout ?? 100;
    if (!Number.isInteger(rollout) || rollout < 0 || rollout > 100) {
      throw new Error(`rollout must be an integer between 0 and 100, got ${extra.rollout}`);
    }
    Object.assign(manifest, { channel, rollout, mandatory: extra.mandatory ?? false });
    if (extra.minVersion) manifest.minVersion = parseVersion(extra.minVersion).raw;
  }
  return manifest;
}

/** Fails when a required platform key is missing (e.g. a matrix leg produced nothing). */
export function requirePlatforms(manifest, required) {
  const missing = required.filter((key) => !manifest.platforms[key]);
  if (missing.length) throw new Error(`missing updater platforms: ${missing.join(", ")}`);
}

// --- Files and JSON ------------------------------------------------------------------------

export function sha256Hex(data) {
  return createHash("sha256").update(data).digest("hex");
}

export function md5Hex(data) {
  return createHash("md5").update(data).digest("hex");
}

/** Canonical JSON as Studio writes it: sorted keys, 2-space indent, trailing newline. */
export function canonicalJson(value) {
  const sort = (v) => {
    if (Array.isArray(v)) return v.map(sort);
    if (v && typeof v === "object") {
      return Object.fromEntries(
        Object.keys(v)
          .sort()
          .filter((k) => v[k] !== undefined)
          .map((k) => [k, sort(v[k])]),
      );
    }
    return v;
  };
  return `${JSON.stringify(sort(value), null, 2)}\n`;
}

/** RFC 3339 UTC without fractional seconds, e.g. 2026-10-15T12:00:00Z. */
export function rfc3339(date = new Date()) {
  return date.toISOString().replace(/\.\d{3}Z$/, "Z");
}

export function readNotes(path) {
  if (!path) return "";
  return readFileSync(path, "utf8").replace(/\r\n/g, "\n").trim();
}

const CONTENT_TYPES = [
  [/\.json$/, "application/json; charset=utf-8"],
  [/\.sig$/, "text/plain; charset=utf-8"],
  [/\.exe$/, "application/vnd.microsoft.portable-executable"],
  [/\.msi$/, "application/x-msi"],
  [/\.dmg$/, "application/x-apple-diskimage"],
  [/\.(tar\.)?gz$/, "application/gzip"],
  [/\.zip$/, "application/zip"],
  [/\.deb$/, "application/vnd.debian.binary-package"],
  [/\.rpm$/, "application/x-rpm"],
  [/\.apk$/, "application/vnd.android.package-archive"],
];

export function contentType(name) {
  return CONTENT_TYPES.find(([re]) => re.test(name))?.[1] ?? "application/octet-stream";
}

// --- R2 (S3 API, AWS Signature Version 4) ---------------------------------------------------

/** RFC 3986 encoding as SigV4 requires (keeps only unreserved characters). */
function uriEncode(text, keepSlash) {
  let out = "";
  for (const byte of Buffer.from(text, "utf8")) {
    const ch = String.fromCharCode(byte);
    if (/[A-Za-z0-9\-._~]/.test(ch) || (keepSlash && ch === "/")) out += ch;
    else out += `%${byte.toString(16).toUpperCase().padStart(2, "0")}`;
  }
  return out;
}

function hmac(key, data) {
  return createHmac("sha256", key).update(data).digest();
}

/**
 * Signs an S3 request. Pure function (exported for the tests): returns the headers to send,
 * including `authorization`. `headers` keys must be lower case; `host` is required.
 */
export function signRequest({
  method,
  path,
  query = "",
  headers,
  payloadHash,
  accessKeyId,
  secretAccessKey,
  region,
  date,
  service = "s3",
}) {
  const amzDate = date.toISOString().replace(/[:-]|\.\d{3}/g, "");
  const day = amzDate.slice(0, 8);
  const all = { ...headers, "x-amz-content-sha256": payloadHash, "x-amz-date": amzDate };
  const names = Object.keys(all).sort();
  const canonicalHeaders = names
    .map((n) => `${n}:${String(all[n]).trim().replace(/\s+/g, " ")}\n`)
    .join("");
  const signedHeaders = names.join(";");
  const canonicalRequest = [
    method,
    uriEncode(path, true),
    query,
    canonicalHeaders,
    signedHeaders,
    payloadHash,
  ].join("\n");
  const scope = `${day}/${region}/${service}/aws4_request`;
  const stringToSign = ["AWS4-HMAC-SHA256", amzDate, scope, sha256Hex(canonicalRequest)].join("\n");
  const kDate = hmac(`AWS4${secretAccessKey}`, day);
  const kSigning = hmac(hmac(hmac(kDate, region), service), "aws4_request");
  const signature = createHmac("sha256", kSigning).update(stringToSign).digest("hex");
  return {
    ...all,
    authorization: `AWS4-HMAC-SHA256 Credential=${accessKeyId}/${scope}, SignedHeaders=${signedHeaders}, Signature=${signature}`,
  };
}

const EMPTY_SHA256 = sha256Hex("");

export class R2Client {
  /** Reads R2_ACCOUNT_ID, R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY and R2_BUCKET. */
  static fromEnv(env = process.env) {
    const missing = [
      "R2_ACCOUNT_ID",
      "R2_ACCESS_KEY_ID",
      "R2_SECRET_ACCESS_KEY",
      "R2_BUCKET",
    ].filter((k) => !env[k]);
    if (missing.length) return { client: null, missing };
    return {
      client: new R2Client({
        endpoint: env.R2_ENDPOINT || `https://${env.R2_ACCOUNT_ID}.r2.cloudflarestorage.com`,
        bucket: env.R2_BUCKET,
        accessKeyId: env.R2_ACCESS_KEY_ID,
        secretAccessKey: env.R2_SECRET_ACCESS_KEY,
      }),
      missing,
    };
  }

  constructor({ endpoint, bucket, accessKeyId, secretAccessKey, region = "auto" }) {
    this.endpoint = new URL(endpoint);
    this.bucket = bucket;
    this.accessKeyId = accessKeyId;
    this.secretAccessKey = secretAccessKey;
    this.region = region;
  }

  /**
   * `key` null addresses the bucket itself (a listing). `query` is a canonical query string
   * (`canonicalQuery`), signed and sent as it is.
   */
  async request(method, key, { body, headers = {}, query = "" } = {}) {
    const path = key === null ? `/${this.bucket}` : `/${this.bucket}/${key}`;
    const payload = body ?? Buffer.alloc(0);
    const payloadHash = body ? sha256Hex(payload) : EMPTY_SHA256;
    let lastError;
    for (let attempt = 1; attempt <= 4; attempt++) {
      const signed = signRequest({
        method,
        path,
        query,
        // Uncompressed answers: Cloudflare gzips text (.sig, .json) otherwise, and a gzipped HEAD
        // has no Content-Length and only a weak ETag, so the upload check could not compare them.
        headers: { host: this.endpoint.host, "accept-encoding": "identity", ...headers },
        payloadHash,
        accessKeyId: this.accessKeyId,
        secretAccessKey: this.secretAccessKey,
        region: this.region,
        date: new Date(),
      });
      const { host: _host, ...sendHeaders } = signed;
      try {
        const target = uriEncode(path, true) + (query ? `?${query}` : "");
        const res = await fetch(new URL(target, this.endpoint), {
          method,
          headers: sendHeaders,
          body: method === "PUT" ? payload : undefined,
          signal: AbortSignal.timeout(10 * 60 * 1000),
        });
        if (res.status >= 500 || res.status === 429) {
          lastError = new Error(`${method} ${key}: HTTP ${res.status}`);
        } else {
          return res;
        }
      } catch (err) {
        lastError = new Error(`${method} ${key}: ${err.message}`);
      }
      await new Promise((r) => setTimeout(r, attempt * 2000));
    }
    throw lastError;
  }

  /** `{ size, etag, sha256 }` or null when the object does not exist. */
  async head(key) {
    const res = await this.request("HEAD", key);
    if (res.status === 404) return null;
    if (!res.ok) throw new Error(`HEAD ${key}: HTTP ${res.status}`);
    return {
      size: Number(res.headers.get("content-length")),
      etag: (res.headers.get("etag") || "").replace(/"/g, ""),
      sha256: res.headers.get("x-amz-meta-sha256") || null,
    };
  }

  /** Object bytes or null when it does not exist. */
  async get(key) {
    const res = await this.request("GET", key);
    if (res.status === 404) return null;
    if (!res.ok) throw new Error(`GET ${key}: HTTP ${res.status} ${await res.text()}`);
    return Buffer.from(await res.arrayBuffer());
  }

  async put(key, body, { contentType, cacheControl, sha256 }) {
    const headers = {
      "content-type": contentType,
      "cache-control": cacheControl,
      "x-amz-meta-sha256": sha256,
    };
    const res = await this.request("PUT", key, { body, headers });
    if (!res.ok) throw new Error(`PUT ${key}: HTTP ${res.status} ${await res.text()}`);
  }

  /**
   * Every object under `prefix` (ListObjectsV2, all pages): `[{ key, size, lastModified }]` with
   * `lastModified` in milliseconds. Throws when any page fails or cannot be read, so a caller never
   * acts on a partial listing.
   */
  async list(prefix) {
    const objects = [];
    let token = null;
    for (let page = 0; page < 1000; page++) {
      const params = { "list-type": "2", prefix };
      if (token) params["continuation-token"] = token;
      const res = await this.request("GET", null, { query: canonicalQuery(params) });
      const text = await res.text();
      if (!res.ok) throw new Error(`LIST ${prefix}: HTTP ${res.status} ${text}`);
      const listing = parseListing(text);
      objects.push(...listing.objects);
      if (!listing.truncated) return objects;
      if (!listing.next)
        throw new Error(`LIST ${prefix}: a truncated page without a continuation token`);
      token = listing.next;
    }
    throw new Error(`LIST ${prefix}: too many pages`);
  }

  async delete(key) {
    const res = await this.request("DELETE", key);
    if (!res.ok && res.status !== 404)
      throw new Error(`DELETE ${key}: HTTP ${res.status} ${await res.text()}`);
  }
}

/** SigV4's canonical query string: names sorted, names and values RFC 3986 encoded. */
export function canonicalQuery(params) {
  return Object.keys(params)
    .sort()
    .map((name) => `${uriEncode(name, false)}=${uriEncode(String(params[name]), false)}`)
    .join("&");
}

function xmlText(value) {
  return value
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&#(\d+);/g, (_, code) => String.fromCodePoint(Number(code)))
    .replace(/&amp;/g, "&");
}

/** One ListObjectsV2 page: its objects, whether more follow and the token for them. */
export function parseListing(xml) {
  if (!/<ListBucketResult[\s>]/.test(xml)) throw new Error("not a ListObjectsV2 answer");
  const objects = [];
  for (const [, body] of xml.matchAll(/<Contents>([\s\S]*?)<\/Contents>/g)) {
    const key = /<Key>([\s\S]*?)<\/Key>/.exec(body)?.[1];
    const size = /<Size>(\d+)<\/Size>/.exec(body)?.[1];
    const modified = /<LastModified>([^<]+)<\/LastModified>/.exec(body)?.[1];
    const lastModified = modified ? Date.parse(modified) : Number.NaN;
    if (key === undefined || size === undefined || Number.isNaN(lastModified)) {
      throw new Error("a listing entry without a key, a size or a date");
    }
    objects.push({ key: xmlText(key), size: Number(size), lastModified });
  }
  const truncated = /<IsTruncated>true<\/IsTruncated>/.test(xml);
  const next = /<NextContinuationToken>([^<]+)<\/NextContinuationToken>/.exec(xml)?.[1];
  return { objects, truncated, next: next ? xmlText(next) : null };
}
