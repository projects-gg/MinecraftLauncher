#!/usr/bin/env node
// Checks a release's notes and writes the GitHub release's text from them. The notes come as the
// release tag's message gives them: Turkish, then optionally a `--- en ---` line and the English
// notes (lib.mjs `parseNotes`, docs/release.md). The release workflow runs this before anything
// builds, so malformed notes stop the release there. `--require tr` also stops it when the Turkish
// notes are missing (a stable taken from a beta always tells players what changed).
//
//   node scripts/release/notes.mjs <notes file> [--body <out.md>] [--require tr[,en]]

import { writeFileSync } from "node:fs";

import { parseArgs, Refusal, readNotes, releaseBody, run, UsageError } from "./lib.mjs";

const USAGE =
  "usage: node scripts/release/notes.mjs <notes file> [--body <out.md>] [--require tr[,en]]";
const LANGUAGES = ["tr", "en"];

run(async () => {
  const { flags, positional } = parseArgs(process.argv.slice(2), {
    body: "string",
    require: "string",
  });
  if (positional.length !== 1) throw new UsageError("expected one notes file");
  const required = (flags.require ?? "").split(",").filter(Boolean);
  for (const lang of required) {
    if (!LANGUAGES.includes(lang)) throw new UsageError(`--require: unknown language "${lang}"`);
  }
  let notes;
  try {
    notes = readNotes(positional[0]);
  } catch (err) {
    throw new Refusal("release/notes", err.message);
  }
  const sizes = Object.entries(notes)
    .filter(([, text]) => text)
    .map(([lang, text]) => `${lang} ${[...text].length} characters`);
  console.log(`notes: ${sizes.join(", ") || "none"}`);
  const missing = required.filter((lang) => !notes[lang]);
  if (missing.length) {
    throw new Refusal(
      "release/notes",
      `release notes: the ${missing.join(" and ")} notes are missing`,
    );
  }
  if (notes.tr && !notes.en) {
    console.log("note: no English notes; the English interface shows the Turkish ones");
  }
  if (flags.body) writeFileSync(flags.body, releaseBody(notes));
}, USAGE);
