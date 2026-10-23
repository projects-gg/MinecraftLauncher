// Browser preview of `projectsSkinApi` (SKIN1): every state of the Görünüm panel of a username
// account, the link code box and the account center's link row, kept in memory. Nothing leaves
// the page: premium players are local fixtures with painted skins, never Mojang.
//
// Preview switches, read from the page URL's query or the route's (`#/…?pskin=none`):
// - `pskin=<scenario>`:
//   `ready` (default): EjderAvcisi is linked, has a Projects skin, 2 of 3 changes left today;
//   `none`: not linked (the code box); `unknown`: the link is not known (a check runs, then
//   linked); `other`: the Discord account is linked to KasaBekcisi; `noskin`: linked, no Projects
//   skin yet; `queued`: a skin already waits in line; `serverAway`: Lobi is not taking the queue
//   (a save waits in line); `last`: one change left; `quota`: none left; `limited`: a new Discord
//   account (5 days to go); `blocked` / `blockedForever`: staff keep the player from changing
//   skins; `reset`: staff reset the skin, with a 7-day block; `off`: the service's skin switch is
//   off; `unreachable`: ProBot is away; `session`: the Discord session ended; `signedOut`: not
//   signed in to friends (also with `social=signedOut`); `hidden`: the signed content leaves the
//   feature off (no entry point; also with `social=off`).
// - `pskinSave=slow`: a save waits in line for 75 s ("Biraz uzun sürüyor"); `pskinSave=<result>`:
//   it fails with that result (`invalidImage`, `mineskinBusy`, `premiumInvalid`,
//   `premiumDisabled`, `timeout`, `error`, `notLinked`, `otherAccount`, `skinBlocked`);
//   `pskinSave=refuse-<code>`: the service turns it away first (`quota`, `pending`, `limited`,
//   `blocked`, `unreachable`, `session`).
// - `pskinAge=<ms>`: how long the `queued` scenario's skin has waited (20 s; 70000: a long wait).
// - `pskinFile=legacy|black|notPng|dimensions|tooBig|empty|unreadable`: what a picked file is.
// - `pskinLive=linked`: the account gets linked in game (codeless) after `pskinAt` ms (4000);
//   `pskinLive=reset`: staff reset the skin then.
// - `pskinGame=1`: the game runs on projects.gg ("Yeniden girince görünür.").
// Codes for the box: 482913 links; 111111 expired; 222222 paused; 333333 the game account is
// another Discord account's; 444444 too many tries; any other six digits are wrong.
// Premium names: AltinKartal (classic), Gecekusu (slim), Bulutcu (wears the default skin),
// Yogun (Mojang busy), EjderAvcisi (the active account's own name); any other: no account.
import { mockAccountName } from "./mock-accounts";
import { mockDroppedFile } from "./mock-conveniences";
import type {
  ProjectsLink,
  ProjectsSkin,
  ProjectsSkinApi,
  ProjectsSkinChange,
  ProjectsSkinChoice,
  ProjectsSkinJob,
  ProjectsSkinLook,
  ProjectsSkinOutcome,
  ProjectsSkinPick,
  ProjectsSkinRefusal,
  ProjectsSkinRefusalCode,
  ProjectsSkinResultCode,
  ProjectsSkinView,
  SkinVariant,
} from "./projects-skin-types";

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const nowSec = () => Math.floor(Date.now() / 1000);

function param(key: string): string | null {
  if (typeof window === "undefined") return null;
  const fromSearch = new URLSearchParams(window.location.search).get(key);
  const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
  return fromSearch ?? fromHash;
}

const scenario = () => param("pskin") ?? "ready";

// ── Painted skins ────────────────────────────────────────────────────────────────────────────

interface Palette {
  hair: string;
  skin: string;
  eyes: string;
  shirt: string;
  trim: string;
  pants: string;
  shoes: string;
}

