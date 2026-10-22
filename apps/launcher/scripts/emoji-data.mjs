#!/usr/bin/env node
// Emoji data for the launcher's picker and its embedded font (CHAT2 §3), from pinned sources whose
// SHA-256 is checked: Unicode Emoji 17.0 `emoji-test.txt` and the CLDR 48.2.0 annotations for
// Turkish and English (names and keywords, with the derived ones for sequences such as flags).
//
// Writes:
//   src/features/emoji/data/emoji.tr.tsv  the picker's list with Turkish names and search words,
//                                         and the English name and words as the fallback
//   src/features/emoji/data/emoji.en.tsv  the same with English names and search words
//   src/assets/fonts/emoji/emoji.css      the font's @font-face slices (unicode-range)
//   <cache>/font-plan.json                which code points each slice holds, for emoji-font.py
// A TSV's first line is its header (format, language, sources, the code points a skin tone
// follows); then one emoji per line: emoji, group (0–8), tones (empty, "1" by rule, or the five
// written out), name, search words (folded, those a longer word already finds dropped) and, in
// Turkish, the English name and words. The Android app can copy the files as they are.
//
// Usage (from the repository root): node apps/launcher/scripts/emoji-data.mjs [--cache <dir>]
// then: python apps/launcher/scripts/emoji-font.py --plan <cache>/font-plan.json
//
// The list follows Unicode's order and groups. Left out: the components (skin tone swatches, hair),
// the subdivision flags (England, Scotland, Wales: the service drops their tag characters, so they
// arrive as a plain black flag) and the middle finger (the chat filter rejects it, chat2.md §3.4).
// Skin tones are columns of their emoji: one tone for the whole picker, the same on every person.
import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const EMOJI_VERSION = "17.0";
const CLDR_VERSION = "48.2.0";

const CLDR = `https://raw.githubusercontent.com/unicode-org/cldr-json/${CLDR_VERSION}/cldr-json`;
const SOURCES = {
  test: {
    url: `https://www.unicode.org/Public/${EMOJI_VERSION}.0/emoji/emoji-test.txt`,
    sha256: "1d8a944f88d7952f7ef7c5167fef3c67995bcae24543949710231b03a201acda",
  },
  tr: {
    url: `${CLDR}/cldr-annotations-full/annotations/tr/annotations.json`,
    sha256: "203f92597ad0dbffde89083cb760c1585843e52f488c0c7866406f2ef6e881cc",
  },
  trDerived: {
    url: `${CLDR}/cldr-annotations-derived-full/annotationsDerived/tr/annotations.json`,
    sha256: "3fdd3797f361e8c41a9e088db3ac9ca25a7da7209ba858249c949a889695a6ca",
  },
  en: {
    url: `${CLDR}/cldr-annotations-full/annotations/en/annotations.json`,
    sha256: "f22083cb86dffb63a643d5bacb5d9899f82d2fa5d388ad4f3aed72184acef505",
  },
  enDerived: {
    url: `${CLDR}/cldr-annotations-derived-full/annotationsDerived/en/annotations.json`,
    sha256: "15c9457afa703a32dcc78d54626418a91e820fe0ddd67b7d491c4c4d9f5846ab",
  },
};

/** The picker's groups, in Unicode's order (the locale modules name them). */
const GROUPS = [
  ["Smileys & Emotion", "smileys"],
  ["People & Body", "people"],
  ["Animals & Nature", "nature"],
  ["Food & Drink", "food"],
  ["Travel & Places", "travel"],
  ["Activities", "activities"],
  ["Objects", "objects"],
  ["Symbols", "symbols"],
  ["Flags", "flags"],
];

/** Never offered and not in the font: the middle finger (chat2.md §3.4). */
const LEFT_OUT = new Set([0x1f595]);

/** Search words CLDR lacks that players type (CLDR names the flag "Türkiye" in English too). */
const ALIASES = { "🇹🇷": { en: ["turkey"] } };

/**
 * The font's slices besides one per group. "common" holds what most chats use: every face, the
 * hands and the most used emoji of the other groups (Unicode's emoji frequency list, and for a
 * game server games, trophies, gems, tools and weapons), so such a chat loads one small file.
 */
