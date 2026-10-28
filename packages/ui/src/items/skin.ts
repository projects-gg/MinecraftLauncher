// A player head drawn with its own skin (INV-HEADS). The skin comes from Mojang's own texture
// server, read the way the game reads a downloaded skin, and the head is drawn the way
// crates/pl-items draws the atlas's heads (special.rs `skull`, render.rs `rasterize`, with 26.3's
// `items/player_head.json` transformation and `models/item/template_skull.json` gui display), so
// a skin head sits beside the plain heads pixel for pixel. Plain data and maths: the loading is
// skin-head.tsx's, the tests and the phone (FREE-M) use this as it is.

/** Mojang's texture server: skins come from there and nowhere else (no avatar service). */
export const SKIN_TEXTURE_ORIGIN = "https://textures.minecraft.net";

/** Whether a frame's `skin` is a texture hash: 16–64 lower case hex digits. */
export function isSkinHash(skin: string | undefined): skin is string {
  return typeof skin === "string" && /^[0-9a-f]{16,64}$/.test(skin);
}

/** The skin's texture on Mojang's server, or undefined for anything that is not a hash. */
export function skinTextureUrl(skin: string | undefined): string | undefined {
  return isSkinHash(skin) ? `${SKIN_TEXTURE_ORIGIN}/texture/${skin}` : undefined;
}

/** RGBA pixels, row by row (as `ImageData` keeps them). */
export interface Pixels {
  width: number;
  height: number;
  data: Uint8ClampedArray;
}

const alphaAt = (x: number, y: number) => (y * 64 + x) * 4 + 3;

/**
 * A downloaded skin as the game keeps it (`SkinTextureDownloader.processLegacySkin`): 64 x 64,
 * a 64 x 32 one moved into the top half; the head's own layer opaque; on a 64 x 32 skin the hat
 * area see-through when that whole area is opaque (Notch's old skins). Any other size: null (the
 * game drops it and draws the default skin; the view keeps its plain head).
 */
export function processSkin(image: Pixels): Pixels | null {
  const { width, height } = image;
  if (width !== 64 || (height !== 64 && height !== 32)) return null;
  if (image.data.length < width * height * 4) return null;
  const data = new Uint8ClampedArray(64 * 64 * 4);
  data.set(image.data.subarray(0, 64 * height * 4));
  const noAlpha = (x0: number, y0: number, x1: number, y1: number) => {
    for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) data[alphaAt(x, y)] = 255;
  };
  noAlpha(0, 0, 32, 16);
  if (height === 32) {
    let opaque = true;
    for (let y = 0; y < 32 && opaque; y++) {
      for (let x = 32; x < 64; x++) {
        if ((data[alphaAt(x, y)] ?? 0) < 128) {
          opaque = false;
          break;
        }
      }
    }
    if (opaque) {
      for (let y = 0; y < 32; y++) for (let x = 32; x < 64; x++) data[alphaAt(x, y)] = 0;
    }
  }
  noAlpha(0, 16, 64, 32);
  noAlpha(16, 48, 48, 64);
  return { width: 64, height: 64, data };
}

type V3 = [number, number, number];
/** A 4 x 4 matrix, row by row, acting on column vectors (pl-items' `Mat4`). */
type M4 = number[];

const IDENTITY: M4 = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

function mul(a: M4, b: M4): M4 {
  const out = new Array<number>(16).fill(0);
  for (let r = 0; r < 4; r++) {
    for (let c = 0; c < 4; c++) {
      let sum = 0;
      for (let k = 0; k < 4; k++) sum += (a[r * 4 + k] ?? 0) * (b[k * 4 + c] ?? 0);
      out[r * 4 + c] = sum;
    }
  }
  return out;
}

function apply(m: M4, p: V3): V3 {
  const at = (i: number) => m[i] ?? 0;
  return [
    at(0) * p[0] + at(1) * p[1] + at(2) * p[2] + at(3),
    at(4) * p[0] + at(5) * p[1] + at(6) * p[2] + at(7),
    at(8) * p[0] + at(9) * p[1] + at(10) * p[2] + at(11),
  ];
}

const translation = ([x, y, z]: V3): M4 => [1, 0, 0, x, 0, 1, 0, y, 0, 0, 1, z, 0, 0, 0, 1];