const PALETTES = {
  /** The Projects skin EjderAvcisi has: obsidian with gold trim. */
  projects: {
    hair: "#24140d",
    skin: "#c8936b",
    eyes: "#3f0a0a",
    shirt: "#1b1820",
    trim: "#e89a2c",
    pants: "#27222c",
    shoes: "#121016",
  },
  /** The one before it ("Geri al"). */
  previous: {
    hair: "#3a2a1c",
    skin: "#b98a6a",
    eyes: "#1f4f5a",
    shirt: "#2f5f8f",
    trim: "#cfe3f5",
    pants: "#2a2f3c",
    shoes: "#15161c",
  },
  /** "ejder-sovalyesi.png". */
  knight: {
    hair: "#1d1a16",
    skin: "#b98a6a",
    eyes: "#1f4f5a",
    shirt: "#2f6f73",
    trim: "#f0b849",
    pants: "#3a3340",
    shoes: "#141218",
  },
  altinKartal: {
    hair: "#5a2a10",
    skin: "#d8a77e",
    eyes: "#2b4a1f",
    shirt: "#8c1f14",
    trim: "#f6d58e",
    pants: "#3f0a0a",
    shoes: "#1a0f05",
  },
  gecekusu: {
    hair: "#0e0c14",
    skin: "#e0b391",
    eyes: "#6a3fb0",
    shirt: "#2c2240",
    trim: "#9b7fd6",
    pants: "#191424",
    shoes: "#0a090c",
  },
  ownPremium: {
    hair: "#2c1d10",
    skin: "#c8936b",
    eyes: "#1e5a3a",
    shirt: "#2f7a4f",
    trim: "#cfe8b0",
    pants: "#2b2a22",
    shoes: "#16140f",
  },
  /** Minecraft's default skin, near enough. */
  standard: {
    hair: "#4a2f1b",
    skin: "#b98a6a",
    eyes: "#3b3f8f",
    shirt: "#3fa6a6",
    trim: "#3fa6a6",
    pants: "#3b3f8f",
    shoes: "#4a4a4a",
  },
} satisfies Record<string, Palette>;

/** A see-through pixel's colour in the base layer: Minecraft draws it opaque, mostly black. */
const HOLE = "#000000";

function shade(color: string, x: number, y: number): string {
  const n = Math.sin(x * 12.9898 + y * 78.233) * 43758.5453;
  const delta = Math.round(((n - Math.floor(n)) * 2 - 1) * 12);
  const value = Number.parseInt(color.slice(1), 16);
  const channel = (shift: number) =>
    Math.max(0, Math.min(255, ((value >> shift) & 255) + delta))
      .toString(16)
      .padStart(2, "0");
  return `#${channel(16)}${channel(8)}${channel(0)}`;
}

type Rect = [x: number, y: number, w: number, h: number];

