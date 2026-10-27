import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { enCloudAfk } from "@/locales/en-cloud-afk";
import { trCloudAfk } from "@/locales/tr-cloud-afk";
import type { CloudAccount, CloudEditData } from "./api";
import { looksSensitive } from "./sensitive";
import {
  type Draft,
  heldIssue,
  isDirty,
  issueKey,
  localIssues,
  newCommand,
  newRule,
  nextId,
  PARTS,
  partOf,
  portOf,
  serverChanges,
  textProblem,
  toDraft,
  toEdit,
  typed,
  unsavedParts,
  withTrigger,
} from "./settings-model";

const NOW = 1_800_000_000;

function view(): CloudEditData {
  const account: CloudAccount = {
    id: "a_TEST0000000000001",
    name: "Madenci07",
    server: { host: "mc.madenciler.net", port: 25570 },
    version: "26.3",
    lang: "tr",
    host: "kz1",
    createdAt: NOW - 86_400,
    expiresAt: NOW + 18 * 86_400,
    expired: false,
    graceUntil: NOW + 48 * 86_400,
    renewal: { open: false, opensAt: NOW + 11 * 86_400 },
    desired: "run",
    hold: null,
    live: { state: "running", since: NOW - 60, reason: null, attempt: null, retryAt: null },
    everJoined: true,
    config: {
      v: 1,
      owner: "",
      antiAfk: { enabled: true, delayMin: 60, delayMax: 90 },
      commands: [],
      rules: [],
      inventory: null,
      movement: null,
    },
    rev: 4,
    serverChanges: { used: 0, limit: 6, resetAt: null },
    tier: "plain",
    plusUntil: null,
    ai: null,
    features: null,
  };
  return {
    account,
    config: {
      owner: "KasaBekcisi",
      antiAfk: { enabled: true, delayMin: 60, delayMax: 90 },
      commands: [
        {
          id: "c1",
          enabled: true,
          on: "join",
          delay: 3,
          every: null,
          at: null,
          words: { kind: "secret", text: "/login Gizli123", kept: true },
        },
        {
          id: "c2",
          enabled: true,
          on: "interval",
          delay: null,
          every: { min: 300, max: 600 },
          at: null,
          words: { kind: "plain", text: "/afk" },
        },
      ],
      rules: [
        {
          id: "r1",
          enabled: true,
          pattern: "giriş yap",
          regex: false,
          from: "server",
          ownerOnly: false,
          cooldown: 30,
          reply: { kind: "sealed", preview: "/login ******" },
        },
      ],
      inventory: null,
      movement: null,
    },
    versions: { versions: [], recommended: "26.3" },
    limits: { minIntervalSec: 60, minRuleCooldownSec: 10, maxCommandRows: 20, maxRules: 10 },
  };
}

const LIMITS = { minIntervalSec: 60, minRuleCooldownSec: 10 };

/** The contract's shared text cases, copied into the crate with a `//` header naming the source. */
const shared = JSON.parse(
  readFileSync(
    new URL(
      "../../../../../../crates/pl-cloud-afk/tests/vectors/text-rule-v1.json",
      import.meta.url,
    ),
    "utf8",
  )
    .split("\n")
    .filter((line) => !line.startsWith("//"))
    .join("\n"),
) as {
  cases: {
    name: string;
    text: string | null;
    utf16?: string[];
    units: number;
    problem: string | null;
    sensitive?: boolean;
  }[];
};

function draftWith(change: (draft: Draft) => void): Draft {
  const draft = toDraft(view());
  change(draft);
  return draft;
}

const problems = (draft: Draft) =>
  localIssues(draft, LIMITS).map((i) => `${i.place}:${i.index ?? "-"}:${i.field}:${i.problem}`);

describe("textProblem", () => {
  it("follows the contract's text rule", () => {
    expect(textProblem("/login %username%")).toBeNull();
    expect(textProblem("Merhaba %SERVERIP%:%serverport%")).toBeNull();
    expect(textProblem("")).toBe("empty");
    expect(textProblem("a".repeat(257))).toBe("length");
    expect(textProblem("a".repeat(256))).toBeNull();
    expect(textProblem(" /afk")).toBe("space");
    expect(textProblem("/afk ")).toBe("space");
    expect(textProblem("/afk\nselam")).toBe("control");
    expect(textProblem("a\u2028b")).toBe("control");
    expect(textProblem("\\say")).toBe("backslash");
    expect(textProblem("\u00a0\\say")).toBe("backslash");
    expect(textProblem("%player% selam")).toBe("variable");
  });
});

describe("the shared text cases", () => {
  it("come out as the contract says, a lone surrogate included", () => {
    expect(shared.cases).toHaveLength(60);
    for (const sample of shared.cases) {
      const units = (sample.utf16 ?? []).map((unit) => Number.parseInt(unit, 16));
      const text = sample.text ?? String.fromCharCode(...units);
      expect(text.length, sample.name).toBe(sample.units);
      expect(textProblem(text), sample.name).toBe(sample.problem);
      if (sample.sensitive !== undefined) {
        expect(looksSensitive(text), sample.name).toBe(sample.sensitive);
      }
    }
  });
});

