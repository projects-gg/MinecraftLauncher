// Browser preview of `api.skins`: procedurally painted skins and capes, kept in memory.
//
// Preview switch for the skin history (read when the call runs): `skinHistory=<scenario>` in the
// page URL's query or in the route's (`#/settings?section=accounts&skinHistory=few`).
// "none": only the skin worn now (the section stays hidden); "few": four skins on one page;
// "slow": every picture still has to be drawn and takes a while; "fail": wearing fails;
// "ratelimit": wearing is refused as too frequent. Default: 23 skins on three pages.
import { mockDroppedFile } from "./mock-conveniences";
import { pick } from "./mock-lang";
import type {
  CapeOption,
  SkinAppearance,
  SkinFile,
  SkinHistory,
  SkinHistoryEntry,
  SkinsApi,
  SkinVariant,
} from "./skins-types";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Same shape as the app's command errors, so the UI shows the friendly message. */
class MockSkinError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(message: string, code = "SKIN-DOSYA") {
    super(message);
    this.code = code;
    this.details = `Kod: ${code}\nÖnizleme verisi`;
  }
}

// The errors the calls below share, worded like the backend (`pl_auth::AuthError::user_message`).
const notPng = () =>
  new MockSkinError(
    pick(
      "Bu dosya PNG değil. Skin olarak 64×64 ya da 64×32 boyutunda bir PNG dosyası seç.",
      "This file isn't a PNG. Pick a 64×64 or 64×32 PNG file as the skin.",
    ),
  );
const unreadable = () =>
  new MockSkinError(
    pick(
      "Skin dosyası okunamadı. Dosyanın yerinde olduğundan emin olup tekrar dene.",
      "Couldn't read the skin file. Make sure it's still there and try again.",
    ),
  );
const notInHistory = () =>
  new MockSkinError(
    pick("Bu skin artık geçmişte yok.", "This skin isn't in your history anymore."),
    "SKIN-GECMIS",
  );

interface SkinPalette {
  hair: string;
  skin: string;
  eyes: string;
  shirt: string;
  trim: string;
  pants: string;
  shoes: string;
}

type Rect = [x: number, y: number, w: number, h: number];

/** Deterministic noise so painted textures look like pixel art, not flat fills. */
function shade(color: string, x: number, y: number): string {
  const n = Math.sin(x * 12.9898 + y * 78.233) * 43758.5453;
  const delta = Math.round(((n - Math.floor(n)) * 2 - 1) * 14);
  const value = Number.parseInt(color.slice(1), 16);
  const channel = (shift: number) =>
    Math.max(0, Math.min(255, ((value >> shift) & 255) + delta))
      .toString(16)
      .padStart(2, "0");
  return `#${channel(16)}${channel(8)}${channel(0)}`;
}

function paint(ctx: CanvasRenderingContext2D, [x0, y0, w, h]: Rect, color: string) {
  for (let y = y0; y < y0 + h; y++) {
    for (let x = x0; x < x0 + w; x++) {
      ctx.fillStyle = shade(color, x, y);
      ctx.fillRect(x, y, 1, 1);
    }
  }
}

function canvas(width: number, height: number) {
  const element = document.createElement("canvas");
  element.width = width;
  element.height = height;
  const ctx = element.getContext("2d");
  if (!ctx) throw new Error("2D canvas unavailable");
  return { element, ctx };
}

