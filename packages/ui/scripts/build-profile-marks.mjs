// Builds packages/ui/src/components/profile-mark-drawings.ts (BADGES, docs/design.md §6): the badge
// outlines (SOC-VERIFY's eight-lobed seal, a round badge, a shield) and glyphs for the 12, 14 and
// 16 px grids. Shapes and glyphs are designed on the 16 px grid; 12 and 14 scale them and round to
// quarter pixels, except the check (placed per size by SOC-VERIFY, legs at 45 degrees through
// whole pixels) and the seal (SOC-VERIFY's paths).
//
//   node packages/ui/scripts/build-profile-marks.mjs [--preview <dir>]
//
// `--preview` also writes <dir>/preview.html: every badge at every size on dark and light grounds
// and beside names, to screenshot at 100 % and 200 %.
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const outPath = join(here, "..", "src", "components", "profile-mark-drawings.ts");
const previewAt = process.argv.indexOf("--preview");
const previewDir = previewAt === -1 ? null : process.argv[previewAt + 1];

const SIZES = [12, 14, 16];
const q = (v) => Math.round(v * 4) / 4;
const f = (v) => {
  const s = q(v)
    .toFixed(2)
    .replace(/\.?0+$/, "");
  return s === "-0" ? "0" : s;
};

// ---- shapes ---------------------------------------------------------------------------------
const SEAL = {
  12: "M4.09 1.4A2 2 0 0 1 7.91 1.4A2 2 0 0 1 10.6 4.09A2 2 0 0 1 10.6 7.91A2 2 0 0 1 7.91 10.6A2 2 0 0 1 4.09 10.6A2 2 0 0 1 1.4 7.91A2 2 0 0 1 1.4 4.09A2 2 0 0 1 4.09 1.4Z",
  14: "M4.72 1.5A2.48 2.48 0 0 1 9.28 1.5A2.48 2.48 0 0 1 12.5 4.72A2.48 2.48 0 0 1 12.5 9.28A2.48 2.48 0 0 1 9.28 12.5A2.48 2.48 0 0 1 4.72 12.5A2.48 2.48 0 0 1 1.5 9.28A2.48 2.48 0 0 1 1.5 4.72A2.48 2.48 0 0 1 4.72 1.5Z",
  16: "M5.37 1.64A2.93 2.93 0 0 1 10.63 1.64A2.93 2.93 0 0 1 14.36 5.37A2.93 2.93 0 0 1 14.36 10.63A2.93 2.93 0 0 1 10.63 14.36A2.93 2.93 0 0 1 5.37 14.36A2.93 2.93 0 0 1 1.64 10.63A2.93 2.93 0 0 1 1.64 5.37A2.93 2.93 0 0 1 5.37 1.64Z",
};
const ROUND_R = { 12: 5, 14: 5.75, 16: 6.5 };
function circle(cx, cy, r) {
  return `M${f(cx - r)} ${f(cy)}A${f(r)} ${f(r)} 0 1 1 ${f(cx + r)} ${f(cy)}A${f(r)} ${f(r)} 0 1 1 ${f(cx - r)} ${f(cy)}Z`;
}
function shield(size) {
  const s = size / 16;
  // Heater shield: flat top with soft corners, straight sides, a curved point.
  const P = (x, y) => `${f(x * s)} ${f(y * s)}`;
  return `M${P(3.6, 1.75)}H${f(12.4 * s)}Q${P(13.75, 1.75)} ${P(13.75, 3.1)}V${f(7.5 * s)}C${P(13.75, 11.1)} ${P(11.1, 13.55)} ${P(8, 14.75)}C${P(4.9, 13.55)} ${P(2.25, 11.1)} ${P(2.25, 7.5)}V${f(3.1 * s)}Q${P(2.25, 1.75)} ${P(3.6, 1.75)}Z`;
}
const SHAPES = {
  seal: (size) => SEAL[size],
  round: (size) => circle(size / 2, size / 2, ROUND_R[size]),
  shield,
};

// ---- glyphs (16 px design grid, centre 8,8) ---------------------------------------------------
const CHECK = {
  12: { d: "M3 6L5 8L8.5 4.5", w: 1.6 },
  14: { d: "M3.75 7L6 9.25L10 5.25", w: 1.8 },
  16: { d: "M4.5 8L7 10.5L11.5 6", w: 2 },
};