/** A turn about one axis (0 x, 1 y, 2 z), in radians. */
function rotation(axis: 0 | 1 | 2, radians: number): M4 {
  const s = Math.sin(radians);
  const c = Math.cos(radians);
  const m = [...IDENTITY];
  if (axis === 0) [m[5], m[6], m[9], m[10]] = [c, -s, s, c];
  else if (axis === 1) [m[0], m[2], m[8], m[10]] = [c, s, -s, c];
  else [m[0], m[1], m[4], m[5]] = [c, -s, s, c];
  return m;
}

const deg = (d: number) => (d * Math.PI) / 180;
const sub = (a: V3, b: V3): V3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: V3, b: V3): V3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
function normalize(v: V3): V3 {
  const len = Math.hypot(v[0], v[1], v[2]);
  return len === 0 ? v : [v[0] / len, v[1] / len, v[2] / len];
}

// items/player_head.json's transformation: translation (0.5, 0, 0.5), left rotation (1, 0, 0, -0)
// (a half turn about x), scale 1.
const PLACE: M4 = [1, 0, 0, 0.5, 0, -1, 0, 0, 0, 0, -1, 0.5, 0, 0, 0, 1];

// template_skull's gui display (rotation [30, 45, 0], translation [0, 3, 0], scale 1) as
// render.rs `view_matrix`: translate, turn X then Y then Z, scale, centre the block.
const VIEW: M4 = mul(
  mul(
    translation([0, 3 / 16, 0]),
    mul(mul(rotation(0, deg(30)), rotation(1, deg(45))), rotation(2, 0)),
  ),
  translation([-0.5, -0.5, -0.5]),
);

// render.rs `gui_lights(false)` (template_skull has no `gui_light`: side lit).
const LIGHTS: [V3, V3] = (() => {
  const flat = mul(rotation(1, deg(-22.5)), rotation(0, deg(135)));
  const side = mul(mul(rotation(1, 1.0821041), rotation(0, 3.2375858)), flat);
  const a = apply(side, normalize([0.2, 1, -0.7]));
  const b = apply(side, normalize([-0.2, 1, 0.7]));
  return [a, b];
})();

function light(normal: V3): number {
  const facing = Math.max(0, dot(LIGHTS[0], normal)) + Math.max(0, dot(LIGHTS[1], normal));
  return Math.min(1, 0.4 + 0.6 * facing);
}

interface Quad {
  corners: [V3, V3, V3, V3];
  uvs: [[number, number], [number, number], [number, number], [number, number]];
}

/** A cuboid's faces (special.rs `cuboid_quads`): `ModelPart.Cube`'s corner order and texture layout. */
function cuboid(tex: [number, number], inflate: number): Quad[] {
  const origin: V3 = [-4, -8, -4];
  const [w, h, d] = [8, 8, 8];
  const [x0, y0, z0] = [origin[0] - inflate, origin[1] - inflate, origin[2] - inflate];
  const [x1, y1, z1] = [origin[0] + w + inflate, origin[1] + h + inflate, origin[2] + d + inflate];
  const v7: V3 = [x0, y0, z0];
  const v0: V3 = [x1, y0, z0];
  const v1: V3 = [x1, y1, z0];
  const v2: V3 = [x0, y1, z0];
  const v3: V3 = [x0, y0, z1];
  const v4: V3 = [x1, y0, z1];
  const v5: V3 = [x1, y1, z1];
  const v6: V3 = [x0, y1, z1];
  const [u, v] = tex;
  const [u0, u1, u2, u3, u4, u5] = [
    u,
    u + d,
    u + d + w,
    u + d + w + w,
    u + d + w + d,
    u + d + w + d + w,
  ];
  const [va, vb, vc] = [v, v + d, v + d + h];
  const faces: [[V3, V3, V3, V3], number, number, number, number][] = [
    [[v4, v3, v7, v0], u1, va, u2, vb], // down
    [[v1, v2, v6, v5], u2, vb, u3, va], // up
    [[v7, v3, v6, v2], u0, vb, u1, vc], // west
    [[v0, v7, v2, v1], u1, vb, u2, vc], // north
    [[v4, v0, v1, v5], u2, vb, u4, vc], // east
    [[v3, v4, v5, v6], u4, vb, u5, vc], // south
  ];
  return faces.map(([corners, ua, vaa, ub, vbb]) => ({
    corners: corners.map((p) => apply(PLACE, [p[0] / 16, p[1] / 16, p[2] / 16])) as Quad["corners"],
    uvs: [
      [ub, vaa],
      [ua, vaa],
      [ua, vbb],
      [ub, vbb],
    ],
  }));
}

/** The player head's quads: the 8-pixel head, then the hat layer a quarter pixel out. */
const HEAD_QUADS: Quad[] = [...cuboid([0, 0], 0), ...cuboid([32, 0], 0.25)];