function paintSkin(p: Palette, options: { holes?: boolean; height?: 64 | 32 } = {}): string {
  if (typeof document === "undefined") return "";
  const height = options.height ?? 64;
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  const fill = ([x0, y0, w, h]: Rect, color: string) => {
    for (let y = y0; y < y0 + h; y++) {
      for (let x = x0; x < x0 + w; x++) {
        if (y >= height) continue;
        ctx.fillStyle = color === HOLE ? HOLE : shade(color, x, y);
        ctx.fillRect(x, y, 1, 1);
      }
    }
  };
  // Head: hair on top and the back, the face in front.
  fill([8, 0, 8, 8], p.hair);
  fill([16, 0, 8, 8], p.skin);
  fill([0, 8, 32, 8], p.skin);
  fill([0, 8, 32, 2], p.hair);
  fill([24, 8, 8, 7], p.hair);
  for (const x of [9, 13]) {
    fill([x, 12, 2, 1], "#f3ebdd");
    fill([x + (x === 9 ? 1 : 0), 12, 1, 1], p.eyes);
  }
  fill([11, 14, 2, 1], "#6b3a2a");
  // Body, arms and legs (the left ones only in a 64×64 skin).
  fill([20, 16, 16, 4], p.shirt);
  fill([16, 20, 24, 12], p.shirt);
  fill([16, 29, 24, 1], p.trim);
  fill([23, 20, 2, 9], p.trim);
  const limbs =
    height === 64
      ? ([
          [40, 16],
          [32, 48],
        ] as const)
      : ([[40, 16]] as const);
  for (const [x, y] of limbs) {
    fill([x + 4, y, 4, 4], p.shirt);
    fill([x + 8, y, 4, 4], p.skin);
    fill([x, y + 4, 16, 8], p.shirt);
    fill([x, y + 11, 16, 1], p.trim);
    fill([x, y + 12, 16, 4], p.skin);
  }
  const legs =
    height === 64
      ? ([
          [0, 16],
          [16, 48],
        ] as const)
      : ([[0, 16]] as const);
  for (const [x, y] of legs) {
    fill([x + 4, y, 4, 4], p.pants);
    fill([x + 8, y, 4, 4], p.shoes);
    fill([x, y + 4, 16, 9], p.pants);
    fill([x, y + 13, 16, 3], p.shoes);
  }
  // See-through patches on the chest and a leg: black in game.
  if (options.holes) {
    fill([21, 22, 6, 5], HOLE);
    fill([4, 24, 3, 6], HOLE);
  }
  return canvas.toDataURL("image/png");
}

/** A 64×32 skin turned into 64×64 the way Minecraft does (right limbs copied to the left). */
function widenLegacy(source: string): Promise<string> {
  return new Promise((resolve) => {
    if (typeof document === "undefined") return resolve(source);
    const image = new Image();
    image.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = 64;
      canvas.height = 64;
      const ctx = canvas.getContext("2d");
      if (!ctx) return resolve(source);
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(image, 0, 0);
      ctx.drawImage(image, 0, 16, 16, 16, 16, 48, 16, 16);
      ctx.drawImage(image, 40, 16, 16, 16, 32, 48, 16, 16);
      resolve(canvas.toDataURL("image/png"));
    };
    image.onerror = () => resolve(source);
    image.src = source;
  });
}

const painted = new Map<string, string>();
function picture(key: keyof typeof PALETTES, options: { holes?: boolean } = {}): string {
  const id = `${key}${options.holes ? "-holes" : ""}`;
  let url = painted.get(id);
  if (!url) {
    url = paintSkin(PALETTES[key], options);
    painted.set(id, url);
  }
  return url;
}

// ── State ────────────────────────────────────────────────────────────────────────────────────

interface MockState {
  link: ProjectsLink;
  skin: ProjectsSkin | null;
  /** The skin before the last change, for "Geri al". */
  previous: ProjectsSkin | null;
  perDay: number;
  left: number;
  linkTriesLeft: number;
  linkRetryAt: number | null;
  job: (ProjectsSkinJob & { accountId: string }) | null;
  lastJob: ProjectsSkinJob | null;
  linkJob: ProjectsSkinJob | null;
  blocked: { until: number | null } | null;
  limited: { until: number } | null;
  serverAway: boolean;
}

const ACTIVE = "EjderAvcisi";

function projectsSkin(): ProjectsSkin {
  return {
    kind: "png",
    variant: "classic",
    premium: null,
    at: nowSec() - 3 * 86_400,
    by: "launcher",
    picture: picture("projects"),
  };
}