function star(cx, cy, r, inner) {
  const pts = [];
  for (let i = 0; i < 10; i++) {
    const a = -Math.PI / 2 + (i * Math.PI) / 5;
    const rr = i % 2 === 0 ? r : r * inner;
    pts.push([cx + rr * Math.cos(a), cy + rr * Math.sin(a)]);
  }
  return pts;
}
function poly(points, t) {
  return `${points
    .map(([x, y], i) => {
      const [X, Y] = t(x, y);
      return `${i === 0 ? "M" : "L"}${f(X)} ${f(Y)}`;
    })
    .join("")}Z`;
}
function circ16(cx, cy, r, t, k) {
  const [X, Y] = t(cx, cy);
  return circle(X, Y, r * k);
}
// Each glyph: (t, k) => path, where t maps 16-grid points to the target grid and k is the scale.
const GLYPHS = {
  star: (t) => poly(star(8, 8.45, 4.35, 0.5), t),
  cloud: (t, k) =>
    [
      // A flat-bottomed cloud: three bumps on a rounded base (one fill, nonzero union).
      roundRect(t, 3.9, 8.5, 8.4, 2.6, 1.3, k),
      circ16(6.1, 8.75, 2.05, t, k),
      circ16(8.75, 7.25, 2.65, t, k),
      circ16(10.6, 9.0, 1.75, t, k),
    ].join(""),
  bolt: (t) =>
    poly(
      [
        [10.7, 3.2],
        [4.3, 9.45],
        [8.0, 9.45],
        [5.5, 12.9],
        [11.9, 6.55],
        [8.2, 6.55],
      ],
      t,
    ),
  crown: (t) =>
    poly(
      [
        [4.25, 11.5],
        [3.75, 5.25],
        [6.35, 7.9],
        [8, 4.5],
        [9.65, 7.9],
        [12.25, 5.25],
        [11.75, 11.5],
      ],
      t,
    ),
  heart: (t) => {
    const p = (x, y) => pt(t, x, y);
    return `M${p(8, 12.35)}C${p(8, 12.35)} ${p(3.6, 9.7)} ${p(3.6, 6.55)}C${p(3.6, 4.95)} ${p(4.75, 3.85)} ${p(6.15, 3.85)}C${p(7, 3.85)} ${p(7.6, 4.3)} ${p(8, 5.0)}C${p(8.4, 4.3)} ${p(9, 3.85)} ${p(9.85, 3.85)}C${p(11.25, 3.85)} ${p(12.4, 4.95)} ${p(12.4, 6.55)}C${p(12.4, 9.7)} ${p(8, 12.35)} ${p(8, 12.35)}Z`;
  },
  flask: (t, k) => {
    const p = (x, y) => pt(t, x, y);
    return (
      `M${p(6.85, 5.0)}L${p(9.15, 5.0)}L${p(9.15, 7.0)}L${p(12.05, 11.15)}Q${p(12.5, 12.25)} ${p(11.3, 12.25)}L${p(4.7, 12.25)}Q${p(3.5, 12.25)} ${p(3.95, 11.15)}L${p(6.85, 7.0)}Z` +
      roundRect(t, 6.1, 3.5, 3.8, 1.5, 0.5, k)
    );
  },
  play: (t) =>
    poly(
      [
        [5.6, 3.9],
        [12.4, 8],
        [5.6, 12.1],
      ],
      t,
    ),
  trophy: (t, k) => {
    const p = (x, y) => pt(t, x, y);
    return (
      // Cup, stem and foot as one outline; the handles are two small rings drawn as fills.
      `M${p(5.25, 3.75)}L${p(10.75, 3.75)}L${p(10.75, 6.6)}C${p(10.75, 8.55)} ${p(9.75, 9.65)} ${p(8.75, 9.95)}L${p(8.75, 10.85)}L${p(10.25, 10.85)}L${p(10.25, 12.25)}L${p(5.75, 12.25)}L${p(5.75, 10.85)}L${p(7.25, 10.85)}L${p(7.25, 9.95)}C${p(6.25, 9.65)} ${p(5.25, 8.55)} ${p(5.25, 6.6)}Z` +
      ring(t, 4.6, 5.9, 1.7, 0.65, k) +
      ring(t, 11.4, 5.9, 1.7, 0.65, k)
    );
  },
};