/** A 64×64 classic skin in the standard layout. */
function paintSkinCanvas(p: SkinPalette): HTMLCanvasElement {
  const { element, ctx } = canvas(64, 64);
  // Head: hair on top and the back, face on the front.
  paint(ctx, [8, 0, 8, 8], p.hair);
  paint(ctx, [16, 0, 8, 8], p.skin);
  paint(ctx, [0, 8, 32, 8], p.skin);
  paint(ctx, [0, 8, 32, 2], p.hair);
  paint(ctx, [24, 8, 8, 7], p.hair);
  paint(ctx, [0, 10, 3, 3], p.hair);
  paint(ctx, [21, 10, 3, 3], p.hair);
  for (const x of [9, 13]) {
    paint(ctx, [x, 12, 2, 1], "#f3ebdd");
    paint(ctx, [x + (x === 9 ? 1 : 0), 12, 1, 1], p.eyes);
  }
  paint(ctx, [11, 14, 2, 1], "#6b3a2a");
  // Body, arms and legs.
  paint(ctx, [20, 16, 16, 4], p.shirt);
  paint(ctx, [16, 20, 24, 12], p.shirt);
  paint(ctx, [16, 29, 24, 1], p.trim);
  paint(ctx, [23, 20, 2, 9], p.trim);
  for (const [x, y] of [
    [40, 16],
    [32, 48],
  ] as const) {
    paint(ctx, [x + 4, y, 4, 4], p.shirt);
    paint(ctx, [x + 8, y, 4, 4], p.skin);
    paint(ctx, [x, y + 4, 16, 8], p.shirt);
    paint(ctx, [x, y + 11, 16, 1], p.trim);
    paint(ctx, [x, y + 12, 16, 4], p.skin);
  }
  for (const [x, y] of [
    [0, 16],
    [16, 48],
  ] as const) {
    paint(ctx, [x + 4, y, 4, 4], p.pants);
    paint(ctx, [x + 8, y, 4, 4], p.shoes);
    paint(ctx, [x, y + 4, 16, 9], p.pants);
    paint(ctx, [x, y + 13, 16, 3], p.shoes);
  }
  return element;
}

function paintSkin(p: SkinPalette): string {
  return paintSkinCanvas(p).toDataURL("image/png");
}

/** A 64×32 cape: plain cloth with a simple emblem on the outside. */
function paintCape(base: string, accent: string): string {
  const { element, ctx } = canvas(64, 32);
  paint(ctx, [0, 0, 22, 17], base);
  paint(ctx, [1, 1, 10, 1], accent);
  paint(ctx, [1, 16, 10, 1], accent);
  for (let i = 0; i < 4; i++) {
    paint(ctx, [5 - i, 5 + i, 2 + i * 2, 1], accent);
    paint(ctx, [5 - i, 12 - i, 2 + i * 2, 1], accent);
  }
  return element.toDataURL("image/png");
}

/**
 * The whole player from the front, 16×32, as the app's backend draws the history's pictures:
 * the inner layer, then the outer one over it; slim skins have 3 px arms.
 */
function frontView(source: CanvasImageSource, variant: SkinVariant): string {
  const { element, ctx } = canvas(16, 32);
  ctx.imageSmoothingEnabled = false;
  const arm = variant === "slim" ? 3 : 4;
  const faces: Array<[sx: number, sy: number, w: number, h: number, dx: number, dy: number]> = [
    [8, 8, 8, 8, 4, 0],
    [20, 20, 8, 12, 4, 8],
    [44, 20, arm, 12, 4 - arm, 8],
    [36, 52, arm, 12, 12, 8],
    [4, 20, 4, 12, 4, 20],
    [20, 52, 4, 12, 8, 20],
    [40, 8, 8, 8, 4, 0],
    [20, 36, 8, 12, 4, 8],
    [44, 36, arm, 12, 4 - arm, 8],
    [52, 52, arm, 12, 12, 8],
    [4, 36, 4, 12, 4, 20],
    [4, 52, 4, 12, 8, 20],
  ];
  for (const [sx, sy, w, h, dx, dy] of faces) ctx.drawImage(source, sx, sy, w, h, dx, dy, w, h);
  return element.toDataURL("image/png");
}

/** The front view of a skin texture given as a URL (painted or dropped by the player). */
function frontFromUrl(url: string, variant: SkinVariant): Promise<string | null> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => {
      if (image.naturalWidth !== 64 || image.naturalHeight !== 64) {
        resolve(null);
        return;
      }
      resolve(frontView(image, variant));
    };
    image.onerror = () => resolve(null);
    image.src = url;
  });
}

const memo = new Map<string, string>();
function texture(key: string, make: () => string): string {
  let url = memo.get(key);
  if (!url) {
    url = make();
    memo.set(key, url);
  }
  return url;
}