function initial(): MockState {
  const s = scenario();
  const linked: ProjectsLink = {
    state: "linked",
    name: ACTIVE,
    at: nowSec() - 40 * 86_400,
    checkedAt: nowSec() - 3600,
  };
  const base: MockState = {
    link: linked,
    skin: projectsSkin(),
    previous: {
      kind: "png",
      variant: "classic",
      premium: null,
      at: nowSec() - 9 * 86_400,
      by: "launcher",
      picture: picture("previous"),
    },
    perDay: 3,
    left: 2,
    linkTriesLeft: 5,
    linkRetryAt: null,
    job: null,
    lastJob: null,
    linkJob: null,
    blocked: null,
    limited: null,
    serverAway: false,
  };
  switch (s) {
    case "none":
    case "signedOut":
      return {
        ...base,
        link: { state: "none", name: null, at: null, checkedAt: nowSec() - 60 },
        skin: null,
        previous: null,
      };
    case "unknown":
      return { ...base, link: { ...linked, state: "unknown", checkedAt: null } };
    case "other":
      return {
        ...base,
        link: { ...linked, name: "KasaBekcisi" },
        skin: null,
        previous: null,
      };
    case "noskin":
      return { ...base, skin: null, previous: null, left: 3 };
    case "last":
      return { ...base, left: 1 };
    case "quota":
      return { ...base, left: 0 };
    case "limited":
      return {
        ...base,
        skin: null,
        previous: null,
        left: 3,
        limited: { until: nowSec() + 5 * 86_400 },
      };
    case "blocked":
      return { ...base, blocked: { until: nowSec() + 7 * 86_400 } };
    case "blockedForever":
      return { ...base, blocked: { until: null } };
    case "reset":
      return {
        ...base,
        skin: {
          kind: "default",
          variant: "classic",
          premium: null,
          at: nowSec() - 600,
          by: "staff",
          picture: null,
        },
        previous: null,
        blocked: { until: nowSec() + 7 * 86_400 },
      };
    case "serverAway":
      return { ...base, serverAway: true };
    case "queued":
      return {
        ...base,
        job: {
          ...openJob("skin", ACTIVE, "png"),
          accountId: "a1",
          state: "queued",
          picture: picture("knight"),
          variant: "classic",
          createdAtMs: Date.now() - Number(param("pskinAge") ?? 20_000),
        },
      };
    default:
      return base;
  }
}

let state: MockState | null = null;
const current = (): MockState => {
  state ??= initial();
  return state;
};
const listeners = new Set<(change: ProjectsSkinChange) => void>();
const emit = (change: ProjectsSkinChange) => {
  for (const listener of listeners) listener(change);
};

let jobSeq = 0;
function openJob(
  kind: ProjectsSkinJob["kind"],
  name: string | null,
  source: ProjectsSkinJob["source"] = null,
): ProjectsSkinJob {
  jobSeq += 1;
  return {
    id: `jmock${jobSeq}`,
    kind,
    state: "pending",
    name,
    source,
    result: null,
    picture: null,
    variant: null,
    createdAtMs: Date.now(),
    updatedAtMs: Date.now(),
  };
}

const closed = (
  job: ProjectsSkinJob,
  code: ProjectsSkinResultCode,
  extra: { name?: string; until?: number | null } = {},
): ProjectsSkinJob => ({
  ...job,
  state: code === "cancelled" ? "cancelled" : FAILURES.has(code) ? "failed" : "done",
  result: { code, ...extra },
  picture: null,
  variant: null,
  updatedAtMs: Date.now(),
});

const FAILURES = new Set<ProjectsSkinResultCode>([
  "notLinked",
  "otherAccount",
  "minecraftTaken",
  "pinInvalid",
  "pinExpired",
  "paused",
  "skinBlocked",
  "premiumInvalid",
  "premiumDisabled",
  "invalidImage",
  "mineskinBusy",
  "superseded",
  "timeout",
  "error",
]);

function socialScenario(): string {
  return param("social") ?? "friends";
}

function access(): ProjectsSkinView["access"] {
  const s = scenario();
  if (
    s === "signedOut" ||
    ["signedOut", "signingIn", "restricted", "banned"].includes(socialScenario())
  )
    return "signedOut";
  if (s === "off") return "off";
  if (s === "unreachable") return "unreachable";
  if (s === "session") return "session";
  return "ready";
}

