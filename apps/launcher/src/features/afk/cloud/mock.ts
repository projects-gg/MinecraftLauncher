// 7/24 Bulut for browser previews. Preview switch (read once, from the page URL): `?cloud=<scenario>`
// - (none) or `off`: the service does not answer; the AFK page looks exactly as without it;
// - `intro`: signed in, no cloud account yet (the intro card);
// - `signedout`: the service answers, the friends session is missing;
// - `soldout`: no free slots; `closed`: sales off (Studio's switch), no account yet;
// - `accounts`: three accounts (one in game, one reconnecting with its time running out, one
//   stopped for a wrong password), `Madenci07` also on this computer (one card for both);
// - `expired`: `accounts` plus one whose time is up;
// - `paused`: maintenance (Studio's note, the accounts held);
// - `failed`: signed in, the accounts could not be read.
// `&reveal=offline|retype|limited` makes "Göster" fail that way. AFK+ adds `&plus=<scenario>`
// (plus/mock-data.ts); without it AFK+ is off, as the service is until its release. The cloud
// features add `&feat=on|view` and their own switches (mock-feat.ts); without them the service is
// one from before the features. `&link=down`: the machine is not connected (a read cannot ask it, a
// console streams nothing); `window.__plMockCloudLink(online)` changes it while the page is open
// (mock-live.ts). Movement adds `&move=<scenario>` (mock-move.ts).
import { api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { enCloudAfk } from "@/locales/en-cloud-afk";
import { trCloudAfk } from "@/locales/tr-cloud-afk";
import type {
  CloudAccount,
  CloudApi,
  CloudCopyResult,
  CloudEdit,
  CloudEditData,
  CloudLiveEvent,
  CloudLogPage,
  CloudMovePlan,
  CloudPageData,
} from "./api";
import {
  accountFeatures,
  barsEvent,
  featScenario,
  inventoryMove,
  kindLines,
  liveKindLine,
  mockTryReply,
  offerFeatures,
  unwatchInventory,
  watchInventory,
} from "./mock-feat";
import { mockLive } from "./mock-live";
import {
  GECE,
  geceRunning,
  movementFeature,
  moveOperation,
  unwatchMovement,
  watchMovement,
} from "./mock-move";
import {
  mockMe as mockPlusMe,
  mockOffer as mockPlusOffer,
  PLUS_ID,
  PLUS_NAME,
  plusAccountParts,
  plusScenario,
  withPlus,
} from "./plus/mock-data";
import { serverLabel } from "./status";

type Scenario =
  | "off"
  | "intro"
  | "signedout"
  | "soldout"
  | "closed"
  | "accounts"
  | "expired"
  | "paused"
  | "failed";

function param(key: string): string | null {
  try {
    const fromSearch = new URLSearchParams(window.location.search).get(key);
    const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
    return fromSearch ?? fromHash;
  } catch {
    return null;
  }
}

const SCENARIOS: Scenario[] = [
  "off",
  "intro",
  "signedout",
  "soldout",
  "closed",
  "accounts",
  "expired",
  "paused",
  "failed",
];

function scenario(): Scenario {
  const value = param("cloud");
  return SCENARIOS.find((s) => s === value) ?? "off";
}

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const nowSecs = () => Math.floor(Date.now() / 1000);
const DAY = 86_400;

const LIMITS = {
  perPlayerCap: 10,
  serverChangesPerHour: 6,
  maxCommandRows: 20,
  maxRules: 10,
  minIntervalSec: 60,
  minRuleCooldownSec: 10,
  consoleLinesPerMinute: 20,
};

function config(rows: "full" | "plain"): CloudAccount["config"] {
  const plain = (id: string, on: string, text: string) => ({
    id,
    enabled: true,
    on,
    delay: on === "join" || on === "start" || on === "reconnect" ? 2 : null,
    every: on === "interval" ? { min: 300, max: 600 } : null,
    at: on === "daily" ? ["08:00", "20:30"] : null,
    secret: false,
    text,
    preview: null,
    fp: null,
    paused: null,
  });
  return {
    v: 1,
    owner: "EjderAvcisi",
    antiAfk: { enabled: true, delayMin: 60, delayMax: 90 },
    commands:
      rows === "full"
        ? [
            {
              id: "c1",
              enabled: true,
              on: "join",
              delay: 3,
              every: null,
              at: null,
              secret: true,
              text: null,
              preview: "/login ******",
              fp: "KEPTKEPTKEPTKEPT",
              paused: null,
            },
            plain("c2", "join", "/warp maden"),
            plain("c3", "interval", "/afk"),
            plain("c4", "daily", "/günlük"),
          ]
        : [plain("c1", "join", "/is home")],
    rules: [
      {
        id: "r1",
        enabled: true,
        match: "giriş yap",
        regex: false,
        from: "server",
        ownerOnly: false,
        cooldown: 30,
        reply: { secret: true, text: null, preview: "/login ******", fp: "SITESITESITESITE" },
        paused: null,
      },
      {
        id: "r2",
        enabled: true,
        match: "selam",
        regex: false,
        from: "player",
        ownerOnly: false,
        cooldown: 60,
        reply: { secret: false, text: "Selam! Şu an AFK'yım.", preview: null, fp: null },
        paused: null,
      },
      // With the cloud features: a rule whose reply names the sender and a part of its pattern.
      ...(featScenario === "off"
        ? []
        : [
            {
              id: "r3",
              enabled: true,
              match: String.raw`^!davet (\w+)$`,
              regex: true,
              from: "any",
              ownerOnly: false,
              cooldown: 30,
              reply: {
                secret: false,
                text: "/msg {u} {1} davet listene eklendi.",
                preview: null,
                fp: null,
              },
              paused: null,
            },
          ]),
    ],
    inventory: null,
    movement: null,
  };
}

/** The account's features with movement's preview (`&move=…`), while the features are on. */
function withMovement(id: string): CloudAccount["features"] {
  const features = accountFeatures(id);
  return features ? { ...features, movement: movementFeature(id) } : features;
}

function account(
  id: string,
  name: string,
  host: string,
  port: number,
  overrides: Partial<CloudAccount>,
): CloudAccount {
  const now = nowSecs();
  const expiresAt = overrides.expiresAt ?? now + 18 * DAY + 3_600;
  return {
    id,
    name,
    server: { host, port },
    version: "26.3",
    lang: "tr",
    host: "kz1",
    createdAt: now - 12 * DAY,
    expiresAt,
    expired: false,
    graceUntil: expiresAt + 30 * DAY,
    // Renewal is open at any time (owner, 25 Oct: "Süre uzatma"; the last-7-days rule is Studio's
    // switch, off by default).
    renewal: { open: true, opensAt: now - 12 * DAY },
    desired: "run",
    hold: null,
    live: {
      state: "running",
      since: now - 3 * 3_600 - 12 * 60,
      reason: null,
      attempt: null,
      retryAt: null,
    },
    everJoined: true,
    config: config("full"),
    rev: 4,
    serverChanges: { used: 0, limit: 6, resetAt: null },
    tier: plusScenario === "off" ? "" : "plain",
    plusUntil: null,
    ai: null,
    features: withMovement(id),
    moderation: null,
    ...overrides,
  };
}

// Moderation (README "Moderation player API"): `&mod=warn|rows|suspend|appealed` gives the first
// account a step in force (`&mod=refuse` turns a save away instead); without it nothing applies,
// as in observe mode.
const MOD_CASE = "mc_01JBQ7XK2M9RPV4TNW8HD3CFGZ";
function withModeration(accounts: CloudAccount[]): CloudAccount[] {
  const mod = param("mod");
  const first = accounts[0];
  if (!first || !["warn", "rows", "suspend", "appealed"].includes(mod ?? "")) return accounts;
  const at = nowSecs() - 3 * 3_600;
  const moderation = {
    case: MOD_CASE,
    step: mod === "rows" ? "restrict_rows" : mod === "suspend" ? "suspend" : "warn",
    category: mod === "suspend" ? "scam" : "advertising",
    at,
    until: null,
    appeal:
      mod === "appealed"
        ? { can: false, until: at + 7 * DAY, state: "open" }
        : { can: true, until: at + 7 * DAY, state: "none" },
  };
  const next: CloudAccount = { ...first, moderation };
  if (mod === "suspend") {
    next.hold = {
      kind: "suspended",
      until: null,
      reason: "abuse",
      case: MOD_CASE,
      category: "scam",
      auto: true,
    };
    next.live = { state: "stopped", since: at, reason: null, attempt: null, retryAt: null };
    // The service closes renewal while an account is suspended.
    next.renewal = { ...first.renewal, open: false };
  }
  if (mod === "rows") {
    next.config = {
      ...first.config,
      commands: first.config.commands.map((row) =>
        row.id === "c2"
          ? {
              ...row,
              text: "play.ornekmc.net adresine gelin",
              paused: { case: MOD_CASE, category: "advertising" },
            }
          : row,
      ),
    };
  }
  return [next, ...accounts.slice(1)];
}

function initialAccounts(which: Scenario): CloudAccount[] {
  if (!["accounts", "expired", "paused"].includes(which)) return [];
  return withModeration(baseAccounts(which));
}

function baseAccounts(which: Scenario): CloudAccount[] {
  return withPlus(plainAccounts(which), () =>
    account(PLUS_ID, PLUS_NAME, "play.projects.gg", 0, {
      ...plusAccountParts(),
      live: {
        state: "running",
        since: nowSecs() - 5 * 3_600 - 40 * 60,
        reason: null,
        attempt: null,
        retryAt: null,
      },
      rev: 11,
    }),
  );
}

function plainAccounts(which: Scenario): CloudAccount[] {
  const now = nowSecs();
  const list = [
    account("a_7KQ2MZ3P9TW4XB1C", "Madenci07", "mc.madenciler.net", 25570, {
      version: "1.21.4",
    }),
    account(GECE, "GeceBekcisi", "play.projects.gg", 0, {
      expiresAt: now + 5 * DAY + 4 * 3_600,
      live: geceRunning
        ? { state: "running", since: now - 52 * 60, reason: null, attempt: null, retryAt: null }
        : { state: "waiting", since: now - 30, reason: null, attempt: 2, retryAt: now + 14 },
      ...(movementFeature(GECE)?.on
        ? { config: { ...config("full"), movement: { enabled: true } } }
        : {}),
      // `&fresh=on`: a new account that types nothing on joining and has no rule (its settings
      // offer "Kuruluma devam et", SETUP-UX-L).
      ...(param("fresh") === "on"
        ? { config: { ...config("plain"), owner: "", commands: [], rules: [] } }
        : {}),
      rev: 9,
    }),
    account("a_3MCV7Y1KD5HS8FQ2", "TuccarBaykus", "play.projects.gg", 0, {
      expiresAt: now + 27 * DAY,
      // `?hold=invalid`: the machine stopped it on a row that starts with a backslash;
      // `?hold=blocked`: its server is one the cloud cannot join ("Girilemeyen sunucular").
      hold:
        param("hold") === "invalid"
          ? { kind: "cause", cause: "invalidCommand", at: now - 2 * 3_600, row: "c2" }
          : param("hold") === "blocked"
            ? { kind: "server_blocked", reason: null }
            : { kind: "cause", cause: "wrongPassword", at: now - 2 * 3_600, row: null },
      live: {
        state: "stopped",
        since: now - 2 * 3_600,
        reason: null,
        attempt: null,
        retryAt: null,
      },
      config: config("plain"),
      rev: 2,
    }),
  ];
  if (which === "expired") {
    list.push(
      account("a_9PZT4KQ8M2WD6XR1", "EskiBot", "play.projects.gg", 0, {
        expiresAt: now - 2 * DAY,
        expired: true,
        graceUntil: now + 28 * DAY,
        renewal: { open: false, opensAt: now - 9 * DAY },
        hold: { kind: "expired", at: now - 2 * DAY },
        live: {
          state: "stopped",
          since: now - 2 * DAY,
          reason: null,
          attempt: null,
          retryAt: null,
        },
      }),
    );
  }
  if (which === "paused") {
    for (const item of list) {
      item.hold = { kind: "paused", mode: "paused" };
      item.live = {
        state: "stopped",
        since: now - 600,
        reason: null,
        attempt: null,
        retryAt: null,
      };
    }
  }
  return list;
}

/**
 * Browser previews only: the 7/24 Bulut texts, until they are wired into tr.ts and en.ts (the
 * lead's step). Never replaces a text that is there; English joins once English has loaded.
 */
function previewTexts() {
  if (isTauri()) return;
  const add = (lang: "tr" | "en") =>
    i18n.addResourceBundle(
      lang,
      "translation",
      { cloudAfk: lang === "tr" ? trCloudAfk : enCloudAfk },
      true,
      false,
    );
  add("tr");
  if (i18n.hasResourceBundle("en", "translation")) add("en");
  i18n.on("languageChanged", (lang) => {
    if (lang === "en" && i18n.hasResourceBundle("en", "translation")) add("en");
  });
}
previewTexts();

const which = scenario();
let accounts = initialAccounts(which);
const emit = (event: CloudLiveEvent) => mockLive.emit(event);

function replace(next: CloudAccount): CloudAccount {
  accounts = accounts.map((item) => (item.id === next.id ? next : item));
  return next;
}

function find(id: string): CloudAccount {
  const found = accounts.find((item) => item.id === id);
  if (!found) throw new Error("account_not_found");
  return found;
}

/** A command error as the backend words it (the console and toasts read `message`). */
function refused(message: string): Error {
  return Object.assign(new Error(message), { code: "BULUT", details: "mock" });
}

/** An account as a read shows it while its machine cannot be asked (README "Accounts"). */
function unasked(cloud: CloudAccount): CloudAccount {
  return {
    ...cloud,
    live: { state: "unknown", since: null, reason: null, attempt: null, retryAt: null },
    ai: cloud.ai ? { ...cloud.ai, state: "unknown" } : cloud.ai,
  };
}

async function overview(): Promise<CloudPageData> {
  await delay(140);
  const now = nowSecs();
  const hidden = which === "off";
  const shown = mockLive.machineOnline() ? accounts : accounts.map(unasked);
  const desktop = await api.afkAccounts();
  const twins = accounts.flatMap((cloud) => {
    const match = desktop.find(
      (d) =>
        d.nickname.toLowerCase() === cloud.name.toLowerCase() &&
        d.server ===
          (cloud.server.port && cloud.server.port !== 25565
            ? `${cloud.server.host}:${cloud.server.port}`
            : cloud.server.host),
    );
    return match ? [{ desktopId: match.id, cloudId: cloud.id }] : [];
  });
  return {
    overview: {
      available: !hidden,
      mode: which === "paused" ? "paused" : "on",
      maintenance:
        which === "paused"
          ? {
              tr: "Bulut sunucusu bakımda; 23:00'te döneriz.",
              en: "The cloud server is under maintenance; back at 23:00.",
            }
          : null,
      monthPrice: hidden ? null : "60.00",
      soldOut: which === "soldout",
      remaining: null,
      salesOpen: which !== "paused" && which !== "closed",
      renewalWindowDays: 7,
      limits: LIMITS,
      session:
        which === "signedout" || hidden ? "signedOut" : which === "failed" ? "failed" : "signedIn",
      accounts: which === "failed" || which === "signedout" || hidden ? [] : shown,
      count: which === "signedout" || hidden ? null : { count: accounts.length, cap: 10 },
      canBuy:
        which === "signedout" || hidden
          ? null
          : { ok: which !== "soldout", reason: null, until: null },
      now,
      plus: hidden ? null : mockPlusOffer(),
      plusMe: which === "signedout" || hidden ? null : mockPlusMe(),
      features: hidden ? null : offerFeatures(),
    },
    twins,
  };
}

const CONSOLE: Array<[string, string]> = [
  ["n", "[Projects] Sunucuya hoş geldin!"],
  ["s", "/login ******"],
  ["n", "[AuthMe] Başarıyla giriş yaptın."],
  ["s", "/warp maden"],
  ["n", "Maden bölgesine ışınlandın."],
  ["n", "<GeceKurdu_> selam millet"],
  ["n", "[Duyuru] Etkinlik 21:00'de başlıyor!"],
  ["e", "Sunucuyla bağlantı bir an yavaşladı."],
  ["n", "<ElmasKazma> akşam pvp var mı"],
  ["n", "[Projects] AFK bölgesindesin, ödüllerin birikiyor."],
];

const CHAT: string[] = [
  "<GeceKurdu_> biri pazara gelsin",
  "[Duyuru] Haftalık sıralama yenilendi.",
  "<SessizOkcu> hoş geldin Madenci07",
  "[Projects] 15 dakika AFK ödülü: 120 altın.",
];

const watching = new Map<string, number>();

// The machine went away or came back while a console is on screen: its socket says so, and back,
// the watched accounts' state and strip come again (mock-live.ts).
mockLive.onLink((online) => {
  if (watching.size === 0) return;
  emit({ kind: "machine", host: "kz1", online });
  if (!online) return;
  for (const id of watching.keys()) {
    const cloud = find(id);
    emit({ kind: "state", acct: id, live: cloud.live });
    const bars = cloud.live.state === "running" ? barsEvent(id) : null;
    if (bars) emit(bars);
  }
});

function lines(id: string, count: number, endAt: number) {
  if (featScenario !== "off") {
    const cloud = find(id);
    // In game since then: its log does not show it joining minutes ago.
    const since = cloud.live.state === "running" ? cloud.live.since : null;
    return kindLines(count, endAt, cloud.name, serverLabel(cloud.server), since);
  }
  return Array.from({ length: count }, (_, i) => {
    const [k, text] = CONSOLE[i % CONSOLE.length] ?? ["n", ""];
    return { at: endAt - (count - i) * 37, text, k, c: null, who: null, goto: null };
  }).map((line) => ({ ...line, text: line.text.replace("Madenci07", find(id).name) }));
}

function editView(cloud: CloudAccount): CloudEditData {
  return {
    account: cloud,
    config: {
      owner: cloud.config.owner,
      antiAfk: cloud.config.antiAfk,
      commands: cloud.config.commands.map((row) => ({
        id: row.id,
        enabled: row.enabled,
        on: row.on,
        delay: row.delay,
        every: row.every,
        at: row.at,
        words: row.secret
          ? row.fp === "KEPTKEPTKEPTKEPT"
            ? { kind: "secret" as const, text: "/login Gizli123", kept: true }
            : { kind: "sealed" as const, preview: row.preview ?? "******" }
          : { kind: "plain" as const, text: row.text ?? "" },
      })),
      rules: cloud.config.rules.map((rule) => ({
        id: rule.id,
        enabled: rule.enabled,
        pattern: rule.match,
        regex: rule.regex,
        from: rule.from,
        ownerOnly: rule.ownerOnly,
        cooldown: rule.cooldown,
        reply: rule.reply.secret
          ? { kind: "sealed" as const, preview: rule.reply.preview ?? "******" }
          : { kind: "plain" as const, text: rule.reply.text ?? "" },
      })),
      inventory: cloud.config.inventory?.enabled ?? null,
      movement: cloud.config.movement?.enabled ?? null,
    },
    versions: {
      versions: [
        { value: "26.3", label: "26.3" },
        { value: "26.2", label: "26.2" },
        { value: "26.1.2", label: "26.1.2" },
        { value: "1.21.4", label: "1.21.4" },
        { value: "1.20.4", label: "1.20.3 – 1.20.4" },
      ],
      recommended: "26.3",
    },
    limits: LIMITS,
  };
}

function plan(name: string, server: string): CloudMovePlan {
  const [host, port] = server.split(":");
  return {
    name,
    server: { host: host ?? server, port: Number(port ?? 0) },
    serverProblem: null,
    version: "26.3",
    config: {
      owner: "EjderAvcisi",
      antiAfk: { enabled: true, delayMin: 60, delayMax: 90 },
      commands: [
        {
          id: "c1",
          enabled: true,
          on: "join",
          delay: 2,
          every: null,
          at: null,
          words: { kind: "secret", text: "/login Gizli123", kept: false },
        },
        {
          id: "c2",
          enabled: true,
          on: "join",
          delay: 2,
          every: null,
          at: null,
          words: { kind: "secret", text: "/warp maden", kept: false },
        },
        {
          id: "c3",
          enabled: true,
          on: "interval",
          delay: null,
          every: { min: 60, max: 120 },
          at: null,
          words: { kind: "plain", text: "/afk" },
        },
      ],
      rules: [
        {
          id: "r1",
          enabled: true,
          pattern: "selam",
          regex: false,
          from: "any",
          ownerOnly: false,
          cooldown: 10,
          reply: { kind: "plain", text: "Selam! Şu an AFK'yım." },
        },
      ],
      inventory: null,
      movement: null,
    },
    dropped: [
      { what: "walking", count: 1 },
      { what: "autoEat", count: 1 },
      { what: "rawActions", count: 2 },
    ],
    adjusted: ["interval"],
  };
}

export const mockCloudApi: CloudApi = {
  overview,
  async start(id) {
    await delay(300);
    const now = nowSecs();
    const next = replace({
      ...find(id),
      desired: "run",
      hold: null,
      live: { state: "connecting", since: now, reason: null, attempt: 1, retryAt: null },
    });
    setTimeout(() => {
      const current = accounts.find((a) => a.id === id);
      if (current?.desired === "run") {
        const live = {
          state: "running",
          since: nowSecs(),
          reason: null,
          attempt: null,
          retryAt: null,
        };
        replace({ ...current, live });
        emit({ kind: "state", acct: id, live });
      }
    }, 2_500);
    return next;
  },
  async stop(id) {
    await delay(250);
    return replace({
      ...find(id),
      desired: "stop",
      live: { state: "stopped", since: nowSecs(), reason: null, attempt: null, retryAt: null },
    });
  },
  async restart(id) {
    await delay(250);
    return replace({
      ...find(id),
      live: { state: "connecting", since: nowSecs(), reason: null, attempt: 1, retryAt: null },
    });
  },
  async send(id, line) {
    await delay(200);
    if (find(id).live.state !== "running") throw refused("Hesap şu an oyunda değil.");
    // The server answers; the console never echoes the line itself.
    setTimeout(() => {
      emit({
        kind: "lines",
        acct: id,
        lines: [
          {
            at: nowSecs(),
            text: `[Projects] Komut alındı: ${line.split(" ")[0]}`,
            k: "n",
            c: featScenario === "off" ? null : "sys",
            who: null,
            goto: null,
          },
        ],
        skipped: 0,
        snapshot: false,
      });
    }, 700);
  },
  async logs(id, before): Promise<CloudLogPage> {
    await delay(300);
    const end = (before ?? nowSecs()) - 1;
    return { lines: lines(id, 20, end), next: null };
  },
  async watch(id) {
    await delay(120);
    emit({ kind: "status", host: "kz1", status: "connecting" });
    setTimeout(() => {
      emit({ kind: "status", host: "kz1", status: "live" });
      // `&link=down`: the account's machine is away (the panel says so above its tabs), so nothing
      // of the account streams.
      const online = mockLive.machineOnline();
      emit({ kind: "hello", host: "kz1", online });
      if (!online) return;
      const cloud = find(id);
      emit({ kind: "state", acct: id, live: cloud.live });
      emit({
        kind: "lines",
        acct: id,
        lines: lines(id, 14, nowSecs()),
        skipped: 0,
        snapshot: true,
      });
      const bars = cloud.live.state === "running" ? barsEvent(id) : null;
      if (bars) emit(bars);
    }, 260);
    let n = 0;
    window.clearInterval(watching.get(id));
    watching.set(
      id,
      window.setInterval(() => {
        if (find(id).live.state !== "running" || !mockLive.machineOnline()) return;
        const at = nowSecs();
        emit({
          kind: "lines",
          acct: id,
          lines: [
            featScenario === "off"
              ? { at, text: CHAT[n++ % CHAT.length] ?? "", k: "n", c: null, who: null, goto: null }
              : liveKindLine(n++, at, find(id).name, serverLabel(find(id).server)),
          ],
          skipped: 0,
          snapshot: false,
        });
      }, 4_200),
    );
  },
  async unwatch(id) {
    window.clearInterval(watching.get(id));
    watching.delete(id);
  },
  onLive(listener) {
    return mockLive.listen(listener);
  },
  async edit(id) {
    await delay(220);
    return editView(find(id));
  },
  async save(id, edit: CloudEdit) {
    await delay(400);
    const current = find(id);
    if (edit.rev !== current.rev) return { kind: "conflict", account: current };
    // README "Saving": `&mod=refuse` turns away the second command as a clear advertising hit.
    if (param("mod") === "refuse" && edit.config) {
      return {
        kind: "invalid",
        issues: [{ place: "commands", index: 1, field: "text", problem: "content:advertising" }],
      };
    }
    const serverChanged =
      edit.server !== null &&
      (edit.server.host !== current.server.host || edit.server.port !== current.server.port);
    if (serverChanged && edit.config) {
      const issues = [
        ...edit.config.commands.flatMap((row, index) =>
          row.words.kind === "sealed"
            ? [{ place: "commands", index, field: "text", problem: "reseal" }]
            : [],
        ),
        ...edit.config.rules.flatMap((rule, index) =>
          rule.reply.kind === "sealed"
            ? [{ place: "rules", index, field: "reply", problem: "reseal" }]
            : [],
        ),
      ];
      if (issues.length) return { kind: "invalid", issues };
    }
    const inventory = edit.config?.inventory;
    const movement = edit.config?.movement;
    const movementNow = current.features?.movement;
    // The service's slot check (README "Movement slots"): `&move=player|host` keep it off.
    if (movement === true && movementNow && !movementNow.on && movementNow.slot) {
      return {
        kind: "invalid",
        issues: [
          {
            place: "general",
            index: null,
            field: "movement",
            problem: movementNow.slot,
            max: movementNow.slot === "player" ? 1 : 10,
          },
        ],
      };
    }
    const next = replace({
      ...current,
      name: edit.name ?? current.name,
      server: edit.server ?? current.server,
      version: edit.version ?? current.version,
      lang: edit.lang ?? current.lang,
      rev: current.rev + 1,
      config: {
        ...current.config,
        ...(typeof inventory === "boolean" ? { inventory: { enabled: inventory } } : {}),
        ...(typeof movement === "boolean" ? { movement: { enabled: movement } } : {}),
      },
      features:
        (typeof inventory === "boolean" || typeof movement === "boolean") && current.features
          ? {
              ...current.features,
              ...(typeof inventory === "boolean"
                ? { inventory: { mode: current.features.inventory?.mode ?? "off", on: inventory } }
                : {}),
              ...(typeof movement === "boolean" && current.features.movement
                ? { movement: { ...current.features.movement, on: movement, slot: null } }
                : {}),
            }
          : current.features,
    });
    return { kind: "saved", account: next };
  },
  async reveal() {
    await delay(650);
    const fail = param("reveal");
    if (fail === "offline") return { kind: "offline" };
    if (fail === "retype") return { kind: "retype" };
    if (fail === "limited") return { kind: "limited", retry_after: 60 };
    return { kind: "shown", text: "/login EskiSifre42" };
  },
  async movePlan(desktopId) {
    await delay(260);
    const desktop = (await api.afkAccounts()).find((a) => a.id === desktopId);
    return plan(desktop?.nickname ?? "KasaBekcisi", desktop?.server ?? "play.projects.gg");
  },
  async move(_desktopId, cloudId, rev, run) {
    await delay(700);
    const current = find(cloudId);
    if (rev !== current.rev) {
      return {
        outcome: { kind: "conflict", account: current },
        stoppedHere: false,
        started: false,
      };
    }
    const next = replace({
      ...current,
      rev: current.rev + 1,
      desired: run ? "run" : current.desired,
      hold: null,
    });
    return { outcome: { kind: "saved", account: next }, stoppedHere: true, started: run };
  },
  async connectHere() {
    // As `cloud_afk_connect_here`: every desktop account without a cloud twin (AFK-CLEAR).
    const { twins } = await overview();
    const left = new Set(twins.map((twin) => twin.desktopId));
    for (const account of await api.afkAccounts()) {
      if (left.has(account.id) || !["stopped", "error"].includes(account.state.kind)) continue;
      await api.afkConnect(account.id);
    }
    return left.size;
  },
  async open(page, account) {
    console.info("[cloud-afk mock] open afk.projects.gg", page, account ?? "");
  },
  async inventoryWatch(id) {
    await delay(60);
    watchInventory(id, find(id).live.state === "running", emit);
  },
  async inventoryUnwatch(id) {
    unwatchInventory(id);
  },
  async invOp(id, opId, op) {
    void inventoryMove(id, op).then((result) =>
      emit({
        kind: "invResult",
        id: opId,
        ok: result.ok === true,
        error:
          result.ok === true
            ? null
            : {
                code: result.error.code,
                reason: result.error.reason ?? null,
                field: result.error.field ?? null,
                retryAfter: result.error.retryAfter ?? null,
              },
      }),
    );
    return null;
  },
  async movementWatch(id) {
    await delay(60);
    watchMovement(id, find(id).live.state === "running", emit);
  },
  async movementUnwatch(id) {
    unwatchMovement(id);
  },
  async movementOp(id, opId, op) {
    const refused = moveOperation(id, op);
    window.setTimeout(
      () => emit({ kind: "movementResult", id: opId, ok: refused === null, error: refused }),
      40,
    );
    return null;
  },
  async setInventory(id, enabled) {
    await delay(380);
    const current = find(id);
    const next = replace({
      ...current,
      rev: current.rev + 1,
      config: { ...current.config, inventory: { enabled } },
      features: current.features
        ? {
            ...current.features,
            inventory: { mode: current.features.inventory?.mode ?? "off", on: enabled },
          }
        : current.features,
    });
    // The running client started without it: it comes at the next connection.
    setTimeout(
      () => emit({ kind: "invState", acct: id, state: "off", reason: "restart", lock: null }),
      120,
    );
    return { kind: "saved", account: next };
  },
  async tryReply(rule, sample, sender) {
    return mockTryReply(rule, sample, sender);
  },
  async download(id) {
    await delay(500);
    const day = new Date().toISOString().slice(0, 10);
    return { fileName: `${find(id).name}-konsol-${day}.txt` };
  },
  async showDownload(fileName) {
    console.info("[cloud-afk mock] show", fileName);
  },
  async copySource(id) {
    await delay(200);
    const source = find(id);
    // The site's seal (r1): this computer has no words for it; its own copy (c1) it has.
    const toType = source.config.rules
      .filter((rule) => rule.reply.secret && rule.reply.fp !== "KEPTKEPTKEPTKEPT")
      .map((rule) => ({ row: rule.id, place: "rules", preview: rule.reply.preview ?? "******" }));
    return { id, toType };
  },
  async copySettings(source, targets) {
    await delay(700);
    const from = find(source);
    return targets.map((target): CloudCopyResult => {
      const current = find(target);
      if (current.expired) return { id: target, account: null, code: "BULUT-SURE" };
      const next = replace({
        ...current,
        config: {
          ...from.config,
          inventory: current.config.inventory,
          movement: current.config.movement,
        },
        rev: current.rev + 1,
      });
      return { id: target, account: next, code: null };
    });
  },
};