const COMMON_SUBGROUPS = new Set([
  "hand-fingers-open",
  "hand-fingers-partial",
  "hand-single-finger",
  "hand-fingers-closed",
  "hands",
  "hand-prop",
]);
const COMMON_EXTRAS = [
  "🔥",
  "✨",
  "🎉",
  "🎊",
  "🎂",
  "🎈",
  "🎁",
  "🌹",
  "🌸",
  "🌺",
  "💐",
  "🍀",
  "☀️",
  "⭐",
  "🌟",
  "⚡",
  "🌈",
  "✅",
  "✔️",
  "❌",
  "❗",
  "❓",
  "‼️",
  "🎮",
  "🏆",
  "🥇",
  "💎",
  "⛏️",
  "⚔️",
  "🗡️",
  "🛡️",
  "🏹",
  "🪓",
  "🧱",
  "🗿",
  "👀",
  "💪",
  "🤦",
  "🤷",
  "🙋",
  "👑",
  "🎯",
  "🍕",
  "☕",
  "🚀",
  "💻",
  "🎵",
  "🎶",
];

const TONES = [0x1f3fb, 0x1f3fc, 0x1f3fd, 0x1f3fe, 0x1f3ff];
const VS16 = 0xfe0f;
const ZWJ = 0x200d;

const here = dirname(fileURLToPath(import.meta.url));
const app = resolve(here, "..");
const DATA_DIR = join(app, "src/features/emoji/data");
const FONT_DIR = join(app, "src/assets/fonts/emoji");

function argument(name, fallback) {
  const at = process.argv.indexOf(name);
  return at > 0 && process.argv[at + 1] ? process.argv[at + 1] : fallback;
}

const cache = resolve(argument("--cache", join(tmpdir(), "pl-emoji-sources")));

async function source(name) {
  const { url, sha256 } = SOURCES[name];
  const file = join(
    cache,
    `${name}-${sha256.slice(0, 12)}${url.endsWith(".json") ? ".json" : ".txt"}`,
  );
  let bytes = existsSync(file) ? readFileSync(file) : null;
  if (!bytes) {
    const response = await fetch(url, {
      headers: { "User-Agent": "projects-launcher-emoji-data" },
    });
    if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
    bytes = Buffer.from(await response.arrayBuffer());
  }
  const actual = createHash("sha256").update(bytes).digest("hex");
  if (actual !== sha256) throw new Error(`${url}: SHA-256 ${actual}, expected ${sha256}`);
  mkdirSync(cache, { recursive: true });
  writeFileSync(file, bytes);
  return bytes.toString("utf8");
}

/** The same folding as `fold` in @pl/ui (search.ts): Turkish lower case, no marks, ı → i. */
function fold(text) {
  return text.toLocaleLowerCase("tr").normalize("NFD").replace(/\p{M}/gu, "").replace(/ı/g, "i");
}

