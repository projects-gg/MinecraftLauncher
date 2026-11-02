// Tests of "Betayı kararlıya al": the rules in lib.mjs and stable-from-beta.mjs against an
// in-memory CDN and GitHub API. Run: `node --test "scripts/release/*.test.mjs"`.

import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import {
  commandData,
  commandProperty,
  Refusal,
  stableFromBetaRefusal,
  stableOf,
  tagRefusal,
} from "./lib.mjs";

const execute = promisify(execFile);
const script = fileURLToPath(new URL("stable-from-beta.mjs", import.meta.url));
const SHA = "a".repeat(40);
const OTHER = "b".repeat(40);
const BETA = "v2.1.0-beta.2";

// What a run started on BETA asks for while the channels announce that beta and an older stable.
const fine = {
  beta: BETA,
  ref: `refs/tags/${BETA}`,
  version: "2.1.0",
  channel: "beta",
  rollout: "100",
  betaChannel: { version: "2.1.0-beta.2" },
  stableChannel: { version: "2.0.3" },
};

test("stableOf gives a prerelease's own release", () => {
  assert.equal(stableOf("2.1.0-beta.2"), "2.1.0");
  assert.equal(stableOf("v2.0.10-rc.1"), "2.0.10");
  assert.equal(stableOf("2.1.0"), null);
});

test("stableFromBetaRefusal lets the announced beta through", () => {
  assert.equal(stableFromBetaRefusal(fine), null);
  assert.equal(stableFromBetaRefusal({ ...fine, stableChannel: null }), null);
  assert.equal(stableFromBetaRefusal({ ...fine, rollout: "0" }), null);
  assert.equal(stableFromBetaRefusal({ ...fine, rollout: " 25 " }), null);
});

test("stableFromBetaRefusal names what stands in the way", () => {
  const code = (change) => stableFromBetaRefusal({ ...fine, ...change })?.code;
  assert.equal(code({ beta: "v2.1.0" }), "release/not-prerelease");
  assert.equal(code({ beta: "2.1.0-beta.2" }), "release/not-prerelease");
  assert.equal(code({ beta: "vnext" }), "release/not-prerelease");
  assert.equal(code({ beta: undefined }), "release/not-prerelease");
  assert.equal(code({ ref: "refs/heads/v2" }), "release/wrong-ref");
  assert.equal(code({ ref: "" }), "release/wrong-ref");
  assert.equal(code({ version: "2.1.1" }), "release/wrong-version");
  assert.equal(code({ version: "2.1.0-beta.2" }), "release/wrong-version");
  assert.equal(code({ channel: "none" }), "release/bad-channel");
  for (const rollout of ["", " ", "101", "-1", "1e2", "50.5", undefined]) {
    assert.equal(code({ rollout }), "release/bad-rollout", String(rollout));
  }
  assert.equal(code({ betaChannel: { version: "2.1.0-beta.3" } }), "release/channel-moved");
  assert.equal(code({ betaChannel: { version: "2.1.0" } }), "release/channel-moved");
  assert.equal(code({ betaChannel: null }), "release/channel-moved");
  assert.equal(code({ stableChannel: { version: "2.1.0" } }), "release/stable-ahead");
  assert.equal(code({ stableChannel: { version: "2.2.0-beta.1" } }), "release/stable-ahead");
  assert.ok(stableFromBetaRefusal({ ...fine, ref: "refs/heads/v2" }) instanceof Refusal);
});

test("tagRefusal refuses a stable tag on another commit only", () => {
  assert.equal(tagRefusal({ tag: "v2.1.0", commit: null, sha: SHA }), null);
  assert.equal(tagRefusal({ tag: "v2.1.0", commit: SHA, sha: SHA }), null);
  assert.equal(tagRefusal({ tag: "v2.1.0", commit: OTHER, sha: SHA })?.code, "release/tag-taken");
});

test("workflow commands escape what GitHub would read as syntax", () => {
  assert.equal(commandData("50%\r\nnext"), "50%25%0D%0Anext");
  assert.equal(commandProperty("release/a:b,c"), "release/a%3Ab%2Cc");
});

// --- the command against an in-memory CDN and GitHub -----------------------------------------

const state = {
  beta: { version: "2.1.0-beta.2" },
  stable: { version: "2.0.3" },
  failing: 0,
  cached: 0,
  refs: new Map(),
  tags: new Map(),
};
let server;
let env;
let dir;

