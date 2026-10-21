// Copies the outlines of the Phosphor icons the overlay draws (the launcher's icon family, MIT) into
// src/main/resources/assets/projects_overlay/icons/phosphor.txt, one "name<TAB>weight<TAB>path" line
// each. The overlay rasterizes them at the GUI scale (ui/draw/VectorIcons.java), so they stay sharp.
//
//   node tools/extract-icons.mjs <@phosphor-icons/react package dir> <out file>

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [pkg, out] = process.argv.slice(2);
const ICONS = [
  ["GearSix", "regular"],
  ["CaretLeft", "bold"],
  ["CaretRight", "bold"],
  ["CaretDown", "bold"],
  ["PaperPlaneRight", "fill"],
  ["X", "bold"],
  ["Check", "bold"],
  ["Clock", "regular"],
  ["WarningCircle", "regular"],
  ["UsersThree", "regular"],
  ["MegaphoneSimple", "regular"],
  ["ChatsCircle", "regular"],
  ["GameController", "regular"],
  ["Info", "regular"],
  ["Bell", "regular"],
  ["SpeakerHigh", "regular"],
  ["Keyboard", "regular"],
  ["ArrowCounterClockwise", "bold"],
  ["Plugs", "regular"],
  ["SignIn", "regular"],
];

const license = readFileSync(join(pkg, "LICENSE"), "utf8")
  .trim()
  .split(/\r?\n/)
  .map((line) => `# ${line}`.trimEnd());
const lines = [
  "# Phosphor Icons (https://phosphoricons.com), copied by tools/extract-icons.mjs. 256x256 viewBox.",
  "#",
  ...license,
];
for (const [name, weight] of ICONS) {
  const source = readFileSync(join(pkg, "dist", "defs", `${name}.es.js`), "utf8");
  const start = source.indexOf(`"${weight}"`);
  if (start < 0) throw new Error(`${name}: no ${weight} weight`);
  const next = source.indexOf("\n  [", start + 1);
  const block = source.slice(start, next < 0 ? undefined : next);
  const paths = [...block.matchAll(/d: "([^"]+)"/g)].map((m) => m[1]);
  if (paths.length === 0) throw new Error(`${name}: no path`);
  if (/createElement\("(?!path)[a-z]+"/.test(block)) throw new Error(`${name}: not only paths`);
  lines.push(`${name}\t${weight}\t${paths.join(" ")}`);
}
writeFileSync(out, `${lines.join("\n")}\n`);
console.log(`${ICONS.length} icons -> ${out}`);