const nameOf = (accountId: string) => mockAccountName(accountId) ?? ACTIVE;
const sameName = (a: string | null, b: string | null) =>
  Boolean(a && b && a.toLowerCase() === b.toLowerCase());

function view(accountId: string): ProjectsSkinView {
  const s = current();
  const name = nameOf(accountId);
  const mine = s.link.state !== "none" && sameName(s.link.name, name);
  return {
    accountId,
    name,
    access: access(),
    serverAway: s.serverAway,
    link: { ...s.link },
    skin: mine && s.skin ? { ...s.skin } : null,
    job: s.job && s.job.accountId === accountId ? strip(s.job) : null,
    lastJob: s.lastJob && sameName(s.lastJob.name, name) ? { ...s.lastJob } : null,
    linkJob: s.linkJob ? { ...s.linkJob } : null,
    limits: {
      perDay: s.perDay,
      left: s.left,
      resetAt: nextTurkeyMidnight(),
      linkTriesLeft: s.linkTriesLeft,
      linkRetryAt: s.linkRetryAt,
    },
    blocked: s.blocked,
    limited: s.limited,
    canUndo: mine && s.previous !== null,
    onProjects: param("pskinGame") === "1",
  };
}

function strip(job: ProjectsSkinJob & { accountId?: string }): ProjectsSkinJob {
  const { accountId: _ignored, ...rest } = job;
  return rest;
}

function nextTurkeyMidnight(): number {
  const turkey = Date.now() + 3 * 3_600_000;
  const startOfDay = Math.floor(turkey / 86_400_000) * 86_400_000;
  return Math.floor((startOfDay + 86_400_000 - 3 * 3_600_000) / 1000);
}

const refused = (code: ProjectsSkinRefusalCode, extra: Partial<ProjectsSkinRefusal> = {}) =>
  ({ status: "refused", refusal: { code, ...extra } }) satisfies ProjectsSkinOutcome;

/** What the service turns away before any job, whatever the job is. */
function serviceRefusal(): ProjectsSkinOutcome | null {
  const a = access();
  if (a === "signedOut" || a === "session") return refused("session");
  if (a === "off") return refused("off");
  if (a === "unreachable") return refused("unreachable", { retryAfter: 60 });
  return null;
}

// ── Choices ──────────────────────────────────────────────────────────────────────────────────

const choices = new Map<string, ProjectsSkinChoice>();
let choiceSeq = 0;
function keep(choice: Omit<ProjectsSkinChoice, "id">): ProjectsSkinChoice {
  choiceSeq += 1;
  const kept = { ...choice, id: `cmock${choiceSeq}` };
  choices.set(kept.id, kept);
  return kept;
}

/** Reads a real file dropped on the browser preview the way the backend checks one. */
async function readDropped(file: File): Promise<ProjectsSkinPick> {
  if (file.size > 1024 * 1024) return { status: "refused", reason: "tooBig" };
  if (file.type !== "image/png" && !/\.png$/i.test(file.name))
    return { status: "refused", reason: "notPng" };
  const url = await new Promise<string>((resolve) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => resolve("");
    reader.readAsDataURL(file);
  });
  const size = await new Promise<{ w: number; h: number } | null>((resolve) => {
    const image = new Image();
    image.onload = () => resolve({ w: image.naturalWidth, h: image.naturalHeight });
    image.onerror = () => resolve(null);
    image.src = url;
  });
  if (!size) return { status: "refused", reason: "notPng" };
  if (size.w !== 64 || (size.h !== 64 && size.h !== 32))
    return { status: "refused", reason: "dimensions" };
  const legacy = size.h === 32;
  return {
    status: "ready",
    choice: keep({
      kind: "png",
      picture: legacy ? await widenLegacy(url) : url,
      name: file.name,
      variant: "classic",
      legacy,
      blackParts: false,
      own: false,
    }),
  };
}

