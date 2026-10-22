// 7/24 Bulut for browser previews. Preview switch (read once, from the page URL): `?cloud=<scenario>`
// - (none) or `off`: the service does not answer; the AFK page looks exactly as without it;
// - `intro`: signed in, no cloud account yet (the intro card);
// - `signedout`: the service answers, the friends session is missing;
// - `soldout`: no free slots;
// - `accounts`: three accounts (one in game, one reconnecting with its time running out, one
//   stopped for a wrong password), `Madenci07` also on this computer (its "Bilgisayarda / Bulutta");
// - `expired`: `accounts` plus one whose time is up;
// - `paused`: maintenance (Studio's note, the accounts held);
// - `failed`: signed in, the accounts could not be read.
// `&reveal=offline|retype|limited` makes "Göster" fail that way.
import { api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { enCloudAfk } from "@/locales/en-cloud-afk";
import { trCloudAfk } from "@/locales/tr-cloud-afk";
import type {
  CloudAccount,
  CloudApi,
  CloudEdit,
  CloudEditData,
  CloudLiveEvent,
  CloudLogPage,
  CloudMovePlan,
  CloudPageData,
} from "./api";

type Scenario =
  | "off"
  | "intro"
  | "signedout"
  | "soldout"
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
  });
  return {
    v: 1,
    owner: "KasaBekcisi",
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
      },
    ],
  };
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
    renewal: { open: expiresAt - now <= 7 * DAY, opensAt: expiresAt - 7 * DAY },
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
    ...overrides,
  };
}

function initialAccounts(which: Scenario): CloudAccount[] {
  if (!["accounts", "expired", "paused"].includes(which)) return [];
  const now = nowSecs();
  const list = [
    account("a_7KQ2MZ3P9TW4XB1C", "Madenci07", "mc.madenciler.net", 25570, {
      version: "1.21.4",
    }),
    account("a_8XH4N2RT6WQ9PB3D", "GeceBekcisi", "play.projects.gg", 0, {
      expiresAt: now + 5 * DAY + 4 * 3_600,
      live: { state: "waiting", since: now - 30, reason: null, attempt: 2, retryAt: now + 14 },
      rev: 9,
    }),
    account("a_3MCV7Y1KD5HS8FQ2", "Tuccar_Ali", "play.projects.gg", 0, {
      expiresAt: now + 27 * DAY,
      // `?hold=invalid`: the machine stopped it on a row that starts with a backslash.
      hold:
        param("hold") === "invalid"
          ? { kind: "cause", cause: "invalidCommand", at: now - 2 * 3_600, row: "c2" }
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
const listeners = new Set<(event: CloudLiveEvent) => void>();
const emit = (event: CloudLiveEvent) => {
  for (const listener of listeners) listener(event);
};

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

async function overview(): Promise<CloudPageData> {
  await delay(140);
  const now = nowSecs();
  const hidden = which === "off";
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
      salesOpen: which !== "paused",
      renewalWindowDays: 7,
      limits: LIMITS,
      session:
        which === "signedout" || hidden ? "signedOut" : which === "failed" ? "failed" : "signedIn",
      accounts: which === "failed" || which === "signedout" || hidden ? [] : accounts,
      count: which === "signedout" || hidden ? null : { count: accounts.length, cap: 10 },
      canBuy:
        which === "signedout" || hidden
          ? null
          : { ok: which !== "soldout", reason: null, until: null },
      now,
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
  ["n", "<Ayşe> selam millet"],
  ["n", "[Duyuru] Etkinlik 21:00'de başlıyor!"],
  ["e", "Sunucuyla bağlantı bir an yavaşladı."],
  ["n", "<Mert> akşam pvp var mı"],
  ["n", "[Projects] AFK bölgesindesin, ödüllerin birikiyor."],
];

const CHAT: string[] = [
  "<Ayşe> biri pazara gelsin",
  "[Duyuru] Haftalık sıralama yenilendi.",
  "<Kerem> hoş geldin Madenci07",
  "[Projects] 15 dakika AFK ödülü: 120 altın.",
];

const watching = new Map<string, number>();

function lines(id: string, count: number, endAt: number) {
  return Array.from({ length: count }, (_, i) => {
    const [k, text] = CONSOLE[i % CONSOLE.length] ?? ["n", ""];
    return { at: endAt - (count - i) * 37, text, k };
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
      owner: "KasaBekcisi",
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
        lines: [{ at: nowSecs(), text: `[Projects] Komut alındı: ${line.split(" ")[0]}`, k: "n" }],
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
      emit({ kind: "hello", host: "kz1", online: true });
      const cloud = find(id);
      emit({ kind: "state", acct: id, live: cloud.live });
      emit({
        kind: "lines",
        acct: id,
        lines: lines(id, 14, nowSecs()),
        skipped: 0,
        snapshot: true,
      });
    }, 260);
    let n = 0;
    window.clearInterval(watching.get(id));
    watching.set(
      id,
      window.setInterval(() => {
        if (find(id).live.state !== "running") return;
        emit({
          kind: "lines",
          acct: id,
          lines: [{ at: nowSecs(), text: CHAT[n++ % CHAT.length] ?? "", k: "n" }],
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
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  },
  async edit(id) {
    await delay(220);
    return editView(find(id));
  },
  async save(id, edit: CloudEdit) {
    await delay(400);
    const current = find(id);
    if (edit.rev !== current.rev) return { kind: "conflict", account: current };
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
    const next = replace({
      ...current,
      name: edit.name ?? current.name,
      server: edit.server ?? current.server,
      version: edit.version ?? current.version,
      lang: edit.lang ?? current.lang,
      rev: current.rev + 1,
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
    await api.afkConnectAll();
    return 1;
  },
  async open(page, account) {
    console.info("[cloud-afk mock] open afk.projects.gg", page, account ?? "");
  },
};
