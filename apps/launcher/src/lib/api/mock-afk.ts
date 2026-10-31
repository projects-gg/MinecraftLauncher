// Simulated AFK sessions for previews: states move like the real MCC bridge would report them, and
// the settings sheet talks to a stand-in for the backend (same redaction and validation rules as
// crates/pl-afk, passwords kept in a pretend keychain).

import i18n, { currentLanguage } from "@/lib/i18n";
import type {
  AfkSettings,
  AfkSettingsSaveResult,
  AfkSettingsView,
  LoginCommandLine,
  PatternProblem,
  SettingsPage,
  ValidationIssue,
  VersionOption,
} from "./afk-settings";
import { pick } from "./mock-lang";
import type { AfkAccount, AfkClientProblem, AfkLogLine, AfkState } from "./types";

const listeners = new Set<() => void>();
const notify = () => {
  for (const l of listeners) l();
};

let lineId = 0;
const now = () => new Date().toISOString();
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Preview switch `?afk=none`: start without accounts (the empty state). */
function startEmpty(): boolean {
  try {
    return new URLSearchParams(window.location.search).get("afk") === "none";
  } catch {
    return false;
  }
}

/**
 * Preview switch `?afk=password`: TarlaBot_1 starts stopped on a login password the server turned
 * down (`refused`/`wrongPassword`), its `/login` line kept in the keychain (AFK-CLEAR's "Şifreyi
 * düzelt").
 */
function startOnPassword(): boolean {
  try {
    return new URLSearchParams(window.location.search).get("afk") === "password";
  } catch {
    return false;
  }
}

/** The refused password's words in the interface's language (the backend words them so). */
const refusedPassword = (): AfkState => ({
  kind: "error",
  reason: pick("Giriş şifresi yanlış (Yanlış şifre!)", "Wrong login password (Wrong password!)"),
  cause: "wrongPassword",
});

const accounts: AfkAccount[] = startEmpty()
  ? []
  : [
      {
        id: "afk1",
        nickname: "KasaBekcisi",
        server: "play.projects.gg",
        autoStart: true,
        mcVersion: "26.3",
        mcVersionRecommended: true,
        state: { kind: "connected", since: new Date(Date.now() - 65 * 60_000).toISOString() },
        health: 20,
        food: 18,
        players: 214,
        settingsChanged: false,
        showDate: false,
      },
      {
        id: "afk2",
        nickname: "TarlaBot_1",
        server: "play.projects.gg",
        autoStart: true,
        mcVersion: "26.3",
        mcVersionRecommended: true,
        state: startOnPassword()
          ? refusedPassword()
          : {
              kind: "reconnecting",
              attempt: 2,
              nextAttemptAt: new Date(Date.now() + 22_000).toISOString(),
            },
        settingsChanged: false,
        showDate: true,
      },
      {
        id: "afk3",
        nickname: "Madenci07",
        server: "mc.madenciler.net:25570",
        autoStart: false,
        mcVersion: "auto",
        mcVersionRecommended: false,
        state: { kind: "stopped" },
        settingsChanged: false,
        showDate: false,
      },
    ];

// --- Settings (the backend's rules, in miniature) ----------------------------------------------

/** `versions::VERSION_TABLE` (the AFK client CI's list, newest first). */
const VERSIONS: Array<[string, string]> = [
  ["26.3", "26.3"],
  ["26.2", "26.2"],
  ["26.1", "26.1"],
  ["1.21.11", "1.21.11"],
  ["1.21.9", "1.21.9 – 1.21.10"],
  ["1.21.7", "1.21.7 – 1.21.8"],
  ["1.21.6", "1.21.6"],
  ["1.21.5", "1.21.5"],
  ["1.21.4", "1.21.4"],
  ["1.21.2", "1.21.2 – 1.21.3"],
  ["1.21", "1.21 – 1.21.1"],
  ["1.20.5", "1.20.5 – 1.20.6"],
  ["1.20.3", "1.20.3 – 1.20.4"],
  ["1.20.2", "1.20.2"],
  ["1.20", "1.20 – 1.20.1"],
  ["1.19.4", "1.19.4"],
  ["1.19.3", "1.19.3"],
  ["1.19.1", "1.19.1 – 1.19.2"],
  ["1.19", "1.19"],
  ["1.18.2", "1.18.2"],
  ["1.18", "1.18 – 1.18.1"],
  ["1.17.1", "1.17.1"],
  ["1.17", "1.17"],
  ["1.16.4", "1.16.4 – 1.16.5"],
  ["1.16.3", "1.16.3"],
  ["1.16.2", "1.16.2"],
  ["1.16.1", "1.16.1"],
  ["1.16", "1.16"],
  ["1.15.2", "1.15.2"],
  ["1.15.1", "1.15.1"],
  ["1.15", "1.15"],
  ["1.14.4", "1.14.4"],
  ["1.14.3", "1.14.3"],
  ["1.14.2", "1.14.2"],
  ["1.14.1", "1.14.1"],
  ["1.14", "1.14"],
  ["1.13.2", "1.13.2"],
  ["1.13.1", "1.13.1"],
  ["1.13", "1.13"],
  ["1.12.2", "1.12.2"],
  ["1.12.1", "1.12.1"],
  ["1.12", "1.12"],
  ["1.11.1", "1.11.1 – 1.11.2"],
  ["1.11", "1.11"],
  ["1.10", "1.10 – 1.10.2"],
  ["1.9.3", "1.9.3 – 1.9.4"],
  ["1.9.2", "1.9.2"],
  ["1.9.1", "1.9.1"],
  ["1.9", "1.9"],
  ["1.8", "1.8 – 1.8.9"],
  ["1.7.6", "1.7.6 – 1.7.10"],
  ["1.7.2", "1.7.2 – 1.7.5"],
  ["1.6.4", "1.6.4"],
  ["1.6.2", "1.6.2"],
  ["1.6.1", "1.6.1"],
  ["1.6", "1.6"],
  ["1.5.2", "1.5.2"],
  ["1.5", "1.5 – 1.5.1"],
  ["1.4.6", "1.4.6 – 1.4.7"],
];