// ── Premium fixtures ─────────────────────────────────────────────────────────────────────────

const PREMIUM: Record<
  string,
  { name: string; palette: keyof typeof PALETTES; variant: SkinVariant }
> = {
  altinkartal: { name: "AltinKartal", palette: "altinKartal", variant: "classic" },
  gecekusu: { name: "Gecekusu", palette: "gecekusu", variant: "slim" },
  ejderavcisi: { name: "EjderAvcisi", palette: "ownPremium", variant: "classic" },
};

// ── Jobs ─────────────────────────────────────────────────────────────────────────────────────

const saveMode = () => param("pskinSave") ?? "";

function finishSkin(job: ProjectsSkinJob & { accountId: string }, applied: ProjectsSkin | null) {
  const s = current();
  if (s.job?.id !== job.id) return;
  const forced = saveMode();
  const failure = FAILURES.has(forced as ProjectsSkinResultCode)
    ? (forced as ProjectsSkinResultCode)
    : null;
  let done: ProjectsSkinJob;
  if (failure) {
    done = closed(job, failure, {
      ...(failure === "otherAccount" ? { name: "KasaBekcisi" } : {}),
      ...(failure === "skinBlocked" ? { until: nowSec() + 7 * 86_400 } : {}),
    });
    if (failure === "skinBlocked") s.blocked = { until: nowSec() + 7 * 86_400 };
  } else {
    done = closed(job, "applied");
    s.previous = s.skin;
    s.skin = applied
      ? { ...applied, at: nowSec(), by: "launcher" }
      : {
          kind: "default",
          variant: "classic",
          premium: null,
          at: nowSec(),
          by: "launcher",
          picture: null,
        };
    s.left = Math.max(0, s.left - 1);
  }
  s.job = null;
  s.lastJob = done;
  emit({
    kind: "job",
    job: done,
    accountId: job.accountId,
    undo: !failure && s.previous !== null && s.left > 0,
    onProjects: param("pskinGame") === "1",
  });
}

function startSkinJob(
  accountId: string,
  source: "png" | "premium" | "default",
  applied: ProjectsSkin | null,
): ProjectsSkinOutcome {
  const s = current();
  const job = {
    ...openJob("skin", nameOf(accountId), source),
    accountId,
    picture: applied?.picture ?? null,
    variant: applied?.variant ?? null,
  };
  s.job = job;
  const slow = saveMode() === "slow" || s.serverAway;
  setTimeout(() => {
    if (current().job?.id === job.id)
      current().job = { ...job, state: "queued", updatedAtMs: Date.now() };
  }, 400);
  setTimeout(() => finishSkin(job, applied), slow ? 75_000 : 2400);
  return { status: "sent", job: strip(job) };
}

function startLinkJob(
  kind: "link" | "unlink" | "check",
  name: string | null,
  finish: () => ProjectsSkinJob,
) {
  const s = current();
  const job = openJob(kind, name);
  s.linkJob = job;
  setTimeout(
    () => {
      if (current().linkJob?.id !== job.id) return;
      const done = finish();
      current().linkJob = null;
      emit({ kind: "job", job: done, accountId: null, undo: false, onProjects: false });
    },
    kind === "check" ? 1200 : kind === "link" ? 1100 : 900,
  );
  return job;
}

function scheduleLive() {
  const live = param("pskinLive");
  if (!live || typeof window === "undefined") return;
  const at = Number(param("pskinAt") ?? 4000);
  setTimeout(() => {
    const s = current();
    if (live === "linked") {
      s.link = { state: "linked", name: ACTIVE, at: nowSec(), checkedAt: nowSec() };
      emit({ kind: "linked", name: ACTIVE });
    } else if (live === "reset") {
      s.previous = null;
      s.skin = {
        kind: "default",
        variant: "classic",
        premium: null,
        at: nowSec(),
        by: "staff",
        picture: null,
      };
      s.blocked = { until: nowSec() + 7 * 86_400 };
      emit({ kind: "state" });
    }
  }, at);
}
let liveScheduled = false;

