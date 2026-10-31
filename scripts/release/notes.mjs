#!/usr/bin/env node
// Checks a release's notes and writes the GitHub release's text from them. The notes come as the
// release tag's message gives them: Turkish, then optionally a `--- en ---` line and the English
// notes (lib.mjs `parseNotes`, docs/release.md). The release workflow runs this before anything
// builds, so malformed notes stop the release there.
//
//   node scripts/release/notes.mjs <notes file> [--body <out.md>]

import { writeFileSync } from "node:fs";

import { parseArgs, readNotes, releaseBody, run, UsageError } from "./lib.mjs";

const USAGE = "usage: node scripts/release/notes.mjs <notes file> [--body <out.md>]";

run(async () => {
  const { flags, positional } = parseArgs(process.argv.slice(2), { body: "string" });
  if (positional.length !== 1) throw new UsageError("expected one notes file");
  const notes = readNotes(positional[0]);
  const sizes = Object.entries(notes)
    .filter(([, text]) => text)
    .map(([lang, text]) => `${lang} ${[...text].length} characters`);
  console.log(`notes: ${sizes.join(", ") || "none"}`);
  if (notes.tr && !notes.en) {
    console.log("note: no English notes; the English interface shows the Turkish ones");
  }
  if (flags.body) writeFileSync(flags.body, releaseBody(notes));
}, USAGE);