describe("localIssues", () => {
  it("finds nothing wrong in a good draft, sealed words included", () => {
    expect(problems(toDraft(view()))).toEqual([]);
  });

  it("checks the name, the address and the port", () => {
    expect(problems(draftWith((d) => (d.name = "ab")))).toEqual(["general:-:name:format"]);
    expect(problems(draftWith((d) => (d.host = "mc madenciler.net")))).toEqual([
      "general:-:server:format",
    ]);
    expect(problems(draftWith((d) => (d.port = "70000")))).toEqual(["general:-:port:port"]);
    expect(problems(draftWith((d) => (d.port = "")))).toEqual([]);
  });

  it("checks the owner and the anti-AFK range", () => {
    const owner = draftWith((d) => (d.config.owner = "x y"));
    expect(problems(owner)).toEqual(["general:-:owner:format"]);
    const range = draftWith(
      (d) => (d.config.antiAfk = { enabled: true, delayMin: 90, delayMax: 60 }),
    );
    expect(problems(range)).toEqual(["general:-:antiAfk:range"]);
  });

  it("checks each trigger's own fields", () => {
    const short = draftWith((d) => {
      const row = d.config.commands[1];
      if (row) row.every = { min: 30, max: 600 };
    });
    expect(problems(short)).toEqual(["commands:1:every:range"]);
    const order = draftWith((d) => {
      const row = d.config.commands[1];
      if (row) row.every = { min: 900, max: 600 };
    });
    expect(problems(order)).toEqual(["commands:1:every:order"]);
    const daily = draftWith((d) => {
      const row = d.config.commands[1];
      if (row) Object.assign(row, { on: "daily", every: null, at: ["08:00", "08:00"] });
    });
    expect(problems(daily)).toEqual(["commands:1:at:duplicate"]);
    const late = draftWith((d) => {
      const row = d.config.commands[0];
      if (row) row.delay = 61;
    });
    expect(problems(late)).toEqual(["commands:0:delay:range"]);
  });

  it("checks auto-replies", () => {
    const owner = draftWith((d) => {
      d.config.owner = "";
      const rule = d.config.rules[0];
      if (rule) Object.assign(rule, { ownerOnly: true, from: "player" });
    });
    expect(problems(owner)).toEqual(["rules:0:ownerOnly:owner"]);
    const server = draftWith((d) => {
      const rule = d.config.rules[0];
      if (rule) rule.ownerOnly = true;
    });
    expect(problems(server)).toEqual(["rules:0:ownerOnly:server"]);
    const fast = draftWith((d) => {
      const rule = d.config.rules[0];
      if (rule) Object.assign(rule, { cooldown: 5, pattern: " " });
    });
    expect(problems(fast)).toEqual(["rules:0:match:empty", "rules:0:cooldown:range"]);
  });
});

describe("heldIssue", () => {
  const stopped = (cause: string, row: string | null): CloudEditData => {
    const base = view();
    return { ...base, account: { ...base.account, hold: { kind: "cause", cause, at: NOW, row } } };
  };

  it("marks the row a stop names while its words are the stored ones", () => {
    const held = stopped("invalidCommand", "c2");
    expect(heldIssue(held, toDraft(held))).toEqual({
      place: "commands",
      index: 1,
      field: "text",
      problem: "backslash",
    });
    const unreadable = stopped("secretUnreadable", "r1");
    expect(heldIssue(unreadable, toDraft(unreadable))).toEqual({
      place: "rules",
      index: 0,
      field: "reply",
      problem: "unreadable",
    });
  });

  it("lets go once the row is rewritten, turned off or gone", () => {
    const held = stopped("invalidCommand", "c2");
    const rewritten = toDraft(held);
    const row = rewritten.config.commands[1];
    if (row) row.words = { kind: "plain", text: "/afk2" };
    expect(heldIssue(held, rewritten)).toBeNull();
    const off = toDraft(held);
    const second = off.config.commands[1];
    if (second) second.enabled = false;
    expect(heldIssue(held, off)).toBeNull();
    const gone = toDraft(held);
    gone.config.commands.splice(1, 1);
    expect(heldIssue(held, gone)).toBeNull();
  });

  it("marks nothing without a row or for another cause", () => {
    const none = stopped("invalidCommand", null);
    expect(heldIssue(none, toDraft(none))).toBeNull();
    const other = stopped("wrongPassword", "c1");
    expect(heldIssue(other, toDraft(other))).toBeNull();
    expect(heldIssue(view(), toDraft(view()))).toBeNull();
  });
});