// ── API ──────────────────────────────────────────────────────────────────────────────────────

export const mockProjectsSkin: ProjectsSkinApi = {
  async overview() {
    if (!liveScheduled) {
      liveScheduled = true;
      scheduleLive();
    }
    const s = current();
    const shown = scenario() !== "hidden" && socialScenario() !== "off";
    const signedIn = access() !== "signedOut";
    return {
      shown,
      service: signedIn ? { link: true, skins: scenario() !== "off" } : null,
      link: signedIn ? { ...s.link } : null,
    };
  },

  async state(accountId) {
    await delay(220);
    return view(accountId);
  },

  async defaultLook(accountId) {
    await delay(300);
    const own = PREMIUM[nameOf(accountId).toLowerCase()];
    const look: ProjectsSkinLook = own
      ? { picture: picture(own.palette), variant: own.variant, premium: true }
      : { picture: picture("standard"), variant: "classic", premium: false };
    return look;
  },

  async pickFile() {
    await delay(150);
    return "C:/Users/Oyuncu/Downloads/ejder-sovalyesi.png";
  },

  async readFile(path) {
    await delay(160);
    const dropped = mockDroppedFile(path);
    if (dropped) return readDropped(dropped);
    const name = path.split(/[\\/]/).pop() ?? path;
    const kind = param("pskinFile") ?? "";
    if (kind === "notPng" || !/\.png$/i.test(name)) return { status: "refused", reason: "notPng" };
    if (kind === "dimensions") return { status: "refused", reason: "dimensions" };
    if (kind === "tooBig") return { status: "refused", reason: "tooBig" };
    if (kind === "empty") return { status: "refused", reason: "empty" };
    if (kind === "unreadable") return { status: "refused", reason: "unreadable" };
    if (kind === "legacy") {
      const wide = await widenLegacy(paintSkin(PALETTES.knight, { height: 32 }));
      return {
        status: "ready",
        choice: keep({
          kind: "png",
          picture: wide,
          name: "eski-skin.png",
          variant: "classic",
          legacy: true,
          blackParts: false,
          own: false,
        }),
      };
    }
    return {
      status: "ready",
      choice: keep({
        kind: "png",
        picture: picture("knight", { holes: kind === "black" }),
        name,
        variant: "classic",
        legacy: false,
        blackParts: kind === "black",
        own: false,
      }),
    };
  },

  async lookup(accountId, name) {
    await delay(380);
    const key = name.toLowerCase();
    if (key === "yogun") return { status: "unreachable" };
    if (key === "bulutcu") return { status: "noSkin" };
    const found = PREMIUM[key];
    if (!found) return { status: "noAccount" };
    return {
      status: "found",
      choice: keep({
        kind: "premium",
        picture: picture(found.palette),
        name: found.name,
        variant: found.variant,
        legacy: false,
        blackParts: false,
        own: sameName(found.name, nameOf(accountId)),
      }),
    };
  },

  async save(accountId, save) {
    await delay(300);
    const forced = saveMode();
    if (forced.startsWith("refuse-")) {
      const code = forced.slice("refuse-".length) as ProjectsSkinRefusalCode;
      return refused(code, {
        ...(code === "limited" ? { until: nowSec() + 5 * 86_400 } : {}),
        ...(code === "blocked" ? { until: nowSec() + 7 * 86_400 } : {}),
        ...(code === "quota" ? { retryAfter: nextTurkeyMidnight() - nowSec() } : {}),
      });
    }
    const early = serviceRefusal();
    if (early) return early;
    const s = current();
    const name = nameOf(accountId);
    if (s.limited && s.limited.until > nowSec())
      return refused("limited", { until: s.limited.until });
    if (s.blocked && (s.blocked.until === null || s.blocked.until > nowSec()))
      return refused("blocked", { until: s.blocked.until });
    if (s.link.state === "none") return refused("notLinked");
    if (s.link.state === "linked" && !sameName(s.link.name, name))
      return refused("otherAccount", { name: s.link.name ?? undefined });
    if (s.job) return refused("pending");
    if (s.left <= 0) return refused("quota", { retryAfter: nextTurkeyMidnight() - nowSec() });

    if (save.kind === "default") return startSkinJob(accountId, "default", null);
    if (save.kind === "undo") {
      if (!s.previous) return refused("choiceGone");
      return startSkinJob(accountId, s.previous.kind, s.previous);
    }
    const choice = choices.get(save.id);
    if (!choice) return refused("choiceGone");
    const variant = choice.kind === "premium" ? choice.variant : save.variant;
    return startSkinJob(accountId, choice.kind, {
      kind: choice.kind,
      variant,
      premium: choice.kind === "premium" ? choice.name : null,
      at: nowSec(),
      by: "launcher",
      picture: choice.picture,
    });
  },

  async cancel(jobId) {
    await delay(200);
    const s = current();
    if (s.job?.id !== jobId || s.job.state !== "pending") return false;
    const done = closed(s.job, "cancelled");
    s.job = null;
    s.lastJob = done;
    emit({ kind: "state" });
    return true;
  },

  async link(accountId, pin) {
    await delay(450);
    const early = serviceRefusal();
    if (early) return early;
    const s = current();
    const name = nameOf(accountId);
    if (!/^\d{6}$/.test(pin)) return refused("invalidPin");
    if (s.link.state === "linked" && sameName(s.link.name, name))
      return refused("alreadyLinked", { name: s.link.name ?? name });
    if (s.link.state === "linked")
      return refused("otherAccount", { name: s.link.name ?? undefined });
    if (s.linkRetryAt && s.linkRetryAt > nowSec())
      return refused("tries", { retryAfter: s.linkRetryAt - nowSec() });
    if (pin === "444444" || s.linkTriesLeft <= 0) {
      s.linkRetryAt = nowSec() + 25 * 60;
      return refused("tries", { retryAfter: 25 * 60 });
    }
    s.linkTriesLeft -= 1;
    const job = startLinkJob("link", name, () => {
      const result: ProjectsSkinResultCode =
        pin === "482913"
          ? "linked"
          : pin === "111111"
            ? "pinExpired"
            : pin === "222222"
              ? "paused"
              : pin === "333333"
                ? "minecraftTaken"
                : "pinInvalid";
      if (result === "linked") {
        current().link = { state: "linked", name, at: nowSec(), checkedAt: nowSec() };
      }
      return closed(job, result);
    });
    return { status: "sent", job };
  },

  async unlink() {
    await delay(400);
    const early = serviceRefusal();
    if (early) return early;
    const s = current();
    if (s.link.state === "none") return refused("notLinked");
    const job = startLinkJob("unlink", null, () => {
      const next = current();
      next.link = { state: "none", name: null, at: null, checkedAt: nowSec() };
      next.skin = null;
      next.previous = null;
      return closed(job, "unlinked");
    });
    return { status: "sent", job };
  },

  async linkCheck() {
    await delay(120);
    const s = current();
    if (serviceRefusal() || s.linkJob) return;
    const job = startLinkJob("check", null, () => {
      const next = current();
      next.link =
        next.link.state === "unknown"
          ? {
              state: "linked",
              name: next.link.name ?? ACTIVE,
              at: next.link.at,
              checkedAt: nowSec(),
            }
          : { ...next.link, checkedAt: nowSec() };
      return closed(job, next.link.state === "linked" ? "linked" : "unlinked");
    });
  },

  onChange(listener) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};

/** Tests and previews start from the scenario again. */
export function resetMockProjectsSkin() {
  state = null;
  choices.clear();
}
