// The launcher's Windows installer (INSTALL-UX): its NSIS template is Tauri's for the pinned CLI
// with our changes, it never deletes the player's data, and both languages name every string it
// shows. Run: `node --test "scripts/release/*.test.mjs"`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const app = join(root, "apps/launcher");
const read = (path) => readFileSync(join(app, "src-tauri", path), "utf8");
const config = JSON.parse(read("tauri.conf.json"));
const windows = config.bundle.windows;
const template = read(windows.nsis.template);

// Strings Tauri's helpers (utils.nsh, CheckIfAppIsRunning) show without the template naming them.
const HELPER_STRINGS = ["appRunning", "appRunningOkKill", "failedToKillApp"];

/** `LangString <name> ${LANG_<language>} "..."` lines of a language file: name → text. */
function langStrings(file, language) {
  const strings = new Map();
  for (const line of read(file).split("\n")) {
    const match = /^LangString\s+(\w+)\s+\$\{LANG_(\w+)\}\s+"(.*)"\s*$/.exec(line);
    if (!match) continue;
    assert.equal(match[2], language, `${file}: ${match[1]} is not for ${language}`);
    strings.set(match[1], match[3]);
  }
  return strings;
}

/** Our and Tauri's strings, without the Modern UI page texts a language file may override. */
const own = (strings) => new Set([...strings.keys()].filter((name) => !name.startsWith("MUI_")));

test("the installer template is Tauri's for the pinned CLI", () => {
  const pkg = JSON.parse(readFileSync(join(app, "package.json"), "utf8"));
  const cli = pkg.devDependencies["@tauri-apps/cli"];
  assert.match(cli, /^\d+\.\d+\.\d+$/, "the Tauri CLI is pinned to one version");
  const based = /tauri-cli (\d+\.\d+\.\d+)/.exec(template)?.[1];
  assert.equal(
    based,
    cli,
    "the Tauri CLI changed: take its new installer.nsi and apply the PROJECTS (INSTALL-UX) changes again",
  );
  assert.equal(windows.nsis.template, "nsis/installer.nsi");
});

test("the installer never deletes the player's data or installs over a newer launcher", () => {
  assert.doesNotMatch(template, /deleteAppData|DeleteAppData/);
  assert.doesNotMatch(template, /RmDir \/r "\$(LOCAL)?APPDATA/);
  assert.equal(windows.allowDowngrades, false);
  // The graphical installer asks nothing on an update and offers to open what is installed.
  assert.match(template, /Call ProjectsDetectInstall/);
  assert.match(template, /Call ProjectsInstalledPage/);
  assert.match(template, /!define MUI_PAGE_CUSTOMFUNCTION_PRE ProjectsSkipWelcome/);
  assert.match(template, /!define MUI_PAGE_CUSTOMFUNCTION_PRE ProjectsSkipDirectory/);
});

test("both languages name every string the installer shows, and no other", () => {
  assert.deepEqual(windows.nsis.languages, ["English", "Turkish"]);
  const files = windows.nsis.customLanguageFiles;
  const english = langStrings(files.English, "ENGLISH");
  const turkish = langStrings(files.Turkish, "TURKISH");

  const used = new Set(HELPER_STRINGS);
  for (const match of template.matchAll(/\$\((\w+)\)/g)) {
    if (!match[1].startsWith("MUI_")) used.add(match[1]);
  }
  for (const name of used) {
    assert.ok(english.has(name), `English.nsh lacks ${name}`);
    assert.ok(turkish.has(name), `Turkish.nsh lacks ${name}`);
  }
  assert.deepEqual(
    [...own(english)].sort(),
    [...used].sort(),
    "English.nsh has strings nothing shows",
  );
  assert.deepEqual(
    [...own(turkish)].sort(),
    [...used].sort(),
    "Turkish.nsh has strings nothing shows",
  );
  for (const [name, text] of [...english, ...turkish]) {
    assert.ok(text.trim().length > 0, `${name} is empty`);
  }
  // Our sentences follow the launcher's voice (locales/voice.test.ts): one idea per sentence.
  for (const [name, text] of [...english, ...turkish]) {
    if (name.startsWith("projects") || name === "MUI_UNTEXT_CONFIRM_SUBTITLE") {
      assert.doesNotMatch(text, /;|—/, `${name} joins two sentences`);
    }
  }
});