function pt(t, x, y) {
  const [X, Y] = t(x, y);
  return `${f(X)} ${f(Y)}`;
}
function roundRect(t, x, y, w, h, r, k) {
  const [X, Y] = t(x, y);
  const W = w * k;
  const H = h * k;
  const R = Math.min(r * k, H / 2);
  return `M${f(X + R)} ${f(Y)}H${f(X + W - R)}A${f(R)} ${f(R)} 0 0 1 ${f(X + W)} ${f(Y + R)}V${f(Y + H - R)}A${f(R)} ${f(R)} 0 0 1 ${f(X + W - R)} ${f(Y + H)}H${f(X + R)}A${f(R)} ${f(R)} 0 0 1 ${f(X)} ${f(Y + H - R)}V${f(Y + R)}A${f(R)} ${f(R)} 0 0 1 ${f(X + R)} ${f(Y)}Z`;
}
// A ring (outer circle minus inner) as one subpath pair: the inner runs the other way (evenodd
// is not needed: nonzero with opposite winding cuts the hole).
function ring(t, cx, cy, r, inner, k) {
  const [X, Y] = t(cx, cy);
  const R = r * k;
  const I = inner * k;
  return (
    `M${f(X - R)} ${f(Y)}A${f(R)} ${f(R)} 0 1 1 ${f(X + R)} ${f(Y)}A${f(R)} ${f(R)} 0 1 1 ${f(X - R)} ${f(Y)}Z` +
    `M${f(X - I)} ${f(Y)}A${f(I)} ${f(I)} 0 1 0 ${f(X + I)} ${f(Y)}A${f(I)} ${f(I)} 0 1 0 ${f(X - I)} ${f(Y)}Z`
  );
}

// Glyph box per shape: the shield's point leaves less room below, so its glyphs sit higher and a
// little smaller.
const FIT = {
  seal: { scale: 1, dy: 0 },
  round: { scale: 1, dy: 0 },
  shield: { scale: 0.9, dy: -0.75 },
};

const drawings = {};
for (const size of SIZES) {
  const k0 = size / 16;
  drawings[size] = { shapes: {}, glyphs: {}, check: {} };
  for (const [name, make] of Object.entries(SHAPES)) {
    drawings[size].shapes[name] = make(size);
  }
  for (const [shape, fit] of Object.entries(FIT)) {
    const k = k0 * fit.scale;
    const c = size / 2;
    const t = (x, y) => [c + (x - 8) * k, c + (y - 8) * k + fit.dy * k0];
    drawings[size].glyphs[shape] = {};
    for (const [name, make] of Object.entries(GLYPHS)) {
      drawings[size].glyphs[shape][name] = make(t, k);
    }
    const dy = fit.dy * k0;
    drawings[size].check[shape] = {
      d: dy === 0 ? CHECK[size].d : shiftPath(CHECK[size].d, dy),
      w: CHECK[size].w,
    };
  }
}
function shiftPath(d, dy) {
  return d.replace(/([ML])([\d.]+) ([\d.]+)/g, (_, c, x, y) => `${c}${x} ${f(Number(y) + dy)}`);
}

const d = drawings;
const sizes = [12, 14, 16];
const shapes = ["seal", "round", "shield"];
const glyphs = ["star", "cloud", "bolt", "crown", "heart", "flask", "play", "trophy"];

for (const size of sizes) {
  for (const g of glyphs) {
    if (d[size].glyphs.seal[g] !== d[size].glyphs.round[g])
      throw new Error(`seal/round differ: ${size} ${g}`);
  }
}

const lines = [];
lines.push(
  "// Generated by packages/ui/scripts/build-profile-marks.mjs (BADGES): every shape and glyph drawn",
);
lines.push("// for each size's pixel grid. Change them there and run it again, never by hand.");
lines.push("");
lines.push("type Size = 12 | 14 | 16;");
lines.push('type Shape = "seal" | "round" | "shield";');
lines.push("");
lines.push("/** The badge's outline per size (the seal is SOC-VERIFY's rosette). */");
lines.push("export const SHAPE_PATHS: Record<Size, Record<Shape, string>> = {");
for (const size of sizes) {
  lines.push(`  ${size}: {`);
  for (const s of shapes) lines.push(`    ${s}: "${d[size].shapes[s]}",`);
  lines.push("  },");
}
lines.push("};");
lines.push("");
lines.push("/**");
lines.push(
  " * The filled glyphs (white) per size: `plain` inside the seal and the round badge, `shield` a",
);
lines.push(" * little smaller and higher (the shield's point leaves less room below).");
lines.push(" */");
lines.push("export const GLYPH_PATHS: Record<");
lines.push("  Size,");
lines.push('  Record<"plain" | "shield", Record<FilledGlyph, string>>');
lines.push("> = {");
for (const size of sizes) {
  lines.push(`  ${size}: {`);
  for (const [key, shape] of [
    ["plain", "seal"],
    ["shield", "shield"],
  ]) {
    lines.push(`    ${key}: {`);
    for (const g of glyphs) lines.push(`      ${g}: "${d[size].glyphs[shape][g]}",`);
    lines.push("    },");
  }
  lines.push("  },");
}
lines.push("};");
lines.push("");
lines.push(
  "/** The check is a stroke (round caps), its legs at 45° through whole pixels (SOC-VERIFY). */",
);
lines.push(
  "export const CHECK_PATHS: Record<Size, { plain: string; shield: string; width: number }> = {",
);
for (const size of sizes) {
  lines.push(
    `  ${size}: { plain: "${d[size].check.seal.d}", shield: "${d[size].check.shield.d}", width: ${d[size].check.seal.w} },`,
  );
}
lines.push("};");
lines.push("");
lines.push(`export type FilledGlyph = ${glyphs.map((g) => `"${g}"`).join(" | ")};`);
lines.push("");
writeFileSync(outPath, lines.join(String.fromCharCode(10)));
console.log("written", outPath);

