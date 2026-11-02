#!/usr/bin/env node
// "Betayı kararlıya al" (docs/release.md, "Studio'dan kararlıya alma"): a run of release.yml
// started on a beta's tag builds that beta's commit as the beta's own stable version, publishes it
// to the beta channel and then to the stable one, and makes it GitHub's latest release.
//
//   check  In Prepare, before anything builds (lib.mjs `stableFromBetaRefusal`): the run started on
//          the beta's tag, the version is the beta's stable one, the beta channel still announces
//          that beta, the stable channel an older version, the rollout is 0-100, the CDN has no
//          files of that version yet, and the stable tag is free or already on this commit (an
//          earlier run of the same promotion).
//   tag    At the end: the annotated stable tag on the beta's commit, the notes as its message.
//          Nothing when an earlier run made it; refused when the tag names another commit.
//
//   node scripts/release/stable-from-beta.mjs check --beta v2.1.0-beta.2 --version 2.1.0 \
//     [--channel beta] [--rollout 100]
//   node scripts/release/stable-from-beta.mjs tag --tag v2.1.0 --notes-file notes.md [--dry-run]
//
// Environment, as GitHub Actions sets it: GITHUB_REF, GITHUB_SHA, GITHUB_REPOSITORY, GH_TOKEN (or
// GITHUB_TOKEN), GITHUB_API_URL (default https://api.github.com). The channel files are read from
// CDN_BASE_URL (default https://cdn.projects.gg) past the CDN's cache, as players will read them.
// Refusals carry a name (lib.mjs `Refusal`) that Studio reads back from the run's annotations.

import { readFileSync } from "node:fs";

import {
  parseArgs,
  parseVersion,
  Refusal,
  releaseBundles,
  run,
  stableFromBetaRefusal,
  tagRefusal,
  UsageError,
} from "./lib.mjs";

const USAGE = `usage: node scripts/release/stable-from-beta.mjs <check|tag> [options]
  check --beta <tag> --version <v> [--channel beta] [--rollout 0-100]
  tag   --tag <tag> --notes-file <md> [--dry-run]`;

const CDN = (process.env.CDN_BASE_URL || "https://cdn.projects.gg").replace(/\/+$/, "");
const PREFIX = (process.env.R2_PREFIX || "v1").replace(/^\/+|\/+$/g, "");
const API = (process.env.GITHUB_API_URL || "https://api.github.com").replace(/\/+$/, "");
// Three tries, two and four seconds apart (the tests shorten the wait).
const ATTEMPTS = 3;
const RETRY_MS = Number(process.env.RELEASE_RETRY_MS ?? 2000);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** `fetch`, tried again after a network error or a 5xx/429, waiting twice as long each time. */
async function fetchRetrying(url, init) {
  for (let attempt = 1; ; attempt++) {
    let response = null;
    let error = null;
    try {
      response = await fetch(url, init);
    } catch (err) {
      error = err;
    }
    const transient = error !== null || response.status >= 500 || response.status === 429;
    if (!transient || attempt === ATTEMPTS) {
      if (error) throw error;
      return response;
    }
    await sleep(RETRY_MS * 2 ** (attempt - 1));
  }
}

function env(name) {
  const value = process.env[name];
  if (!value) throw new UsageError(`${name} is not set`);
  return value;
}

/** A channel file as players read it now (null: there is none), past the CDN's cache. */
async function publicChannel(name) {
  const url = `${CDN}/${PREFIX}/releases/${name}.json?fresh=${Date.now()}`;
  let response;
  try {
    response = await fetchRetrying(url, { headers: { accept: "application/json" } });
  } catch (err) {
    throw new Refusal("release/unreadable", `${name}.json could not be read: ${err.message}`);
  }
  if (response.status === 404) return null;
  if (!response.ok) {
    throw new Refusal(
      "release/unreadable",
      `${name}.json could not be read: HTTP ${response.status}`,
    );
  }
  const doc = await response.json().catch(() => null);
  try {
    parseVersion(doc?.version);
  } catch {
    throw new Refusal("release/unreadable", `${name}.json names no valid version`);
  }
  return doc;
}