const RECOMMENDED = "26.3";

/** `VersionCatalog::options`: "auto" first, then the list, the recommended one flagged. */
const versions: VersionOption[] = [
  { value: "auto", label: "auto", recommended: false, auto: true },
  ...VERSIONS.map(([value, label]) => ({
    value,
    label,
    recommended: value === RECOMMENDED,
    auto: false,
  })),
];

/** `VersionCatalog::resolve`: "" follows the recommendation, unknown values fall back to it. */
function resolveVersion(stored: string) {
  const value = stored.trim();
  if (!value) return { value: RECOMMENDED, recommended: true, unknown: null };
  if (value.toLowerCase() === "auto") return { value: "auto", recommended: false, unknown: null };
  const entry = VERSIONS.find(([v]) => v.toLowerCase() === value.toLowerCase());
  return entry
    ? { value: entry[0], recommended: entry[0] === RECOMMENDED, unknown: null }
    : { value: RECOMMENDED, recommended: true, unknown: value };
}

/** `VersionCatalog::to_stored`: the recommended entry means "follow". */
function toStoredVersion(chosen: string): string {
  const value = chosen.trim();
  if (!value || value === RECOMMENDED) return "";
  if (value.toLowerCase() === "auto") return "auto";
  return VERSIONS.find(([v]) => v === value)?.[0] ?? "";
}

/** Stored game versions ("1.7" is a legacy value the list does not know). */
const storedVersions = new Map<string, string>([
  ["afk1", ""],
  ["afk2", "1.7"],
  ["afk3", "auto"],
]);