if (previewDir) {
  // ---- preview ---------------------------------------------------------------------------------
  const COLORS = {
    blue: "#2a8af0",
    red: "#e0434a",
    purple: "#9b5eeb",
    green: "#219453",
    orange: "#d95f1a",
    teal: "#138d89",
    indigo: "#6673e8",
    pink: "#df378b",
    slate: "#7f7a9f",
  };
  const GLYPH_ORDER = ["check", ...Object.keys(GLYPHS)];
  function svg(size, shape, glyph, color) {
    const d = drawings[size];
    const g =
      glyph === "check"
        ? `<path d="${d.check[shape].d}" fill="none" stroke="#fff" stroke-width="${d.check[shape].w}" stroke-linecap="round" stroke-linejoin="round"/>`
        : `<path d="${d.glyphs[shape][glyph]}" fill="#fff"/>`;
    return `<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" style="color:${color}"><path d="${d.shapes[shape]}" fill="currentColor"/>${g}</svg>`;
  }
  const glyphColor = (i) => Object.values(COLORS)[i % 9];
  let body = "";
  for (const bg of ["#121016", "#ffffff"]) {
    const ink = bg === "#ffffff" ? "#1a1720" : "#f3ebdd";
    body += `<section style="background:${bg};color:${ink};padding:10px 14px">`;
    for (const size of SIZES) {
      body += `<div class="row"><span class="lab">${size}</span>`;
      for (const shape of Object.keys(SHAPES)) {
        body += `<span class="grp">`;
        GLYPH_ORDER.forEach((glyph, i) => {
          body += svg(size, shape, glyph, glyphColor(i));
        });
        body += `</span>`;
      }
      body += `</div>`;
    }
    // Names beside text, as rows use them.
    body += `<div class="row names">`;
    const samples = [
      ["Kaan_07", 14, "seal", "check", "red"],
      ["xNightOwl", 14, "seal", "check", "purple"],
      ["BlockSmith", 14, "seal", "check", "green"],
      ["Deniz", 14, "seal", "check", "blue"],
      ["PixelKedi", 14, "seal", "star", "orange"],
      ["CloudRider", 14, "round", "cloud", "teal"],
      ["AFKKing", 14, "seal", "cloud", "indigo"],
      ["LabRat_", 14, "round", "flask", "pink"],
      ["StreamCat", 14, "round", "play", "slate"],
    ];
    for (const [name, size, shape, glyph, color] of samples) {
      body += `<span class="nm" style="font-size:13.5px">${name}${svg(size, shape, glyph, COLORS[color])}</span>`;
    }
    body += `</div><div class="row names">`;
    for (const [name, , shape, glyph, color] of samples) {
      body += `<span class="nm" style="font-size:12px">${name}${svg(12, shape, glyph, COLORS[color])}</span>`;
    }
    body += `</div><div class="row names">`;
    for (const [name, , shape, glyph, color] of samples.slice(0, 5)) {
      body += `<span class="nm" style="font-size:16px;font-weight:600">${name}${svg(16, shape, glyph, COLORS[color])}</span>`;
    }
    body += `</div></section>`;
  }
  const html = `<!doctype html><html><head><meta charset="utf-8"><style>
  body{margin:0;font-family:Segoe UI,system-ui,sans-serif}
  .row{display:flex;align-items:center;gap:14px;margin:6px 0}
  .lab{width:22px;font-size:11px;opacity:.7}
  .grp{display:inline-flex;gap:6px;padding-right:10px;border-right:1px solid #8884}
  .nm{display:inline-flex;align-items:center;gap:4px}
  svg{display:block;flex:none}
  </style></head><body><main>${body}</main></body></html>`;
  writeFileSync(`${previewDir}/preview.html`, html);
}