function github(path, { method = "GET", body } = {}) {
  const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
  if (!token) throw new UsageError("GH_TOKEN is not set");
  return fetchRetrying(`${API}${path}`, {
    method,
    headers: {
      accept: "application/vnd.github+json",
      authorization: `Bearer ${token}`,
      "x-github-api-version": "2022-11-28",
      ...(body ? { "content-type": "application/json" } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
}

async function githubJson(path, init) {
  const response = await github(path, init);
  if (!response.ok) {
    throw new Error(
      `${init?.method ?? "GET"} ${path}: HTTP ${response.status} ${await response.text()}`,
    );
  }
  return response.json();
}

/**
 * Whether the CDN already serves any of `version`'s bundles: an earlier run uploaded (some of)
 * the release and stopped before its channel file. A new build makes other bytes, which R2's
 * immutable files refuse an hour later, so the way on is that run again ("Re-run failed jobs").
 * Every bundle is asked for, not one: an upload that stopped half way left the first few only.
 */
async function filesThere(version) {
  const answers = await Promise.all(
    releaseBundles(version).map(async (name) => {
      const url = `${CDN}/${PREFIX}/releases/${version}/${name}`;
      let response;
      try {
        response = await fetchRetrying(`${url}?fresh=${Date.now()}`, { method: "HEAD" });
      } catch (err) {
        throw new Refusal("release/unreadable", `${url} could not be read: ${err.message}`);
      }
      if (response.status === 404 || response.status === 403) return false;
      if (!response.ok) {
        throw new Refusal(
          "release/unreadable",
          `${url} could not be read: HTTP ${response.status}`,
        );
      }
      return true;
    }),
  );
  return answers.some(Boolean);
}

/** The commit `tag` names (an annotated tag followed to it), or null when there is no such tag. */
async function tagCommit(repo, tag) {
  const response = await github(`/repos/${repo}/git/ref/tags/${encodeURIComponent(tag)}`);
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`reading ${tag}: HTTP ${response.status}`);
  let object = (await response.json()).object;
  for (let depth = 0; object?.type === "tag" && depth < 5; depth++) {
    object = (await githubJson(`/repos/${repo}/git/tags/${object.sha}`)).object;
  }
  if (object?.type !== "commit") {
    throw new Error(`${tag} names a ${object?.type ?? "missing"} object, not a commit`);
  }
  return object.sha;
}

async function check(flags) {
  if (!flags.beta || !flags.version) throw new UsageError("check needs --beta and --version");
  const version = parseVersion(flags.version).raw;
  const [betaChannel, stableChannel] = await Promise.all([
    publicChannel("beta"),
    publicChannel("stable"),
  ]);
  const refusal = stableFromBetaRefusal({
    beta: flags.beta,
    ref: process.env.GITHUB_REF ?? "",
    version,
    channel: flags.channel ?? "beta",
    rollout: flags.rollout ?? "100",
    betaChannel,
    stableChannel,
  });
  if (refusal) throw refusal;
  if (await filesThere(version)) {
    throw new Refusal(
      "release/files-exist",
      `the CDN already has ${version}'s files: re-run the run that uploaded them instead`,
    );
  }
  const repo = env("GITHUB_REPOSITORY");
  const sha = env("GITHUB_SHA");
  const tag = `v${version}`;
  const taken = tagRefusal({ tag, commit: await tagCommit(repo, tag), sha });
  if (taken) throw taken;
  console.log(
    `${flags.beta} (${sha}) becomes ${version}: the beta channel announces ${betaChannel.version}, ` +
      `the stable one ${stableChannel?.version ?? "nothing"}`,
  );
}

async function tag(flags) {
  if (!flags.tag || !flags["notes-file"]) throw new UsageError("tag needs --tag and --notes-file");
  const name = `v${parseVersion(flags.tag).raw}`;
  const repo = env("GITHUB_REPOSITORY");
  const sha = env("GITHUB_SHA");
  const notes = readFileSync(flags["notes-file"], "utf8").replace(/\r\n?/g, "\n").trim();
  if (!notes)
    throw new Refusal("release/notes", "a stable tag carries the release notes; none given");
  const message = `${notes}\n`;
  const existing = await tagCommit(repo, name);
  const taken = tagRefusal({ tag: name, commit: existing, sha });
  if (taken) throw taken;
  if (existing) {
    console.log(`${name} already names ${sha}; left as it is`);
    return;
  }
  if (flags["dry-run"]) {
    console.log(`[dry-run] would tag ${name} -> ${sha}:\n${message}`);
    return;
  }
  const object = await githubJson(`/repos/${repo}/git/tags`, {
    method: "POST",
    body: { tag: name, message, object: sha, type: "commit" },
  });
  const ref = await github(`/repos/${repo}/git/refs`, {
    method: "POST",
    body: { ref: `refs/tags/${name}`, sha: object.sha },
  });
  if (!ref.ok) {
    const detail = `creating ${name}: HTTP ${ref.status} ${await ref.text()}`;
    // 422: the tag came meanwhile (a retried request, or someone else): fine on this commit only.
    const now = ref.status === 422 ? await tagCommit(repo, name) : null;
    const raced = tagRefusal({ tag: name, commit: now, sha });
    if (raced) throw raced;
    if (!now) throw new Error(detail);
  }
  console.log(`tagged ${name} -> ${sha}`);
}

run(async () => {
  const [command, ...rest] = process.argv.slice(2);
  const { flags, positional } = parseArgs(rest, {
    beta: "string",
    version: "string",
    channel: "string",
    rollout: "string",
    tag: "string",
    "notes-file": "string",
    "dry-run": "boolean",
  });
  if (positional.length) throw new UsageError(`unexpected argument ${positional[0]}`);
  const commands = { check, tag };
  if (!Object.hasOwn(commands, command ?? "")) throw new UsageError("expected a command");
  await commands[command](flags);
}, USAGE);