describe("toEdit", () => {
  it("sends only what changed", () => {
    const baseline = toDraft(view());
    const draft = draftWith((d) => (d.name = "Madenci08"));
    expect(isDirty(draft, baseline)).toBe(true);
    expect(toEdit(draft, baseline, view())).toEqual({
      rev: 4,
      name: "Madenci08",
      server: null,
      version: null,
      lang: null,
      config: null,
    });
  });

  it("sends the config with a new server, its seals name the server", () => {
    const baseline = toDraft(view());
    const draft = draftWith((d) => (d.host = "Play.Projects.gg"));
    expect(serverChanges(draft, view())).toBe(true);
    const edit = toEdit(draft, baseline, view());
    expect(edit.server).toEqual({ host: "Play.Projects.gg", port: 25570 });
    expect(edit.config).toEqual(draft.config);
  });

  it("reads an empty port as the default and a typed one as a number", () => {
    expect(portOf(draftWith((d) => (d.port = "")))).toBe(0);
    expect(portOf(draftWith((d) => (d.port = " 25565 ")))).toBe(25565);
    expect(portOf(draftWith((d) => (d.port = "25a")))).toBeNaN();
    expect(
      serverChanges(
        draftWith((d) => (d.host = "MC.Madenciler.NET")),
        view(),
      ),
    ).toBe(false);
  });
});

describe("rows", () => {
  it("numbers new rows across commands and rules", () => {
    const { config } = view();
    expect(nextId(config, "c")).toBe("c3");
    expect(nextId(config, "r")).toBe("r2");
    expect(newCommand(config)).toMatchObject({ id: "c3", on: "join", delay: 2 });
    expect(newRule(config, 45)).toMatchObject({ id: "r2", cooldown: 45, from: "any" });
    expect(newRule(config, 10).cooldown).toBe(30);
  });

  it("gives a new trigger its own starting values", () => {
    const row = newCommand(view().config);
    expect(withTrigger(row, "interval", 60)).toMatchObject({
      on: "interval",
      delay: null,
      every: { min: 300, max: 600 },
    });
    expect(withTrigger(row, "interval", 900).every).toEqual({ min: 900, max: 1800 });
    expect(withTrigger(row, "daily", 60)).toMatchObject({
      on: "daily",
      at: ["08:00"],
      every: null,
    });
    expect(withTrigger(withTrigger(row, "daily", 60), "join", 60)).toMatchObject({
      on: "join",
      delay: 2,
      at: null,
    });
  });

  it("makes a password command secret as it is typed", () => {
    expect(typed({ kind: "plain", text: "" }, "/login abc")).toEqual({
      kind: "secret",
      text: "/login abc",
      kept: false,
    });
    expect(typed({ kind: "plain", text: "" }, "/warp maden")).toEqual({
      kind: "plain",
      text: "/warp maden",
    });
    expect(typed({ kind: "secret", text: "x", kept: true }, "/warp")).toMatchObject({
      kind: "secret",
      kept: false,
    });
  });
});

describe("issueKey", () => {
  it("names a text both languages have for every issue", () => {
    const fields: Record<string, string[]> = {
      name: ["format", "duplicate"],
      server: ["format", "dot", "private", "self", "port", "blocked", "full", "changeLimit", "x"],
      port: ["port"],
      version: ["unknown"],
      antiAfk: ["range"],
      owner: ["format"],
      count: ["max"],
      delay: ["range"],
      every: ["range", "order"],
      at: ["format", "duplicate", "empty", "many", "x"],
      cooldown: ["range"],
      ownerOnly: ["owner", "server"],
      match: ["empty", "tooLong", "invalid", "tooComplex", "x"],
      text: [
        "empty",
        "length",
        "unicode",
        "unreadable",
        "space",
        "control",
        "backslash",
        "variable",
        "secret",
        "reseal",
        "x",
      ],
      reply: ["secret", "x"],
      config: ["reseal", "x"],
    };
    for (const [field, list] of Object.entries(fields)) {
      for (const problem of list) {
        const key = issueKey({ place: "general", index: null, field, problem });
        expect(trCloudAfk.settings.issues, `${field}/${problem}`).toHaveProperty(key);
        expect(enCloudAfk.settings.issues, `${field}/${problem}`).toHaveProperty(key);
      }
    }
  });
});

describe("the panel's sections (SHEET-UX)", () => {
  it("shows a problem in the section that holds its field", () => {
    const at = (place: string, field: string) =>
      partOf({ place, index: null, field, problem: "format" });
    expect(at("general", "name")).toBe("connection");
    expect(at("general", "server")).toBe("connection");
    expect(at("general", "port")).toBe("connection");
    expect(at("general", "antiAfk")).toBe("game");
    expect(at("general", "movement")).toBe("game");
    expect(at("general", "owner")).toBe("replies");
    expect(at("commands", "text")).toBe("commands");
    expect(at("rules", "reply")).toBe("replies");
  });

  it("marks only the sections whose fields changed", () => {
    const base = toDraft(view());
    expect(unsavedParts(base, base).size).toBe(0);
    expect([...unsavedParts({ ...base, port: "25571" }, base)]).toEqual(["connection"]);
    const owner = { ...base, config: { ...base.config, owner: "Baska" } };
    expect([...unsavedParts(owner, base)]).toEqual(["replies"]);
    const anti = {
      ...base,
      config: { ...base.config, antiAfk: { ...base.config.antiAfk, enabled: false } },
    };
    expect([...unsavedParts(anti, base)]).toEqual(["game"]);
    expect(PARTS).toEqual(["connection", "commands", "game", "replies"]);
  });
});