/** Search words of a text: folded, split at anything but letters, digits and + # *. */
function words(text) {
  return fold(text)
    .split(/[^\p{L}\p{N}+#*]+/u)
    .filter(Boolean);
}

/** Words not already found by a prefix search for a longer word (or by one in `covered`). */
function reduce(list, covered = []) {
  const unique = [...new Set(list)];
  return unique.filter(
    (word) =>
      !unique.some((other) => other !== word && other.startsWith(word)) &&
      !covered.some((other) => other.startsWith(word)),
  );
}

const codepoints = (text) => [...text].map((ch) => ch.codePointAt(0));
const string = (cps) => String.fromCodePoint(...cps);
const isTone = (cp) => cp >= 0x1f3fb && cp <= 0x1f3ff;
/** A sequence without its skin tones and presentation selectors (groups a base with its tones). */
const bare = (cps) => cps.filter((cp) => !isTone(cp) && cp !== VS16).join(" ");

function parseTest(text) {
  const entries = [];
  let group = "";
  let subgroup = "";
  for (const line of text.split("\n")) {
    if (line.startsWith("# group:")) group = line.slice(8).trim();
    else if (line.startsWith("# subgroup:")) subgroup = line.slice(11).trim();
    else if (line && !line.startsWith("#")) {
      const [points, rest] = line.split(";");
      const status = rest.split("#")[0].trim();
      const version = /#\s*\S+\s+E(\d+\.\d+)\s/u.exec(line)?.[1] ?? "";
      const cps = points
        .trim()
        .split(/\s+/)
        .map((hex) => Number.parseInt(hex, 16));
      entries.push({ cps, status, group, subgroup, version });
    }
  }
  return entries;
}

function annotationsOf(json) {
  const root = JSON.parse(json);
  return (root.annotations ?? root.annotationsDerived).annotations;
}

async function main() {
  const test = parseTest(await source("test"));
  const names = {
    tr: {
      ...annotationsOf(await source("tr")),
      ...annotationsOf(await source("trDerived")),
    },
    en: {
      ...annotationsOf(await source("en")),
      ...annotationsOf(await source("enDerived")),
    },
  };

  const fully = test.filter(
    (entry) =>
      entry.status === "fully-qualified" &&
      entry.group !== "Component" &&
      entry.subgroup !== "subdivision-flag" &&
      !entry.cps.some((cp) => LEFT_OUT.has(cp)),
  );

  // Code points a skin tone may follow (learned from the data, so new ones need no list here).
  const toneBases = new Set();
  for (const { cps } of fully) {
    cps.forEach((cp, index) => {
      if (isTone(cps[index + 1] ?? 0)) toneBases.add(cp);
    });
  }
  /** A tone on every person of the sequence, the presentation selector after each dropped. */
  const withTone = (cps, tone) => {
    const out = [];
    for (let index = 0; index < cps.length; index++) {
      const cp = cps[index];
      out.push(cp);
      if (toneBases.has(cp)) {
        out.push(tone);
        if (cps[index + 1] === VS16) index++;
      }
    }
    return out;
  };

  const bases = fully.filter(({ cps }) => !cps.some(isTone));
  const toned = new Map();
  for (const entry of fully.filter(({ cps }) => cps.some(isTone))) {
    const key = bare(entry.cps);
    if (!toned.has(key)) toned.set(key, []);
    toned.get(key).push(entry);
  }

  const groupIndex = new Map(GROUPS.map(([name], index) => [name, index]));
  const items = [];
  let explicitTones = 0;
  const missing = { tr: [], en: [] };
  for (const base of bases) {
    const emoji = string(base.cps);
    const group = groupIndex.get(base.group);
    if (group === undefined) throw new Error(`Unknown group ${base.group}`);
    // The variants with one tone on every person, in tone order.
    const variants = toned.get(bare(base.cps)) ?? [];
    let tones = "";
    if (variants.length) {
      const uniform = TONES.map((tone) =>
        variants.find(({ cps }) => cps.filter(isTone).every((cp) => cp === tone)),
      );
      if (uniform.some((entry) => !entry)) throw new Error(`Missing a skin tone of ${emoji}`);
      const ruled = TONES.every(
        (tone, index) => string(withTone(base.cps, tone)) === string(uniform[index].cps),
      );
      if (ruled) tones = "1";
      else {
        tones = uniform.map((entry) => string(entry.cps)).join(" ");
        explicitTones++;
      }
    }
    const key = emoji.replaceAll("\u{fe0f}", "");
    const tr = names.tr[key];
    const en = names.en[key];
    if (!tr?.tts?.[0]) missing.tr.push(emoji);
    if (!en?.tts?.[0]) missing.en.push(emoji);
    items.push({ emoji, base, group, tones, tr, en });
  }

  const row = (fields) => {
    for (const field of fields) {
      if (/[\t\n\r]/.test(field)) throw new Error(`A field holds a tab or line break: ${field}`);
    }
    return fields.join("\t");
  };
  // The header: format, language, sources, then the code points a skin tone follows (for the rows
  // whose tones column is "1": a tone after each of them, a VS16 right after one dropped).
  const header = (lang) =>
    row([
      "PL-EMOJI",
      "1",
      lang,
      `Unicode Emoji ${EMOJI_VERSION}`,
      `CLDR ${CLDR_VERSION} (Unicode License v3)`,
      "generated by apps/launcher/scripts/emoji-data.mjs, do not edit",
      [...toneBases]
        .sort((a, b) => a - b)
        .map((cp) => cp.toString(16).toUpperCase())
        .join(" "),
    ]);

  mkdirSync(DATA_DIR, { recursive: true });
  for (const lang of ["tr", "en"]) {
    const lines = [header(lang)];
    for (const item of items) {
      const own = item[lang] ?? item.en;
      const name = own?.tts?.[0] ?? item.en?.tts?.[0] ?? "";
      const nameWords = words(name);
      const aliases = ALIASES[item.emoji] ?? {};
      const keywordWords = [...(own?.default ?? []), ...(aliases[lang] ?? [])].flatMap(words);
      const searchWords = reduce(keywordWords, nameWords);
      const fields = [item.emoji, String(item.group), item.tones, name, searchWords.join(" ")];
      if (lang === "tr") {
        // The English name (it ranks an English search: "fire" puts 🔥 first) and the English
        // words the Turkish ones do not already find ("skull", "lol").
        const englishName = item.en?.tts?.[0] ?? "";
        const english = [...(item.en?.default ?? []), ...(aliases.en ?? [])].flatMap(words);
        fields.push(
          englishName,
          reduce(english, [...nameWords, ...searchWords, ...words(englishName)]).join(" "),
        );
      }
      lines.push(row(fields));
    }
    writeFileSync(join(DATA_DIR, `emoji.${lang}.tsv`), `${lines.join("\n")}\n`);
  }

  // The font's slices. A sequence lives in the slice of its emoji (tones with their base); every
  // code point of its sequences is in that slice's unicode-range, so each slice shapes its
  // sequences on its own (Blink maps no character outside a face's range). Ranges overlap (ZWJ,
  // tones, the rocket of an astronaut), and the engine (Blink, measured in Edge) tries the faces
  // from the last declared: each face whose range holds a character of an emoji loads, until one
  // holds all of them. So the ZWJ sequences of the other groups (a polar bear, a rainbow flag) live
  // with the people's, and the faces are ordered for the fewest needless loads.
  const commonExtras = new Set(COMMON_EXTRAS.map((emoji) => bare(codepoints(emoji))));
  const sliceOf = (entry) => {
    if (entry.group === "Smileys & Emotion") return "common";
    if (COMMON_SUBGROUPS.has(entry.subgroup)) return "common";
    if (commonExtras.has(bare(entry.cps))) return "common";
    if (process.env.ZWJ_TO_PEOPLE !== "0" && entry.cps.includes(ZWJ)) return "people";
    return GROUPS[groupIndex.get(entry.group)][1];
  };
  for (const extra of commonExtras) {
    if (!bases.some(({ cps }) => bare(cps) === extra)) throw new Error(`Unknown extra ${extra}`);
  }
  const homeOfBase = new Map(bases.map((entry) => [bare(entry.cps), sliceOf(entry)]));
  const slices = new Map();
  const sequences = new Map();
  for (const entry of fully) {
    // Tones go with their base; a sequence that exists only with tones (a handshake with two
    // tones) goes with the slice of its first person.
    const home =
      homeOfBase.get(bare(entry.cps)) ?? homeOfBase.get(bare([entry.cps[0]])) ?? sliceOf(entry);
    if (!slices.has(home)) slices.set(home, new Set());
    // VS16 has no glyph (the shaper hides it), so no slice claims it: a slice claiming it would
    // load for any emoji written with one.
    for (const cp of entry.cps) if (cp !== VS16) slices.get(home).add(cp);
    sequences.set(home, (sequences.get(home) ?? 0) + 1);
  }
  // The needless bytes of an order: every emoji drawn alone, the faces the engine loads until one
  // draws it, beyond its own slice (sizes in KB as emoji-font.py measured them; a slice the
  // engine reaches first and that holds every character draws it, a big one too). The common
  // slice counts as loaded (nearly every chat with an emoji needs it); its emoji weigh ten (they
  // are the ones chats are full of), a toned one a fifth of its base. Emoji with the same faces to
  // try count together, so all 9! orders are quick to weigh.
  const SIZE_KB = {
    people: 420,
    flags: 700,
    objects: 166,
    nature: 163,
    travel: 161,
    food: 116,
    activities: 67,
    symbols: 50,
    common: 0,
  };
  const ids = [...slices.keys()];
  const kinds = new Map();
  const weightOf = (entry) =>
    (sliceOf(entry) === "common" ? 10 : 1) * (entry.cps.some(isTone) ? 0.2 : 1);
  for (const entry of fully) {
    const cps = [...new Set(entry.cps.filter((cp) => cp !== VS16))];
    const tries = ids.filter((id) => cps.some((cp) => slices.get(id).has(cp)));
    const draws = tries.filter((id) => cps.every((cp) => slices.get(id).has(cp)));
    const own = homeOfBase.get(bare(entry.cps)) ?? homeOfBase.get(bare([entry.cps[0]]));
    const key = `${tries.join(",")}|${draws.join(",")}|${own}`;
    const kind = kinds.get(key) ?? { tries, draws, own, weight: 0 };
    kind.weight += weightOf(entry);
    kinds.set(key, kind);
  }
  const weighed = [...kinds.values()].filter(({ tries }) => tries.length > 1);
  const cost = (order) => {
    const position = new Map(order.map((id, index) => [id, index]));
    let needless = 0;
    for (const { tries, draws, own, weight } of weighed) {
      // The faces it holds, from the last declared, until the first that draws it.
      const sorted = [...tries].sort((a, b) => position.get(b) - position.get(a));
      const at = sorted.findIndex((id) => draws.includes(id));
      const loaded = sorted.slice(0, at + 1).reduce((sum, id) => sum + SIZE_KB[id], 0);
      needless += weight * (loaded - SIZE_KB[own]);
    }
    return needless;
  };
  let best = null;
  let bestCost = Number.POSITIVE_INFINITY;
  const permute = (rest, prefix) => {
    if (!rest.length) {
      const value = cost(prefix);
      if (value < bestCost) {
        bestCost = value;
        best = prefix;
      }
      return;
    }
    rest.forEach((id, index) => {
      permute([...rest.slice(0, index), ...rest.slice(index + 1)], [...prefix, id]);
    });
  };
  permute(ids, []);
  const total = fully.reduce((sum, entry) => sum + weightOf(entry), 0);

  const ranges = (cps) => {
    const sorted = [...cps].sort((a, b) => a - b);
    const out = [];
    for (const cp of sorted) {
      const last = out.at(-1);
      if (last && cp === last[1] + 1) last[1] = cp;
      else out.push([cp, cp]);
    }
    const hex = (cp) => cp.toString(16).toUpperCase();
    return out.map(([from, to]) => (from === to ? `U+${hex(from)}` : `U+${hex(from)}-${hex(to)}`));
  };
  const faces = best.map((id) => ({
    id,
    file: `pl-emoji-${id}.woff2`,
    codepoints: [...slices.get(id)].sort((a, b) => a - b),
    sequences: sequences.get(id) ?? 0,
  }));
  const css = [
    "/* PL Emoji: Noto Color Emoji 2.051 (COLRv1; Copyright 2022 Google Inc.; SIL Open Font License 1.1,",
    "   LICENSE.txt), subset to Unicode Emoji 17.0 and sliced so the engine loads only the slices of",
    "   the emoji on screen. Generated by apps/launcher/scripts/emoji-data.mjs and emoji-font.py; do",
    "   not edit. Engines that cannot draw COLRv1 (WebKit on macOS) skip it and keep the system's",
    "   emoji. The face declared last wins a character two slices share; the script orders them. */",
    "/* biome-ignore lint/correctness/noUnknownFunction: font-tech() is CSS Fonts 4 (Chromium 108+) */",
    "@supports font-tech(color-COLRv1) {",
    ...faces.flatMap((face) => [
      "  @font-face {",
      '    font-family: "PL Emoji";',
      "    font-style: normal;",
      "    font-weight: 100 900;",
      "    font-display: block;",
      `    src: url("./${face.file}") format("woff2");`,
      `    unicode-range: ${ranges(face.codepoints).join(", ")};`,
      "  }",
    ]),
    "}",
    "",
  ];
  mkdirSync(FONT_DIR, { recursive: true });
  writeFileSync(join(FONT_DIR, "emoji.css"), css.join("\n"));
  // In the repository's style, so a run changes only what changed.
  try {
    execSync(`pnpm exec biome format --write "${join(FONT_DIR, "emoji.css")}"`, {
      cwd: resolve(app, "../.."),
      stdio: "ignore",
    });
  } catch {
    console.log("emoji.css is not formatted (Biome did not run)");
  }
  const plan = join(cache, "font-plan.json");
  writeFileSync(
    plan,
    JSON.stringify(
      { emojiVersion: EMOJI_VERSION, faces, outDir: FONT_DIR, leftOut: [...LEFT_OUT] },
      null,
      1,
    ),
  );

  const groups = GROUPS.map(
    ([, id], index) => `${id} ${items.filter((i) => i.group === index).length}`,
  );
  console.log(`${items.length} emoji (${groups.join(", ")})`);
  console.log(
    `tones: ${items.filter((i) => i.tones).length} with tones, ${explicitTones} listed in full`,
  );
  console.log(
    `missing names: tr ${missing.tr.join(" ") || "none"}; en ${missing.en.join(" ") || "none"}`,
  );
  console.log(
    `slices (lowest first): ${faces.map((f) => `${f.id} ${f.sequences} seq/${f.codepoints.length} cp`).join(", ")}`,
  );
  console.log(
    `needless KB per emoji drawn alone: ${(bestCost / total).toFixed(1)} (weighed, ${total.toFixed(0)} emoji)`,
  );
  console.log(`plan: ${plan}`);
}

await main();