/** A repetition inside a repeated group, like (a+)+. */
const NESTED_REPEAT = /\((?:[^()\\]|\\.)*[+*}](?:[^()\\]|\\.)*\)\s*[+*{]/;

/** `pattern::check_pattern`, approximated with the browser's engine for previews. */
function checkPattern(pattern: string, isRegex: boolean): PatternProblem | null {
  const text = pattern.trim();
  if ([...text].length > 256) return "tooLong";
  if ([...text].some((c) => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127)) return "invalid";
  if (!isRegex || !text) return null;
  if (NESTED_REPEAT.test(text)) return "tooComplex";
  try {
    new RegExp(text.replace(/^\(\?[ims]+\)/, ""));
    return null;
  } catch {
    return "invalid";
  }
}

/** `console::SENSITIVE_COMMANDS`. */
const SENSITIVE = [
  "/login",
  "/l",
  "/log",
  "/signin",
  "/g",
  "/gir",
  "/giris",
  "/giriş",
  "/girisyap",
  "/girişyap",
  "/register",
  "/reg",
  "/signup",
  "/k",
  "/kayit",
  "/kayıt",
  "/kaydol",
  "/password",
  "/pass",
  "/passwd",
  "/sifre",
  "/şifre",
  "/changepassword",
  "/changepass",
  "/cp",
  "/auth",
  "/authme",
  "/2fa",
  "/2fauth",
];

/** `model::default_kick_messages_in`: the legacy English list and the Turkish one. */
const KICK_EN = [
  "Connection has been lost",
  "Server is restarting",
  "Server is full",
  "Too Many people",
];
const KICK_TR = [
  "Bağlantı koptu",
  "Sunucu yeniden başlatılıyor",
  "Sunucu dolu",
  "Çok fazla oyuncu",
];

/** `model::kick_messages_in`: an untouched default list reads in the interface's language. */
function kickMessagesNow(list: string[]): string[] {
  const lines = list.map((line) => line.trim()).filter(Boolean);
  const same = (defaults: string[]) =>
    defaults.length === lines.length && defaults.every((line, i) => line === lines[i]);
  if (!same(KICK_EN) && !same(KICK_TR)) return list;
  return [...(currentLanguage() === "en" ? KICK_EN : KICK_TR)];
}

const defaultPage = (): SettingsPage => ({
  serverHost: "",
  serverPort: 0,
  minecraftVersion: "26.3",
  autoStart: false,
  allowResourcePacks: false,
  loginCommands: { enabled: false, onlyFirstLogin: false, delaySeconds: 2, commands: [] },
  scheduler: { enabled: false, tasks: [] },
  antiAfk: {
    enabled: true,
    delayMin: 60,
    delayMax: 90,
    command: "",
    useSneak: true,
    useTerrainHandling: false,
    walkRange: 5,
    walkRetries: 20,
  },
  autoRelog: {
    enabled: true,
    delayMin: 5,
    delayMax: 10,
    retries: -1,
    ignoreKickMessage: true,
    kickMessages: [...KICK_EN],
  },
  autoEat: { enabled: false, threshold: 6 },
  autoRespond: { enabled: false, soundEnabled: false, matchColors: false, rules: [] },
  chatLog: { enabled: false, addDateTime: true, showDate: false, filter: "messages" },
  alerts: {
    enabled: false,
    beepEnabled: false,
    triggerByWords: true,
    logToFile: true,
    matches: [],
    excludes: [],
  },
});

/** Settings as stored (login commands live apart, like in the keychain). */
const stored = new Map<string, SettingsPage>([
  [
    "afk1",
    {
      ...defaultPage(),
      autoStart: true,
      loginCommands: { enabled: true, onlyFirstLogin: false, delaySeconds: 2, commands: [] },
      scheduler: {
        enabled: true,
        tasks: [
          {
            name: "Günlük kit",
            action: "send /kit gunluk",
            onLogin: false,
            onFirstLogin: true,
            intervalEnabled: false,
            minSeconds: 300,
            maxSeconds: 600,
          },
          {
            name: "Pazar duyurusu",
            action: "send Pazarda elmas kılıç satılık, /pazar KasaBekcisi",
            onLogin: false,
            onFirstLogin: false,
            intervalEnabled: true,
            minSeconds: 900,
            maxSeconds: 1800,
          },
        ],
      },
      antiAfk: { ...defaultPage().antiAfk, useTerrainHandling: true, walkRange: 8 },
      autoEat: { enabled: true, threshold: 8 },
      autoRespond: {
        enabled: true,
        soundEnabled: true,
        matchColors: false,
        rules: [
          {
            pattern: "selam",
            reply: "Merhaba $u, şu an klavye başında değilim!",
            privateReply: "Selam $u, sonra yazarım.",
            otherReply: "",
            ownersOnly: false,
            cooldownSeconds: 60,
            isRegex: false,
          },
          {
            pattern: "^kasa (açık|acik) mı",
            reply: "Kasa her gün 18.00'de açılıyor.",
            privateReply: "",
            otherReply: "",
            ownersOnly: false,
            cooldownSeconds: 30,
            isRegex: true,
          },
        ],
      },
      chatLog: { enabled: true, addDateTime: true, showDate: false, filter: "messages" },
      alerts: {
        enabled: true,
        beepEnabled: true,
        triggerByWords: true,
        logToFile: true,
        matches: ["KasaBekcisi", "elmas"],
        excludes: ["[Bot]"],
      },
    },
  ],
  [
    "afk2",
    {
      ...defaultPage(),
      autoStart: true,
      loginCommands: {
        enabled: startOnPassword(),
        onlyFirstLogin: false,
        delaySeconds: 2,
        commands: [],
      },
      autoRelog: {
        ...defaultPage().autoRelog,
        retries: 25,
        ignoreKickMessage: false,
        kickMessages: ["Sunucu yeniden başlatılıyor", "Sunucu dolu"],
      },
      chatLog: { enabled: false, addDateTime: true, showDate: true, filter: "all" },
    },
  ],
  [
    "afk3",
    {
      ...defaultPage(),
      serverHost: "mc.madenciler.net",
      serverPort: 25570,
      minecraftVersion: "auto",
      antiAfk: { ...defaultPage().antiAfk, command: "/afk" },
    },
  ],
]);

const keychain = new Map<string, string[]>([
  ["afk1", ["/login KasaSifre42", "/warp afk"]],
  ...(startOnPassword() ? ([["afk2", ["/login TarlaEski1"]]] as Array<[string, string[]]>) : []),
]);
const globalSettings: AfkSettings = { showScriptCommands: false };

function secretName(command: string): string | null {
  const [name = "", ...rest] = command.trim().split(/[ \t]+/);
  const upper = name.toUpperCase();
  return rest.join("").length && SENSITIVE.some((s) => s.toUpperCase() === upper) ? name : null;
}

/** `SettingsPage::from_settings`: passwords never leave the "keychain". */
function redact(page: SettingsPage, commands: string[]): SettingsPage {
  const lines: LoginCommandLine[] = [];
  commands.forEach((command, index) => {
    if (!command.trim()) return;
    const name = secretName(command);
    lines.push(name ? { kind: "stored", index, command: name } : { kind: "text", text: command });
  });
  return { ...page, loginCommands: { ...page.loginCommands, commands: lines } };
}

const clean = (items: string[]) => items.map((s) => s.trim()).filter(Boolean);
const ordered = (a: number, b: number): [number, number] => (a > b ? [b, a] : [a, b]);
const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

function commandError(code: string, message: string): Error {
  return Object.assign(new Error(message), { code, details: `mock: ${code}` });
}

/** The account is gone, worded like the backend. */
const accountGone = () =>
  commandError(
    "AFK-HESAP-YOK",
    pick(
      "Bu AFK hesabı artık yok. Listeyi yenileyip tekrar dene.",
      "This AFK account is gone. Refresh the list and try again.",
    ),
  );

function serverLabel(page: SettingsPage): string {
  const host = page.serverHost.trim() || "play.projects.gg";
  return page.serverPort > 0 && page.serverPort !== 25565 ? `${host}:${page.serverPort}` : host;
}

const isRunning = (a: AfkAccount) =>
  a.state.kind === "starting" || a.state.kind === "connecting" || a.state.kind === "connected";

// --- Console ----------------------------------------------------------------------------------

const logs = new Map<string, AfkLogLine[]>([
  [
    "afk1",
    [
      {
        id: ++lineId,
        time: now(),
        text: "Sunucuya bağlanılıyor: play.projects.gg",
        kind: "system",
      },
      { id: ++lineId, time: now(), text: "/login ********", kind: "script" },
      { id: ++lineId, time: now(), text: "Başarıyla giriş yaptın.", kind: "chat" },
      { id: ++lineId, time: now(), text: "<AlevKanat> akşam boss'a gelen var mı?", kind: "chat" },
      { id: ++lineId, time: now(), text: "<Demirci> ben varım", kind: "chat" },
    ],
  ],
]);

function push(id: string, text: string, kind: AfkLogLine["kind"]) {
  const list = logs.get(id) ?? [];
  list.push({ id: ++lineId, time: now(), text, kind });
  if (list.length > 500) list.splice(0, list.length - 500);
  logs.set(id, list);
}

/** A preview switch of the page's address. */
function param(name: string): string | null {
  try {
    return new URLSearchParams(window.location.search).get(name);
  } catch {
    return null;
  }
}

/** A line's words and their looks (AFK-UX: console looks on this computer, as pl-afk gives them). */
function parts(...list: [text: string, code: string, event?: number][]) {
  return {
    text: list.map(([text]) => text).join(""),
    st: list.map(([text, code, event]) =>
      event === undefined ? [text.length, code] : [text.length, code, event],
    ),
  };
}

/**
 * Preview switches of KasaBekcisi's console: `?looks=1` the evening's lines in the game's colours
 * (a client 4.2.4 while Studio's switch is on), `?lines=<n>` that many chat lines before them (the
 * panel's drag measured over a full console).
 */
function seedConsole() {
  const list = logs.get("afk1");
  if (!list) return;
  const extra = Math.min(500, Number(param("lines")) || 0);
  for (let i = 0; i < extra; i++) {
    const name = ["AlevKanat", "Demirci", "GeceKurdu_", "Lavci_42"][i % 4] ?? "Demirci";
    list.push({
      id: ++lineId,
      time: now(),
      text: `<${name}> pazarda elmas kazma ${i + 1} kaldı, isteyen var mı`,
      kind: "chat",
    });
  }
  if (param("looks") !== "1") return;
  const line = (look: { text: string; st: unknown }, ev?: unknown, who?: string): AfkLogLine => ({
    id: ++lineId,
    time: now(),
    kind: "chat",
    ...look,
    ...(ev ? { ev } : {}),
    who: who ?? null,
  });
  list.push(
    line(parts(["[Duyuru] ", "6l", 0], ["Boss etkinliği 21:00'de başlıyor.", "e"]), [
      {
        hover: {
          text: "Sunucu duyurusu\nKatılmak için: /etkinlik",
          st: [
            [16, "e"],
            [24, "7"],
          ],
        },
      },
    ]),
    line(
      parts(["[Parti] ", "9"], ["GeceKurdu_", "b", 0], [": lobiye geliyorum", "f"]),
      [
        {
          click: { a: "suggest_command", v: "/msg GeceKurdu_ " },
          hover: { entity: { name: "GeceKurdu_", st: [[10, "b"]] } },
        },
      ],
      "GeceKurdu_",
    ),
    line(
      parts(["[Yetkili] ", "cl"], ["Mod_Kirpi", "c"], [": herkes spawn'a", "7"]),
      undefined,
      "Mod_Kirpi",
    ),
    line(parts(["SessizOkcu sana fısıldıyor: pazar açık mı?", "7o"]), undefined, "SessizOkcu"),
    line(
      parts(
        ["[Parti] ", "9"],
        ["GeceKurdu_ seni partiye çağırıyor ", "7"],
        ["[Kabul et]", "al", 0],
      ),
      [
        {
          click: { a: "run_command", v: "/party accept GeceKurdu_" },
          hover: { text: "Partiye katılmak için tıkla", st: [[27, "a"]] },
        },
      ],
    ),
    line(parts(["Kurallar: ", "7"], ["forum.projects.gg/kurallar", "bn", 0]), [
      { click: { a: "open_url", v: "https://forum.projects.gg/kurallar" } },
    ]),
    line(parts(["Eski fiyat ", "7"], ["900", "7m"], [" yeni fiyat ", "7"], ["600 ripple", "an"])),
  );
}
seedConsole();

/**
 * A client 4.2.5's dialog as a real form (CONSOLE-ANY): an item part, a checkbox, radio buttons, a
 * slider in the game's steps and words, a multiline field, tooltips and widths.
 */
const FORM_DIALOG = {
  title: "Klan kur",
  titleSt: [[8, "6l"]],
  body: [
    { text: "Klanını kur, ilk sezon ödülünü al." },
    {
      text: "Sezon ödülü",
      item: { id: "minecraft:diamond", count: 3, label: { tr: "Elmas", en: "Diamond" } },
    },
  ],
  inputs: [
    { key: "name", kind: "text", label: "Klan adı", max: 16, initial: "" },
    {
      key: "color",
      kind: "option",
      label: "Renk",
      options: [
        { id: "red", label: "Kırmızı", labelSt: [[7, "c"]] },
        { id: "blue", label: "Mavi", labelSt: [[4, "9"]] },
        { id: "green", label: "Yeşil", labelSt: [[5, "a"]] },
      ],
      initial: "blue",
    },
    {
      key: "size",
      kind: "range",
      label: "Üye sınırı",
      start: 2,
      end: 20,
      step: 2,
      initial: 8,
      fmt: "%s: %s",
    },
    {
      key: "open",
      kind: "bool",
      label: "Herkes katılabilsin",
      initial: true,
      onTrue: "acik",
      onFalse: "kapali",
    },
    {
      key: "motto",
      kind: "text",
      label: "Klan sözü",
      max: 120,
      multiline: true,
      lines: 3,
      initial: "",
    },
  ],
  buttons: [
    {
      i: 1,
      label: "Kur",
      labelSt: [[3, "a"]],
      a: "run_command",
      v: "/klan kur $(name) $(color) $(size) $(open)",
      tpl: true,
      tip: "Klanı bu ayarlarla kurar",
    },
    { i: 2, label: "Klan kuralları", a: "open_url", v: "https://forum.projects.gg/klan", w: 120 },
  ],
  cancel: true,
  play: true,
  cols: 2,
  after: "wait",
  exit: { label: "Vazgeç" },
};

/**
 * Preview switch `?dialog=1`: KasaBekcisi's server shows its rules as a dialog (a client 4.2.4
 * while Studio's dialog cards switch is on). Any press closes it, as the preview's server does.
 * `?dialog=form`: a client 4.2.5's form (CONSOLE-ANY); `?dialog=wait`: the game's waiting screen;
 * `?menus=1`: what its pause screen and quick actions key open.
 */
let mockDialog: { rev: number; dialog: string; at: string; wait?: boolean } | null =
  param("dialog") === "form"
    ? { rev: 2, at: now(), dialog: JSON.stringify(FORM_DIALOG) }
    : param("dialog") === "wait"
      ? { rev: 3, at: now(), dialog: "", wait: true }
      : param("dialog") === "1"
        ? {
            rev: 1,
            at: now(),
            dialog: JSON.stringify({
              title: "Sunucu Kuralları",
              titleSt: [[16, "6l"]],
              body: [
                { text: "1. Saygılı ol, küfür etme.\n2. Hile ve makro yok.\n3. Reklam yapma." },
              ],
              inputs: [
                {
                  key: "ok",
                  kind: "bool",
                  label: "Kuralları okudum",
                  initial: false,
                  onTrue: "evet",
                  onFalse: "hayir",
                },
              ],
              buttons: [
                {
                  i: 0,
                  label: "Kabul et",
                  labelSt: [[8, "a"]],
                  a: "run_command",
                  v: "/kurallar kabul $(ok)",
                },
                {
                  i: 1,
                  label: "Forumda oku",
                  a: "open_url",
                  v: "https://forum.projects.gg/kurallar",
                },
              ],
              cancel: true,
              play: true,
              cols: 2,
            }),
          }
        : null;
const dialogListeners = new Set<(accountId: string) => void>();

function setState(account: AfkAccount, state: AfkAccount["state"]) {
  account.state = state;
  notify();
}

/**
 * Preview switch `?afk=fail`: five seconds after start the connected account is banned and stops
 * for good, as the backend reports it (the notice of features/afk/use-afk-watch.ts).
 */
function failSoon() {
  try {
    if (new URLSearchParams(window.location.search).get("afk") !== "fail") return;
  } catch {
    return;
  }
  setTimeout(() => {
    const account = accounts.find((a) => a.state.kind === "connected");
    if (!account) return;
    account.health = undefined;
    account.food = undefined;
    account.players = undefined;
    push(account.id, "You are banned from this server!", "system");
    setState(account, {
      kind: "error",
      // Worded like the API layer words a `refused` kick (tauri.ts `failure`).
      reason: `${i18n.t("afk.refusal.banned")} (You are banned from this server!)`,
    });
  }, 5000);
}
failSoon();

// --- The AFK client, fetched on the first connect (pl_afk::client) -----------------------------

type ClientPreview = "client" | "noclient" | "offline" | "clientfail";

/**
 * Preview switches for a launcher without the AFK client yet: `?afk=client` fetches it on the
 * first connect (cancellable), `?afk=noclient` finds none published, `?afk=offline` cannot reach
 * the content, `?afk=clientfail` loses the connection halfway through the download. A failure
 * happens once: "Tekrar dene" then fetches the client.
 */
function clientPreview(): ClientPreview | null {
  try {
    const value = new URLSearchParams(window.location.search).get("afk");
    return value === "client" ||
      value === "noclient" ||
      value === "offline" ||
      value === "clientfail"
      ? value
      : null;
  } catch {
    return null;
  }
}

let clientMode = clientPreview();
let clientReady = clientMode === null;
/** Accounts waiting for the client; the last one to leave stops the download. */
const waiting = new Set<string>();
let preparation: Promise<AfkClientProblem | "ready" | "cancelled"> | null = null;

function showClient(state: AfkState) {
  for (const account of accounts) if (waiting.has(account.id)) account.state = state;
  notify();
}

async function fetchClient(): Promise<AfkClientProblem | "ready" | "cancelled"> {
  const mode = clientMode;
  // Fails once; the next attempt gets the client.
  clientMode = "client";
  showClient({ kind: "preparing", step: "checking" });
  await delay(mode === "offline" ? 1600 : 700);
  if (!waiting.size) return "cancelled";
  if (mode === "noclient") return "notPublished";
  if (mode === "offline") return "offline";
  for (let percent = 0; percent < 100; percent += 3) {
    if (!waiting.size) return "cancelled";
    if (mode === "clientfail" && percent >= 40) return "offline";
    showClient({ kind: "preparing", step: "downloading", percent });
    await delay(180);
  }
  showClient({ kind: "preparing", step: "installing" });
  await delay(800);
  return waiting.size ? "ready" : "cancelled";
}

/** Waits for the client with these accounts. True for the ones still waiting once it is ready. */
async function waitForClient(list: AfkAccount[]): Promise<AfkAccount[]> {
  if (clientReady) return list;
  for (const account of list) waiting.add(account.id);
  preparation ??= fetchClient().finally(() => {
    preparation = null;
  });
  const outcome = await preparation;
  const still = list.filter((account) => waiting.delete(account.id));
  if (outcome === "ready") {
    clientReady = true;
    return still;
  }
  if (outcome !== "cancelled") {
    for (const account of still) account.state = { kind: "clientFailed", problem: outcome };
    notify();
  }
  return [];
}

async function connect(account: AfkAccount) {
  if (!(await waitForClient([account])).length) return;
  account.settingsChanged = false;
  setState(account, { kind: "starting" });
  await delay(700);
  setState(account, { kind: "connecting" });
  push(account.id, `Sunucuya bağlanılıyor: ${account.server}`, "system");
  await delay(1400);
  account.health = 20;
  account.food = 20;
  account.players = 211;
  setState(account, { kind: "connected", since: now() });
  push(account.id, "Başarıyla giriş yaptın.", "chat");
}

export const mockAfk = {
  async afkAccounts() {
    return accounts.map((a) => ({
      ...a,
      // Worded in the language of the moment, as the real list is read again on a switch.
      state:
        a.state.kind === "error" && a.state.cause === "wrongPassword" ? refusedPassword() : a.state,
      running: isRunning(a),
    }));
  },
  async afkAdd(nickname: string) {
    const account: AfkAccount = {
      id: `afk-${nickname}`,
      nickname,
      server: "play.projects.gg",
      autoStart: false,
      mcVersion: RECOMMENDED,
      mcVersionRecommended: true,
      state: { kind: "stopped" },
      settingsChanged: false,
      showDate: false,
    };
    accounts.push(account);
    stored.set(account.id, defaultPage());
    storedVersions.set(account.id, "");
    notify();
    return account;
  },
  async afkRemove(id: string) {
    const index = accounts.findIndex((a) => a.id === id);
    if (index >= 0) accounts.splice(index, 1);
    stored.delete(id);
    storedVersions.delete(id);
    keychain.delete(id);
    notify();
  },
  async afkConnect(id: string) {
    const account = accounts.find((a) => a.id === id);
    if (account) void connect(account);
  },
  async afkStop(id: string) {
    const account = accounts.find((a) => a.id === id);
    if (!account) return;
    // Waiting for the client (or failed to get it): it just stops waiting.
    if (waiting.delete(id) || account.state.kind === "clientFailed") {
      setState(account, { kind: "stopped" });
      return;
    }
    setState(account, { kind: "stopping" });
    push(id, "/quit", "self");
    setTimeout(() => setState(account, { kind: "stopped" }), 900);
  },
  async afkReconnect(id: string) {
    const account = accounts.find((a) => a.id === id);
    if (!account) throw accountGone();
    if (account.state.kind !== "stopped" && account.state.kind !== "error") {
      setState(account, { kind: "stopping" });
      push(id, "/quit", "self");
      await delay(900);
    }
    void connect(account);
  },
  async afkConnectAll() {
    const idle = accounts.filter(
      (a) =>
        a.state.kind === "stopped" || a.state.kind === "error" || a.state.kind === "clientFailed",
    );
    // The run's accounts wait for the client together, then connect one by one.
    void waitForClient(idle).then((ready) =>
      ready.forEach((account, index) => {
        if (index === 0) {
          void connect(account);
          return;
        }
        setState(account, { kind: "queued", inSecs: index * 15 });
        setTimeout(() => void connect(account), index * 15_000);
      }),
    );
  },
  async afkStopAll() {
    for (const a of accounts) if (a.state.kind !== "stopped") void mockAfk.afkStop(a.id);
  },
  async afkSend(id: string, line: string) {
    const account = accounts.find((a) => a.id === id);
    if (!account || !isRunning(account))
      throw commandError(
        "AFK-BAGLI-DEGIL",
        pick("Bu hesap şu an bağlı değil.", "This account isn't connected right now."),
      );
    push(id, `> ${line}`, "self");
    notify();
  },
  async afkOpenFolder(id: string) {
    if (!accounts.some((a) => a.id === id)) throw accountGone();
  },
  async afkLog(id: string) {
    return [...(logs.get(id) ?? [])];
  },
  async afkDialog(id: string) {
    const account = accounts.find((a) => a.id === id);
    return id === "afk1" && account && isRunning(account) ? mockDialog : null;
  },
  async afkDialogMenus(id: string) {
    const account = accounts.find((a) => a.id === id);
    return id === "afk1" && account && isRunning(account) && param("menus") === "1"
      ? JSON.stringify({
          pause: { label: "Sunucu kuralları", labelSt: [[16, "6"]] },
          quick: { label: "Hızlı Eylemler..." },
        })
      : null;
  },
  async afkDialogPress(id: string, press: { rev: number } | { open: string }) {
    await delay(200);
    if ("open" in press) return { ok: false, why: "menu" };
    if (!mockDialog || id !== "afk1") return { ok: false, why: "stale" };
    if (press.rev !== mockDialog.rev) return { ok: false, why: "stale" };
    // The preview's server closes its dialog after any press.
    mockDialog = null;
    for (const listener of dialogListeners) listener(id);
    return { ok: true, why: null };
  },
  onAfkDialog(listener: (accountId: string) => void) {
    dialogListeners.add(listener);
    return () => {
      dialogListeners.delete(listener);
    };
  },
  async afkSettings(id: string): Promise<AfkSettingsView> {
    await delay(250);
    const page = stored.get(id);
    if (!page) throw accountGone();
    const version = resolveVersion(storedVersions.get(id) ?? "");
    const shown = redact(structuredClone(page), keychain.get(id) ?? []);
    return {
      settings: {
        ...shown,
        autoRelog: {
          ...shown.autoRelog,
          kickMessages: kickMessagesNow(shown.autoRelog.kickMessages),
        },
        minecraftVersion: version.value,
      },
      versions,
      unknownVersion: version.unknown,
      keychainAvailable: true,
      sensitiveCommands: SENSITIVE,
    };
  },
  async afkSaveSettings(id: string, page: SettingsPage): Promise<AfkSettingsSaveResult> {
    await delay(450);
    const previous = stored.get(id);
    const account = accounts.find((a) => a.id === id);
    if (!previous || !account) throw accountGone();

    // Kept passwords are resolved against the "keychain"; a changed list is refused.
    const secrets = keychain.get(id) ?? [];
    const commands: string[] = [];
    for (const line of page.loginCommands.commands) {
      if (line.kind === "text") {
        commands.push(line.text);
        continue;
      }
      const current = secrets[line.index];
      const name = current ? secretName(current) : null;
      if (!current || !name || name.toUpperCase() !== line.command.toUpperCase())
        throw commandError(
          "AFK-AYAR-DEGISTI",
          pick(
            "Giriş komutları bu sırada değişmiş. Ayarları kapatıp yeniden aç.",
            "The login commands changed in the meantime. Close the settings and open them again.",
          ),
        );
      commands.push(current);
    }
    const login = clean(commands);

    // `AccountSettings::apply`: host check, then the legacy "enabled but empty" checks.
    let host = page.serverHost.trim();
    let port = clamp(Math.round(page.serverPort) || 0, 0, 65535);
    const split = /^([^:]+):(\d{1,5})$/.exec(host);
    if (
      split?.[1] &&
      split[2] &&
      port === 0 &&
      Number(split[2]) >= 1 &&
      Number(split[2]) <= 65535
    ) {
      host = split[1].trim();
      port = Number(split[2]);
    }
    const issues: ValidationIssue[] = [];
    if (host && !(host.length <= 253 && /^[A-Za-z0-9_][A-Za-z0-9._-]*$/.test(host)))
      issues.push("invalidServerHost");
    if (page.loginCommands.enabled && !login.length) issues.push("loginCommandsEmpty");
    const rules = page.autoRespond.rules
      .filter(
        (r) => r.pattern.trim() && (r.reply.trim() || r.privateReply.trim() || r.otherReply.trim()),
      )
      .map((r) => ({
        ...r,
        pattern: r.pattern.trim(),
        cooldownSeconds: clamp(r.cooldownSeconds, 0, 86_400),
      }));
    if (page.autoRespond.enabled) {
      if (!rules.length) issues.push("autoRespondNoRules");
      page.autoRespond.rules.forEach((rule, index) => {
        const problem = checkPattern(rule.pattern, rule.isRegex);
        if (problem) issues.push({ autoRespondPattern: { rule: index, problem } });
      });
    }
    const tasks = page.scheduler.tasks
      .filter((task) => task.action.trim())
      .map((task) => {
        const [minSeconds, maxSeconds] = ordered(
          clamp(task.minSeconds, 1, 604_800),
          clamp(task.maxSeconds, 1, 604_800),
        );
        return {
          ...task,
          name: task.name.trim() || "Görev",
          action: task.action.trim(),
          minSeconds,
          maxSeconds,
        };
      });
    if (page.scheduler.enabled && !tasks.length) issues.push("schedulerNoTasks");
    if (issues.length) return { status: "invalid", issues };

    const [antiMin, antiMax] = ordered(page.antiAfk.delayMin, page.antiAfk.delayMax);
    const [relogMin, relogMax] = ordered(page.autoRelog.delayMin, page.autoRelog.delayMax);
    const next: SettingsPage = {
      ...page,
      serverHost: host,
      serverPort: port,
      loginCommands: { ...page.loginCommands, commands: [] },
      scheduler: { ...page.scheduler, tasks },
      antiAfk: {
        ...page.antiAfk,
        delayMin: antiMin,
        delayMax: antiMax,
        command: page.antiAfk.command.trim(),
      },
      autoRelog: {
        ...page.autoRelog,
        delayMin: relogMin,
        delayMax: relogMax,
        kickMessages: clean(page.autoRelog.kickMessages),
      },
      autoRespond: { ...page.autoRespond, rules },
      alerts: {
        ...page.alerts,
        matches: clean(page.alerts.matches),
        excludes: clean(page.alerts.excludes),
      },
    };
    const changed =
      JSON.stringify(next) !== JSON.stringify(previous) ||
      JSON.stringify(login) !== JSON.stringify(secrets);
    stored.set(id, next);
    if (login.length) keychain.set(id, login);
    else keychain.delete(id);

    storedVersions.set(id, toStoredVersion(next.minecraftVersion));
    const version = resolveVersion(storedVersions.get(id) ?? "");
    account.server = serverLabel(next);
    account.autoStart = next.autoStart;
    account.mcVersion = version.value;
    account.mcVersionRecommended = version.recommended;
    account.showDate = next.chatLog.showDate;
    if (changed && isRunning(account)) account.settingsChanged = true;
    notify();
    return {
      status: "saved",
      settings: redact(structuredClone(next), login),
      reconnectNeeded: Boolean(account.settingsChanged && isRunning(account)),
    };
  },
  async afkCheckPattern(pattern: string, isRegex: boolean) {
    return checkPattern(pattern, isRegex);
  },
  async afkGlobalSettings(): Promise<AfkSettings> {
    return { ...globalSettings };
  },
  async afkSetShowScriptCommands(show: boolean) {
    globalSettings.showScriptCommands = show;
  },
  onAfkChange(listener: () => void) {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  },
};
