// Tests of notes.mjs, the release workflow's check of a release's notes (lib.mjs `parseNotes`).
// Run: `node --test "scripts/release/*.test.mjs"`.

import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const run = promisify(execFile);
const script = fileURLToPath(new URL("notes.mjs", import.meta.url));
const dir = mkdtempSync(join(tmpdir(), "pl-notes-cli-"));

after(() => rmSync(dir, { recursive: true, force: true }));

function notesFile(name, text) {
  const path = join(dir, name);
  writeFileSync(path, text);
  return path;
}

test("notes.mjs checks the notes and writes the GitHub release text", async () => {
  const body = join(dir, "body.md");
  const both = notesFile("both.md", "Yeni ekran\n\n--- en ---\n\nNew screen\n");
  const { stdout } = await run(process.execPath, [script, both, "--body", body]);
  assert.equal(stdout, "notes: tr 10 characters, en 10 characters\n");
  assert.equal(readFileSync(body, "utf8"), "Yeni ekran\n\n---\n\nNew screen\n");

  const turkish = await run(process.execPath, [script, notesFile("tr.md", "Yeni ekran\n")]);
  assert.match(turkish.stdout, /^notes: tr 10 characters\nnote: no English notes/);

  const none = await run(process.execPath, [script, notesFile("none.md", ""), "--body", body]);
  assert.equal(none.stdout, "notes: none\n");
  assert.equal(readFileSync(body, "utf8"), "");
});

test("notes.mjs stops on malformed notes", async () => {
  const mistyped = notesFile("bad.md", "Yeni\n--- English ---\nNew\n");
  await assert.rejects(
    run(process.execPath, [script, mistyped]),
    (error) => error.code === 1 && /is not a language line/.test(error.stderr),
  );
  await assert.rejects(run(process.execPath, [script]), (error) => error.code === 2);
});