const palettes = {
  ember: {
    hair: "#3b2417",
    skin: "#c8936b",
    eyes: "#3f0a0a",
    shirt: "#7a2416",
    trim: "#e89a2c",
    pants: "#2b2530",
    shoes: "#18141c",
  },
  knight: {
    hair: "#1d1a16",
    skin: "#b98a6a",
    eyes: "#1f4f5a",
    shirt: "#2f6f73",
    trim: "#f0b849",
    pants: "#3a3340",
    shoes: "#141218",
  },
  standard: {
    hair: "#4a2f1b",
    skin: "#b98a6a",
    eyes: "#3b3f8f",
    shirt: "#3fa6a6",
    trim: "#3fa6a6",
    pants: "#3b3f8f",
    shoes: "#4a4a4a",
  },
} satisfies Record<string, SkinPalette>;

/** `#rrggbb` of an HSL colour (h in degrees, s and l in 0–1). */
function hex(h: number, s: number, l: number): string {
  const k = (n: number) => (n + h / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  const byte = (x: number) =>
    Math.round(x * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${byte(f(0))}${byte(f(8))}${byte(f(4))}`;
}

const SKIN_TONES = ["#c8936b", "#b98a6a", "#e0b391", "#8d5a3d", "#d8a77e", "#6b4430"];

/** A distinct outfit for the n-th skin of the history. */
function historyPalette(n: number): SkinPalette {
  const hue = (n * 47 + 12) % 360;
  return {
    hair: hex((n * 29) % 360, 0.35, 0.16 + (n % 4) * 0.08),
    skin: SKIN_TONES[n % SKIN_TONES.length] ?? "#c8936b",
    eyes: hex((hue + 180) % 360, 0.5, 0.35),
    shirt: hex(hue, 0.55, 0.38 + (n % 3) * 0.06),
    trim: hex((hue + 38) % 360, 0.75, 0.6),
    pants: hex((hue + 200) % 360, 0.22, 0.22),
    shoes: hex((hue + 220) % 360, 0.12, 0.12),
  };
}

function mockCapes(): CapeOption[] {
  const cape = (id: string, name: string, base: string, accent: string, active = false) => ({
    id,
    name,
    url: texture(`cape-${id}`, () => paintCape(base, accent)),
    active,
  });
  return [
    cape("migrator", "Migrator", "#5b7fa8", "#b5d2ee", true),
    cape("vanilla", "Vanilla", "#b79f74", "#6e5532"),
    cape("cherry", "Cherry Blossom", "#e391b2", "#fde3ec"),
    cape("anniversary", "15th Anniversary", "#2e6b52", "#f0b849"),
  ];
}

function historyScenario(): string {
  if (typeof window === "undefined") return "";
  const fromSearch = new URLSearchParams(window.location.search).get("skinHistory");
  const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get("skinHistory");
  return fromSearch ?? fromHash ?? "";
}

interface MockHistory {
  entries: SkinHistoryEntry[];
  currentId: string | null;
  /** Removed moments ago, for undo. */
  removed: Map<string, SkinHistoryEntry>;
}

/** `api.skins` for the browser preview. `username` resolves the mock account list. */
export function createMockSkins(username: (accountId: string) => string | undefined): SkinsApi {
  const profiles = new Map<string, SkinAppearance>();
  const histories = new Map<string, MockHistory>();
  /** Picked and dropped files, by their (fake) path. */
  const files = new Map<string, SkinFile>();

  const profile = (accountId: string): SkinAppearance => {
    const existing = profiles.get(accountId);
    if (existing) return existing;
    const name = username(accountId);
    if (!name) throw new Error(`mock account ${accountId} not found`);
    const created: SkinAppearance = {
      accountId,
      username: name,
      skin: {
        url: texture("skin-ember", () => paintSkin(palettes.ember)),
        variant: "classic",
        isDefault: false,
      },
      // A freshly added account has no capes: shows the empty state.
      capes: accountId === "ms-new" ? [] : mockCapes(),
      stale: false,
    };
    profiles.set(accountId, created);
    return created;
  };

  const update = (accountId: string, change: (current: SkinAppearance) => SkinAppearance) => {
    const next = change(profile(accountId));
    profiles.set(accountId, next);
    return { ...next };
  };

  const historyOf = (accountId: string): MockHistory => {
    const existing = histories.get(accountId);
    if (existing) return existing;
    const scenario = historyScenario();
    const now = Date.now();
    const day = 86_400_000;
    const current: SkinHistoryEntry = {
      id: "ember",
      url: texture("skin-ember", () => paintSkin(palettes.ember)),
      variant: "classic",
      firstSeen: new Date(now - 40 * day).toISOString(),
      lastWorn: new Date(now - 60_000).toISOString(),
      favorite: false,
      thumbUrl: scenario === "slow" ? null : frontView(paintSkinCanvas(palettes.ember), "classic"),
    };
    const count = accountId === "ms-new" || scenario === "none" ? 0 : scenario === "few" ? 3 : 22;
    const older = Array.from({ length: count }, (_, n): SkinHistoryEntry => {
      const variant: SkinVariant = n % 4 === 1 ? "slim" : "classic";
      const key = `history-${n}`;
      const worn = now - (n + 1) * 2.6 * day - (n % 3) * 3_600_000;
      return {
        id: `h${n}`,
        url: texture(key, () => paintSkin(historyPalette(n))),
        variant,
        firstSeen: new Date(worn - 9 * day).toISOString(),
        lastWorn: new Date(worn).toISOString(),
        favorite: n === 4 || n === 13,
        // The two most recent still need their pictures, as after a change made elsewhere.
        thumbUrl:
          scenario === "slow" || n < 2
            ? null
            : frontView(paintSkinCanvas(historyPalette(n)), variant),
      };
    });
    const created: MockHistory = {
      entries: [current, ...older],
      currentId: "ember",
      removed: new Map(),
    };
    histories.set(accountId, created);
    return created;
  };

  /** Favourites first, then the most recently worn (as the app sorts). */
  const snapshot = (accountId: string): SkinHistory => {
    const history = historyOf(accountId);
    const entries = [...history.entries].sort(
      (a, b) =>
        Number(b.favorite) - Number(a.favorite) || Date.parse(b.lastWorn) - Date.parse(a.lastWorn),
    );
    return { entries: entries.map((e) => ({ ...e })), currentId: history.currentId };
  };

  /** What a profile change through the launcher records in the history. */
  const recordWorn = (accountId: string, url: string, variant: SkinVariant) => {
    const history = historyOf(accountId);
    const now = new Date().toISOString();
    const known = history.entries.find((e) => e.url === url);
    if (known) {
      known.lastWorn = now;
      if (known.variant !== variant) {
        known.variant = variant;
        known.thumbUrl = null;
      }
      history.currentId = known.id;
      return;
    }
    const id = `u${Date.now().toString(36)}`;
    history.entries.push({
      id,
      url,
      variant,
      firstSeen: now,
      lastWorn: now,
      favorite: false,
      thumbUrl: null,
    });
    history.currentId = id;
  };

  /** Checks a dropped image like the app does (64×64 or 64×32 PNG). */
  const readDropped = (file: File): Promise<SkinFile> =>
    new Promise((resolve, reject) => {
      if (file.type && file.type !== "image/png") {
        reject(notPng());
        return;
      }
      const reader = new FileReader();
      reader.onerror = () => reject(unreadable());
      reader.onload = () => {
        const dataUrl = String(reader.result);
        const image = new Image();
        image.onerror = () => reject(notPng());
        image.onload = () => {
          const { naturalWidth: width, naturalHeight: height } = image;
          if (width !== 64 || (height !== 64 && height !== 32)) {
            reject(
              new MockSkinError(
                pick(
                  "Skin 64×64 ya da 64×32 piksel olmalı.",
                  "A skin must be 64×64 or 64×32 pixels.",
                ),
              ),
            );
            return;
          }
          resolve({ path: `C:/Mock/${file.name}`, name: file.name, dataUrl, width, height });
        };
        image.src = dataUrl;
      };
      reader.readAsDataURL(file);
    });

  return {
    async appearance(accountId) {
      await delay(450);
      return { ...profile(accountId) };
    },
    async texture(url) {
      await delay(120);
      return url;
    },
    async pickFile() {
      await delay(150);
      const path = "C:/Users/Oyuncu/Downloads/ejder-sovalyesi.png";
      files.set(path, {
        path,
        name: "ejder-sovalyesi.png",
        dataUrl: texture("skin-knight", () => paintSkin(palettes.knight)),
        width: 64,
        height: 64,
      });
      return path;
    },
    async readFile(path) {
      await delay(150);
      const known = files.get(path);
      if (known) return known;
      // A real file dropped onto the browser preview is checked like the app does; a scripted
      // drag (`__plMockDrag`) of a ".png" path gets a painted skin.
      const dropped = mockDroppedFile(path);
      const name = path.split(/[\\/]/).pop() ?? path;
      const read = dropped
        ? await readDropped(dropped)
        : /\.png$/i.test(path)
          ? {
              path,
              name,
              dataUrl: texture("skin-knight", () => paintSkin(palettes.knight)),
              width: 64,
              height: 64,
            }
          : null;
      if (!read) throw unreadable();
      files.set(path, read);
      return read;
    },
    async upload(accountId, path, variant) {
      await delay(900);
      const file = files.get(path);
      if (!file) throw unreadable();
      recordWorn(accountId, file.dataUrl, variant);
      return update(accountId, (current) => ({
        ...current,
        skin: { url: file.dataUrl, variant, isDefault: false },
        stale: false,
      }));
    },
    async setVariant(accountId, variant: SkinVariant) {
      await delay(600);
      const next = update(accountId, (current) => ({
        ...current,
        skin: current.skin ? { ...current.skin, variant, isDefault: false } : null,
      }));
      if (next.skin) recordWorn(accountId, next.skin.url, variant);
      return next;
    },
    async setCape(accountId, capeId) {
      await delay(500);
      return update(accountId, (current) => ({
        ...current,
        capes: current.capes.map((c) => ({ ...c, active: c.id === capeId })),
      }));
    },
    async reset(accountId) {
      await delay(700);
      historyOf(accountId).currentId = null;
      return update(accountId, (current) => ({
        ...current,
        skin: {
          url: texture("skin-standard", () => paintSkin(palettes.standard)),
          variant: "classic",
          isDefault: true,
        },
      }));
    },
    async history(accountId) {
      await delay(60);
      return snapshot(accountId);
    },
    async prepareHistory(accountId) {
      await delay(historyScenario() === "slow" ? 1400 : 300);
      let made = 0;
      for (const entry of historyOf(accountId).entries) {
        if (entry.thumbUrl) continue;
        entry.thumbUrl = await frontFromUrl(entry.url, entry.variant);
        if (entry.thumbUrl) made++;
      }
      return made;
    },
    async wearFromHistory(accountId, entryId) {
      await delay(900);
      const scenario = historyScenario();
      if (scenario === "ratelimit") {
        throw new MockSkinError(
          pick(
            "Kısa sürede çok fazla deneme yapıldı. Birkaç dakika bekleyip tekrar dene.",
            "Too many tries in a short time. Wait a few minutes and try again.",
          ),
          "MC-SKIN-429",
        );
      }
      if (scenario === "fail") {
        throw new MockSkinError(
          pick("Skin güncellenemedi. Tekrar dene.", "Couldn't change the skin. Try again."),
          "MC-SKIN-500",
        );
      }
      const entry = historyOf(accountId).entries.find((e) => e.id === entryId);
      if (!entry) throw notInHistory();
      recordWorn(accountId, entry.url, entry.variant);
      return update(accountId, (current) => ({
        ...current,
        skin: { url: entry.url, variant: entry.variant, isDefault: false },
        stale: false,
      }));
    },
    async setHistoryFavorite(accountId, entryId, favorite) {
      await delay(150);
      const entry = historyOf(accountId).entries.find((e) => e.id === entryId);
      if (!entry) throw notInHistory();
      entry.favorite = favorite;
    },
    async removeFromHistory(accountId, entryId) {
      await delay(150);
      const history = historyOf(accountId);
      const entry = history.entries.find((e) => e.id === entryId);
      if (!entry) return;
      history.entries = history.entries.filter((e) => e.id !== entryId);
      history.removed.set(entryId, entry);
    },
    async restoreToHistory(accountId, entryId) {
      await delay(150);
      const history = historyOf(accountId);
      const entry = history.removed.get(entryId);
      if (!entry) throw notInHistory();
      history.removed.delete(entryId);
      if (!history.entries.some((e) => e.id === entryId)) history.entries.push(entry);
    },
  };
}