function startServer() {
  return new Promise((resolve) => {
    server = createServer((req, res) => {
      const chunks = [];
      req.on("data", (chunk) => chunks.push(chunk));
      req.on("end", () => {
        const url = new URL(req.url, "http://localhost");
        const send = (status, body) => {
          res.writeHead(status, { "content-type": "application/json" });
          res.end(JSON.stringify(body ?? {}));
        };
        const channel = /^\/v1\/releases\/(beta|stable)\.json$/.exec(url.pathname);
        if (channel) {
          if (!url.searchParams.get("fresh")) state.cached++;
          if (state.failing > 0) {
            state.failing--;
            return send(503);
          }
          const doc = state[channel[1]];
          return doc ? send(200, doc) : send(404);
        }
        if (req.headers.authorization !== "Bearer test-token") return send(401);
        const body = chunks.length ? JSON.parse(Buffer.concat(chunks).toString("utf8")) : null;
        const ref = /^\/repos\/o\/r\/git\/ref\/tags\/(.+)$/.exec(url.pathname);
        if (ref && req.method === "GET") {
          const target = state.refs.get(decodeURIComponent(ref[1]));
          return target ? send(200, { object: target }) : send(404);
        }
        const tagObject = /^\/repos\/o\/r\/git\/tags\/([0-9a-f]+)$/.exec(url.pathname);
        if (tagObject && req.method === "GET") {
          const made = state.tags.get(tagObject[1]);
          return made ? send(200, { object: { type: made.type, sha: made.object } }) : send(404);
        }
        if (url.pathname === "/repos/o/r/git/tags" && req.method === "POST") {
          const sha = createHash("sha1").update(JSON.stringify(body)).digest("hex");
          state.tags.set(sha, body);
          return send(201, { sha });
        }
        if (url.pathname === "/repos/o/r/git/refs" && req.method === "POST") {
          const name = body.ref.replace(/^refs\/tags\//, "");
          if (state.refs.has(name)) return send(422, { message: "Reference already exists" });
          state.refs.set(name, { type: "tag", sha: body.sha });
          return send(201);
        }
        return send(404);
      });
    });
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
}

function cli(args, extra = {}) {
  return execute(process.execPath, [script, ...args], { env: { ...env, ...extra } });
}

const CHECK = ["check", "--beta", BETA, "--version", "2.1.0"];

before(async () => {
  const port = await startServer();
  const base = `http://127.0.0.1:${port}`;
  env = {
    ...process.env,
    CDN_BASE_URL: base,
    GITHUB_API_URL: base,
    GITHUB_REPOSITORY: "o/r",
    GITHUB_SHA: SHA,
    GITHUB_REF: `refs/tags/${BETA}`,
    GH_TOKEN: "test-token",
    GITHUB_ACTIONS: "true",
    RELEASE_RETRY_MS: "1",
  };
  dir = mkdtempSync(join(tmpdir(), "pl-stable-"));
});

after(() => {
  server.close();
  rmSync(dir, { recursive: true, force: true });
});

test("check passes the announced beta, read past the CDN's cache", async () => {
  state.failing = 1;
  const { stdout } = await cli(CHECK);
  assert.match(
    stdout,
    /^v2\.1\.0-beta\.2 \(a{40}\) becomes 2\.1\.0: the beta channel announces 2\.1\.0-beta\.2, the stable one 2\.0\.3$/m,
  );
  assert.equal(state.failing, 0, "the CDN's 503 was tried again");
  assert.equal(state.cached, 0, "every read went past the cache");

  // The first stable: no stable channel yet.
  state.stable = null;
  assert.match((await cli(CHECK)).stdout, /the stable one nothing/);
  state.stable = { version: "2.0.3" };
});

test("check refuses with a named annotation", async () => {
  const refused = (code) => (error) =>
    error.code === 1 && new RegExp(`^::error title=release/${code}::`, "m").test(error.stdout);

  state.beta = { version: "2.1.0-beta.3" };
  await assert.rejects(cli(CHECK), (error) => {
    assert.match(
      error.stdout,
      /^::error title=release\/channel-moved::the beta channel announces 2\.1\.0-beta\.3, not 2\.1\.0-beta\.2$/m,
    );
    return true;
  });
  state.beta = { version: "2.1.0-beta.2" };

  await assert.rejects(cli(CHECK, { GITHUB_REF: "refs/heads/v2" }), refused("wrong-ref"));
  await assert.rejects(
    cli(["check", "--beta", BETA, "--version", "2.1.1"]),
    refused("wrong-version"),
  );
  await assert.rejects(cli([...CHECK, "--rollout", "150"]), refused("bad-rollout"));

  state.stable = { version: "2.1.0" };
  await assert.rejects(cli(CHECK), refused("stable-ahead"));
  state.stable = { version: "2.0.3" };

  // The stable tag on another commit stops it; on this one (an earlier run's) it does not.
  state.refs.set("v2.1.0", { type: "commit", sha: OTHER });
  await assert.rejects(cli(CHECK), refused("tag-taken"));
  state.refs.set("v2.1.0", { type: "commit", sha: SHA });
  await cli(CHECK);
  state.refs.delete("v2.1.0");

  // Channels the CDN does not give (three tries each) stop it as well.
  state.failing = 6;
  await assert.rejects(cli(CHECK), refused("unreadable"));
  state.failing = 0;

  // Outside Actions there is no annotation, only the error line.
  await assert.rejects(
    cli(CHECK, { GITHUB_REF: "refs/heads/v2", GITHUB_ACTIONS: "" }),
    (error) => !error.stdout.includes("::error") && /error: start this run on/.test(error.stderr),
  );
});

test("tag makes the annotated stable tag once, on this commit only", async () => {
  const notes = join(dir, "notes.md");
  writeFileSync(notes, "Yenilikler\r\n\r\n- Daha hızlı\r\n\r\n--- en ---\r\n\r\nWhat's new\r\n");

  const dry = await cli(["tag", "--tag", "v2.1.0", "--notes-file", notes, "--dry-run"]);
  assert.match(dry.stdout, /\[dry-run\] would tag v2\.1\.0 -> a{40}/);
  assert.equal(state.refs.size, 0);

  const made = await cli(["tag", "--tag", "v2.1.0", "--notes-file", notes]);
  assert.match(made.stdout, /^tagged v2\.1\.0 -> a{40}$/m);
  const ref = state.refs.get("v2.1.0");
  assert.equal(ref.type, "tag");
  assert.deepEqual(state.tags.get(ref.sha), {
    tag: "v2.1.0",
    message: "Yenilikler\n\n- Daha hızlı\n\n--- en ---\n\nWhat's new\n",
    object: SHA,
    type: "commit",
  });

  // Again (a re-run of the job): left as it is.
  const again = await cli(["tag", "--tag", "2.1.0", "--notes-file", notes]);
  assert.match(again.stdout, /already names a{40}; left as it is/);
  assert.equal(state.tags.size, 1);

  // From another commit: refused, the tag stays.
  await assert.rejects(
    cli(["tag", "--tag", "v2.1.0", "--notes-file", notes], { GITHUB_SHA: OTHER }),
    (error) =>
      /^::error title=release\/tag-taken::v2\.1\.0 already names a{40}/m.test(error.stdout),
  );
  assert.equal(state.refs.get("v2.1.0"), ref);

  // A stable tag always carries notes.
  const empty = join(dir, "empty.md");
  writeFileSync(empty, "\n\n");
  await assert.rejects(cli(["tag", "--tag", "v2.2.0", "--notes-file", empty]), (error) =>
    /title=release\/notes/.test(error.stdout),
  );
  assert.equal(state.refs.has("v2.2.0"), false);
});

test("tag takes a tag made meanwhile on this commit", async () => {
  // The ref appears between the read and the write (a retried request): fine on this commit.
  const notes = join(dir, "race.md");
  writeFileSync(notes, "Yenilikler\n");
  state.refs.clear();
  state.tags.clear();
  const original = server.listeners("request")[0];
  server.removeAllListeners("request");
  server.on("request", (req, res) => {
    if (req.method === "POST" && req.url === "/repos/o/r/git/refs") {
      state.refs.set("v2.3.0", { type: "commit", sha: SHA });
    }
    original(req, res);
  });
  try {
    const { stdout } = await cli(["tag", "--tag", "v2.3.0", "--notes-file", notes]);
    assert.match(stdout, /^tagged v2\.3\.0 -> a{40}$/m);
    assert.deepEqual(state.refs.get("v2.3.0"), { type: "commit", sha: SHA });
  } finally {
    server.removeAllListeners("request");
    server.on("request", original);
  }
});
