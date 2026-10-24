// Copies the outlines of the Phosphor icons the overlay draws (the launcher's icon family, MIT) into
// src/main/resources/assets/projects_overlay/icons/phosphor.txt, one "name<TAB>weight<TAB>path" line
// each. The overlay rasterizes them at the GUI scale (ui/draw/ShapeBaker.java), so they stay sharp.
// The crews' emblems (TEAM6: the fill weight the launcher's crew mark uses, team-emblem.tsx) go to
// emblems.txt beside it, one "emblem<TAB>name<TAB>path" line each.
//
//   node tools/extract-icons.mjs <@phosphor-icons/react package dir> <out file> [<emblems out file>]

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

const [pkg, out, emblemsOut = join(dirname(out), "emblems.txt")] = process.argv.slice(2);
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
  ["Smiley", "regular"],
  ["ArrowBendUpLeft", "bold"],
  ["Sticker", "regular"],
  ["Globe", "regular"],
  ["ArrowSquareOut", "regular"],
  ["HandWaving", "regular"],
  ["DiscordLogo", "fill"],
];

// The crew palette's emblems (crew-look.ts EMBLEMS) and the icons team-emblem.tsx draws them with.
const EMBLEMS = [
  ["sword", "Sword"],
  ["shield", "Shield"],
  ["axe", "Axe"],
  ["crown", "Crown"],
  ["trophy", "Trophy"],
  ["flag", "Flag"],
  ["target", "Target"],
  ["skull", "Skull"],
  ["ghost", "Ghost"],
  ["flame", "Fire"],
  ["bolt", "Lightning"],
  ["sun", "Sun"],
  ["moon", "Moon"],
  ["star", "Star"],
  ["planet", "Planet"],
  ["snowflake", "Snowflake"],
  ["waves", "Waves"],
  ["leaf", "Leaf"],
  ["tree", "Tree"],
  ["cactus", "Cactus"],
  ["paw", "PawPrint"],
  ["cat", "Cat"],
  ["horse", "Horse"],
  ["bird", "Bird"],
  ["fish", "FishSimple"],
  ["butterfly", "Butterfly"],
  ["heart", "Heart"],
  ["diamond", "Diamond"],
  ["cube", "Cube"],
  ["rocket", "Rocket"],
  ["potion", "Flask"],
  ["wand", "MagicWand"],
  ["castle", "CastleTurret"],
  ["controller", "GameController"],
  ["alien", "Alien"],
  ["robot", "Robot"],
];

const license = readFileSync(join(pkg, "LICENSE"), "utf8")
  .trim()
  .split(/\r?\n/)
  .map((line) => `# ${line}`.trimEnd());
const header = (what) => [
  `# Phosphor Icons (https://phosphoricons.com), ${what}, copied by tools/extract-icons.mjs. 256x256 viewBox.`,
  "#",
  ...license,
];

function outline(name, weight) {
  const source = readFileSync(join(pkg, "dist", "defs", `${name}.es.js`), "utf8");
  const start = source.indexOf(`"${weight}"`);
  if (start < 0) throw new Error(`${name}: no ${weight} weight`);
  const next = source.indexOf("\n  [", start + 1);
  const block = source.slice(start, next < 0 ? undefined : next);
  const paths = [...block.matchAll(/d: "([^"]+)"/g)].map((m) => m[1]);
  if (paths.length === 0) throw new Error(`${name}: no path`);
  if (/createElement\("(?!path)[a-z]+"/.test(block)) throw new Error(`${name}: not only paths`);
  return paths.join(" ");
}

const lines = header("the interface's icons");
for (const [name, weight] of ICONS) {
  lines.push(`${name}\t${weight}\t${outline(name, weight)}`);
}
writeFileSync(out, `${lines.join("\n")}\n`);
console.log(`${ICONS.length} icons -> ${out}`);

const emblems = header("the crews' emblems (fill)");
for (const [emblem, name] of EMBLEMS) {
  emblems.push(`${emblem}\t${name}\t${outline(name, "fill")}`);
}
writeFileSync(emblemsOut, `${emblems.join("\n")}\n`);
console.log(`${EMBLEMS.length} emblems -> ${emblemsOut}`);