const edge = (a: [number, number], b: [number, number], p: [number, number]) =>
  (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);

/** `top` over `bottom`, straight alpha (image.rs `over`). */
function over(out: Uint8ClampedArray, at: number, top: [number, number, number, number]): void {
  const ta = top[3] / 255;
  if (ta >= 1) {
    out.set(top, at);
    return;
  }
  if (ta <= 0) return;
  const ba = (out[at + 3] ?? 0) / 255;
  const a = ta + ba * (1 - ta);
  if (a <= 0) {
    out.set([0, 0, 0, 0], at);
    return;
  }
  for (let i = 0; i < 3; i++) {
    const mixed = ((top[i] ?? 0) * ta + (out[at + i] ?? 0) * ba * (1 - ta)) / a;
    out[at + i] = Math.min(255, Math.max(0, Math.round(mixed)));
  }
  out[at + 3] = Math.round(a * 255);
}

/**
 * The head in a `size` x `size` icon (32 or 48, the atlas's 3D drawings), from a skin
 * `processSkin` returned: render.rs `rasterize`, faces facing away left out, depth-tested, opaque
 * texels writing depth and see-through ones laid over, sampled nearest-neighbour and lit.
 */
export function renderHead(skin: Pixels, size: number): Pixels {
  const n = size;
  const out = new Uint8ClampedArray(n * n * 4);
  const depth = new Float64Array(n * n).fill(Number.NEGATIVE_INFINITY);
  const texel = (u: number, v: number): [number, number, number, number] => {
    const tx = Math.min(Math.floor(Math.max(0, Math.floor(u))), skin.width - 1);
    const ty = Math.min(Math.floor(Math.max(0, Math.floor(v))), skin.height - 1);
    const i = (ty * skin.width + tx) * 4;
    return [skin.data[i] ?? 0, skin.data[i + 1] ?? 0, skin.data[i + 2] ?? 0, skin.data[i + 3] ?? 0];
  };
  for (const quad of HEAD_QUADS) {
    const world = quad.corners.map((p) => apply(VIEW, p)) as Quad["corners"];
    const normal = cross(sub(world[1], world[0]), sub(world[2], world[0]));
    if (normal[2] <= 1e-6) continue;
    const shade = light(normalize(normal));
    const screen = world.map((p) => [(p[0] + 0.5) * n, (0.5 - p[1]) * n] as [number, number]);
    for (const tri of [
      [0, 1, 2],
      [0, 2, 3],
    ] as const) {
      const a = screen[tri[0]] as [number, number];
      const b = screen[tri[1]] as [number, number];
      const c = screen[tri[2]] as [number, number];
      const area = edge(a, b, c);
      if (Math.abs(area) < 1e-6) continue;
      const minX = Math.max(0, Math.floor(Math.min(a[0], b[0], c[0])));
      const maxX = Math.min(n, Math.ceil(Math.max(a[0], b[0], c[0])));
      const minY = Math.max(0, Math.floor(Math.min(a[1], b[1], c[1])));
      const maxY = Math.min(n, Math.ceil(Math.max(a[1], b[1], c[1])));
      for (let y = minY; y < maxY; y++) {
        for (let x = minX; x < maxX; x++) {
          const p: [number, number] = [x + 0.5, y + 0.5];
          const w0 = edge(b, c, p) / area;
          const w1 = edge(c, a, p) / area;
          const w2 = edge(a, b, p) / area;
          if (w0 < -1e-5 || w1 < -1e-5 || w2 < -1e-5) continue;
          const z = w0 * world[tri[0]][2] + w1 * world[tri[1]][2] + w2 * world[tri[2]][2];
          const slot = y * n + x;
          if (z < (depth[slot] ?? 0) - 1e-5) continue;
          const uv0 = quad.uvs[tri[0]];
          const uv1 = quad.uvs[tri[1]];
          const uv2 = quad.uvs[tri[2]];
          const px = texel(
            w0 * uv0[0] + w1 * uv1[0] + w2 * uv2[0],
            w0 * uv0[1] + w1 * uv1[1] + w2 * uv2[1],
          );
          if (px[3] === 0) continue;
          for (let i = 0; i < 3; i++) {
            px[i] = Math.min(255, Math.max(0, Math.round((px[i] ?? 0) * shade)));
          }
          if (px[3] === 255) {
            out.set(px, slot * 4);
            depth[slot] = z;
          } else {
            over(out, slot * 4, px);
          }
        }
      }
    }
  }
  return { width: n, height: n, data: out };
}
