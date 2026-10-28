import { describe, expect, it } from "vitest";
import type { SettingsPage } from "@/lib/api";
import {
  fromDraft,
  fromServerIssues,
  GROUP_OF,
  GROUPS,
  isDirty,
  isSecretLine,
  isValidHost,
  newRule,
  newTask,
  passwordIssue,
  SECTIONS,
  splitHostPort,
  toDraft,
  unsavedGroups,
  validate,
} from "./model";

const page = (): SettingsPage => ({
  serverHost: "",
  serverPort: 0,
  minecraftVersion: "26.3",
  autoStart: false,
  allowResourcePacks: false,
  loginCommands: {
    enabled: false,
    onlyFirstLogin: false,
    delaySeconds: 2,
    commands: [
      { kind: "stored", index: 0, command: "/login" },
      { kind: "text", text: "/warp afk" },
    ],
  },
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
    kickMessages: ["Server is full"],
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

describe("server address (mirrors form.rs)", () => {
  it("accepts host names and IPs, rejects the rest", () => {
    for (const ok of ["", "  ", "play.projects.gg", " mc.example.org ", "192.168.1.20", "a_b-c."])
      expect(isValidHost(ok), ok).toBe(true);
    for (const bad of [
      "play projects.gg",
      "https://play.projects.gg",
      "play.projects.gg/x",
      "play.projects.gg:25565",
      ".projects.gg",
      "-projects.gg",
      "sunucu.türk",
      "a".repeat(254),
    ])
      expect(isValidHost(bad), bad).toBe(false);
  });

  it("splits host:port only for a real port", () => {
    expect(splitHostPort(" play.example.net:25566 ")).toEqual(["play.example.net", 25566]);
    expect(splitHostPort("play.example.net")).toBeNull();
    expect(splitHostPort("play.example.net:0")).toBeNull();
    expect(splitHostPort("play.example.net:70000")).toBeNull();
    expect(splitHostPort("::1:25565")).toBeNull();
  });
});

describe("draft", () => {
  it("round-trips the page and keeps stored passwords as references", () => {
    const original = page();
    const draft = toDraft(original);
    expect(fromDraft(draft)).toEqual(original);

    const stored = draft.login[0];
    if (stored?.kind !== "stored") throw new Error("expected a stored line");
    draft.login[0] = { ...stored, kind: "replace", args: " yeniSifre " };
    expect(fromDraft(draft).loginCommands.commands[0]).toEqual({
      kind: "text",
      text: "/login yeniSifre",
    });
  });

  it("is dirty only for changes that saving would keep", () => {
    const baseline = toDraft(page());
    const same = toDraft(page());
    same.page = {
      ...same.page,
      serverHost: "  ",
      autoRelog: { ...same.page.autoRelog, kickMessages: ["  Server is full ", ""] },
    };
    expect(isDirty(same, baseline)).toBe(false);

    const changed = toDraft(page());
    changed.page = { ...changed.page, antiAfk: { ...changed.page.antiAfk, delayMin: Number.NaN } };
    expect(isDirty(changed, baseline)).toBe(true);
  });

  it("takes 'Değiştir' alone for no change, a typed password for one (AFK-CLEAR)", () => {
    const baseline = toDraft(page());
    const opened = toDraft(page());
    const stored = opened.login[0];
    if (stored?.kind !== "stored") throw new Error("expected a stored line");
    opened.login[0] = { ...stored, kind: "replace", args: "  " };
    expect(isDirty(opened, baseline)).toBe(false);

    opened.login[0] = { ...stored, kind: "replace", args: "yeniSifre" };
    expect(isDirty(opened, baseline)).toBe(true);
  });
});

describe("validation", () => {
  it("reports format problems right away and empty lists after a save attempt", () => {
    const draft = toDraft(page());
    draft.page = {
      ...draft.page,
      serverHost: "mc.example.org:25566",
      serverPort: 25565,
      loginCommands: { ...draft.page.loginCommands, enabled: true },
      scheduler: { enabled: true, tasks: [] },
      antiAfk: { ...draft.page.antiAfk, walkRange: 1.5, delayMax: 5000 },
    };
    draft.login = [];
    const issues = validate(draft);
    const codes = issues.map((i) => `${i.field}:${i.code}${i.structural ? "*" : ""}`);
    expect(codes).toEqual([
      "serverHost:hostPort",
      "login.list:loginEmpty*",
      "scheduler.list:schedulerEmpty*",
      "antiAfk.max:range",
      "antiAfk.walkRange:integer",
    ]);
  });

  it("checks rows only while their feature is on", () => {
    const draft = toDraft(page());
    const task = { ...newTask(), minSeconds: 0 };
    const rule = { ...newRule(), pattern: "selam" };
    draft.tasks = [task];
    draft.rules = [rule];
    expect(validate(draft)).toEqual([]);

    draft.page = {
      ...draft.page,
      scheduler: { ...draft.page.scheduler, enabled: true },
      autoRespond: { ...draft.page.autoRespond, enabled: true },
    };
    const fields = validate(draft).map((i) => `${i.field}:${i.code}`);
    expect(fields).toEqual([
      `task.${task.key}.action:taskAction`,
      `task.${task.key}.min:range`,
      `rule.${rule.key}.reply:ruleReply`,
    ]);

    // Emptied lists get the list-level message instead.
    draft.tasks = [];
    draft.rules = [];
    expect(validate(draft).map((i) => i.code)).toEqual(["schedulerEmpty", "respondEmpty"]);
  });

  it("maps backend issues to sections", () => {
    const draft = toDraft(page());
    expect(fromServerIssues(["invalidServerHost", "schedulerNoTasks"], draft)).toEqual([
      { section: "server", field: "serverHost", code: "hostInvalid" },
      { section: "scheduler", field: "scheduler.list", code: "schedulerEmpty" },
    ]);
  });

  it("places backend pattern problems on their rule", () => {
    const draft = toDraft(page());
    const first = newRule();
    const second = newRule();
    draft.rules = [first, second];
    expect(
      fromServerIssues(
        [
          { autoRespondPattern: { rule: 1, problem: "invalid" } },
          { autoRespondPattern: { rule: 0, problem: "tooComplex" } },
          // A rule the sheet no longer has: the list carries the message.
          { autoRespondPattern: { rule: 7, problem: "tooLong" } },
        ],
        draft,
      ),
    ).toEqual([
      { section: "respond", field: `rule.${second.key}.pattern`, code: "patternInvalid" },
      { section: "respond", field: `rule.${first.key}.pattern`, code: "patternComplex" },
      { section: "respond", field: "respond.list", code: "patternLong" },
    ]);
  });

  it("asks for reconnect delays of at least five seconds", () => {
    const draft = toDraft(page());
    draft.page.autoRelog.delayMin = 2;
    expect(validate(draft).map((i) => `${i.field}:${i.code}:${i.limit?.min}`)).toEqual([
      "relog.min:range:5",
    ]);
  });
});

it("recognizes password commands like the console does", () => {
  const sensitive = ["/login", "/giriş", "/l"];
  expect(isSecretLine("/login abc", sensitive)).toBe(true);
  expect(isSecretLine("  /GIRIŞ  abc", sensitive)).toBe(true);
  expect(isSecretLine("/login", sensitive)).toBe(false);
  expect(isSecretLine("/list", sensitive)).toBe(false);
  expect(isSecretLine("merhaba /login x", sensitive)).toBe(false);
});

describe("the panel's sections (SHEET-UX)", () => {
  it("puts every feature in one of five sections, each holding something", () => {
    expect(Object.keys(GROUP_OF).sort()).toEqual([...SECTIONS].sort());
    for (const group of GROUPS) expect(Object.values(GROUP_OF)).toContain(group);
  });

  it("marks only the sections whose fields would change on save", () => {
    const base = toDraft(page());
    expect(unsavedGroups(base, base).size).toBe(0);
    const host = {
      ...base,
      page: { ...base.page, serverHost: "mc.madenciler.net", serverPort: 25570 },
    };
    expect([...unsavedGroups(host, base)]).toEqual(["connection"]);
    const eat = { ...base, page: { ...base.page, autoEat: { enabled: true, threshold: 6 } } };
    expect([...unsavedGroups(eat, base)]).toEqual(["game"]);
    const login = {
      ...base,
      login: [...base.login, { key: "x", kind: "text" as const, text: "/spawn" }],
    };
    expect([...unsavedGroups(login, base)]).toEqual(["commands"]);
  });

  it("ignores what saving would not change (spaces around the host)", () => {
    const base = toDraft(page());
    const spaced = { ...base, page: { ...base.page, serverHost: "  " } };
    expect(unsavedGroups(spaced, base).size).toBe(0);
  });
});

describe("a stop for a refused password (AFK-CLEAR)", () => {
  const sensitive = ["/login", "/l", "/register"];
  const on = (draft: ReturnType<typeof toDraft>) => ({
    ...draft,
    page: { ...draft.page, loginCommands: { ...draft.page.loginCommands, enabled: true } },
  });

  it("marks the kept login line, never blocking, until it is being replaced", () => {
    const base = on(toDraft(page()));
    const first = base.login[0];
    expect(passwordIssue(base, base, sensitive)).toEqual({
      section: "login",
      field: `login.${first?.key}`,
      code: "wrongPasswordKept",
    });
    const replacing = {
      ...base,
      login: base.login.map((row) =>
        row.kind === "stored" ? { ...row, kind: "replace" as const, args: "" } : row,
      ),
    };
    expect(passwordIssue(replacing, base, sensitive)).toBeNull();
    // The login commands are off: they typed no password.
    expect(passwordIssue(toDraft(page()), toDraft(page()), sensitive)).toBeNull();
  });

  it("marks a typed password line while it has its saved words", () => {
    const typed = page();
    typed.loginCommands.commands = [{ kind: "text", text: "/login eski1" }];
    const base = on(toDraft(typed));
    const line = base.login[0];
    expect(passwordIssue(base, base, sensitive)?.code).toBe("wrongPassword");
    const edited = {
      ...base,
      login: base.login.map((row) =>
        row.kind === "text" && row.key === line?.key ? { ...row, text: "/login yeni2" } : row,
      ),
    };
    expect(passwordIssue(edited, base, sensitive)).toBeNull();
  });

  it("falls back to the auto-reply that types the password", () => {
    const replies = page();
    replies.loginCommands.commands = [];
    replies.autoRespond = {
      enabled: true,
      soundEnabled: false,
      matchColors: false,
      rules: [
        { ...newRule(), pattern: "selam", reply: "Merhaba" },
        { ...newRule(), pattern: "giriş yap", reply: "/login gizli1" },
      ],
    };
    const base = toDraft(replies);
    const rule = base.rules[1];
    expect(passwordIssue(base, base, sensitive)).toEqual({
      section: "respond",
      field: `rule.${rule?.key}.reply`,
      code: "wrongPassword",
    });
    expect(GROUP_OF.respond).toBe("replies");
  });
});
