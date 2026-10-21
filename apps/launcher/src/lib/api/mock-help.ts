// Browser preview of `api.help` (the troubleshooter): every row settles after its own delay, as
// the real checks do. `?help=` (or `localStorage["pl.mock.help"]`) picks what they find:
// `fine` (the default), `problems`, `offline`, `clock`, `java`, `warnings`; `?helpSlow=1`
// stretches the delays (screens of the checking state).
import type { HelpApi, HelpCheck, HelpCrash, HelpFinding, HelpReport } from "./help-types";
import { mockCrash } from "./mock-modhealth";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const GB = 1024 * 1024 * 1024;
const MB = 1024 * 1024;

function flag(key: string): string {
  try {
    return (
      new URLSearchParams(window.location.search).get(key) ??
      new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key) ??
      localStorage.getItem(`pl.mock.${key}`) ??
      ""
    );
  } catch {
    return "";
  }
}

/** When each row settles (ms), roughly as the real checks do. */
const SETTLES: Record<HelpCheck, number> = {
  disk: 140,
  crash: 190,
  java: 320,
  memory: 330,
  internet: 640,
  clock: 650,
  content: 730,
  friends: 810,
  discord: 870,
  microsoft: 920,
  server: 1060,
  mods: 1280,
};

/** Rows that appear only with a finding. */
const CONDITIONAL: readonly HelpCheck[] = ["clock", "memory"];

const finding = (
  check: HelpCheck,
  level: HelpFinding["level"],
  code: HelpFinding["code"],
  extra: Partial<Omit<HelpFinding, "check" | "level" | "code">> = {},
): HelpFinding => ({ check, level, code, facts: {}, ...extra });

const hoursAgo = (hours: number) => new Date(Date.now() - hours * 3_600_000).toISOString();

function fine(game: boolean): HelpFinding[] {
  return [
    finding("internet", "ok", "internetOk", { facts: { latencyMs: 38 } }),
    finding("server", "ok", "serverOk", { facts: { latencyMs: 42, players: 1_204 } }),
    finding("content", "ok", "contentOk"),
    finding("friends", "ok", "friendsOk"),
    finding("discord", "ok", "discordOk"),
    finding("disk", "ok", "diskOk", { facts: { freeBytes: 48.6 * GB } }),
    ...(game
      ? [
          finding("java", "ok", "javaReady", { facts: { major: 21 } }),
          finding("mods", "ok", "modsOk", { facts: { count: 48 } }),
          finding("crash", "ok", "crashNone"),
        ]
      : []),
  ];
}

function replace(base: HelpFinding[], changes: HelpFinding[]): HelpFinding[] {
  const out = base.filter((row) => !changes.some((change) => change.check === row.check));
  return [...out, ...changes];
}

function scenario(name: string, game: boolean): HelpFinding[] {
  const base = fine(game);
  switch (name) {
    case "problems":
      return replace(base, [
        finding("server", "warning", "serverSlow", { facts: { latencyMs: 340, players: 1_188 } }),
        finding("disk", "problem", "diskFull", {
          fix: "makeSpace",
          facts: { freeBytes: 820 * MB },
        }),
        ...(game
          ? [
              finding("mods", "problem", "modsBlocking", { fix: "repair", facts: { count: 2 } }),
              finding("crash", "problem", "crashRecent", {
                fix: "crashDetails",
                facts: { at: hoursAgo(26), crash: "modError", name: "Sodium" },
              }),
            ]
          : []),
      ]);
    case "warnings":
      return replace(base, [
        finding("content", "warning", "contentUnreachable"),
        finding("disk", "warning", "diskLow", { fix: "makeSpace", facts: { freeBytes: 1.6 * GB } }),
      ]);
    case "offline":
      return replace(base, [
        finding("internet", "problem", "internetOffline"),
        finding("server", "skipped", "serverNeedsInternet"),
        finding("content", "skipped", "contentNeedsInternet"),
        finding("friends", "skipped", "friendsNeedsInternet"),
        finding("discord", "skipped", "discordNeedsInternet"),
      ]);
    case "clock":
      return replace(base, [
        finding("internet", "problem", "internetInsecure", { fix: "clockSettings" }),
        finding("clock", "problem", "clockWrong", {
          fix: "clockSettings",
          facts: { minutes: -3 * 24 * 60 },
        }),
        finding("server", "ok", "serverOk", { facts: { latencyMs: 44, players: 1_204 } }),
        finding("content", "warning", "contentUnreachable"),
        finding("friends", "warning", "friendsUnreachable"),
        finding("discord", "warning", "discordUnreachable"),
      ]);
    case "java":
      return replace(base, [
        ...(game
          ? [
              finding("java", "problem", "javaTooOld", {
                fix: "installJava",
                facts: { major: 8, need: 21 },
              }),
              finding("memory", "problem", "memoryTooHigh", { fix: "memoryAuto" }),
            ]
          : []),
      ]);
    default:
      return base;
  }
}

export function createMockHelp(): HelpApi {
  return {
    async run(instanceId, onEvent) {
      const game = instanceId !== null;
      const findings = scenario(flag("help") || "fine", game);
      const stretch = flag("helpSlow") === "1" ? 4 : 1;
      const checks = findings
        .map((row) => row.check)
        .filter((check) => !CONDITIONAL.includes(check));
      onEvent({ type: "plan", checks });
      const ordered = [...findings].sort((a, b) => SETTLES[a.check] - SETTLES[b.check]);
      let elapsed = 0;
      for (const row of ordered) {
        const at = SETTLES[row.check] * stretch;
        await delay(at - elapsed);
        elapsed = at;
        onEvent({ type: "finding", finding: row });
      }
      const report: HelpReport = {
        findings: ordered,
        system: {
          launcher: "2.0.1-beta.1.6",
          debug: true,
          os: "Windows 11 Pro 24H2",
          arch: "x86_64",
          memoryMb: 16_271,
        },
        instance: game
          ? {
              kind: "official",
              minecraft: "26.1.2",
              loader: "fabric",
              loaderVersion: "0.17.2",
              packVersion: "1.4.2",
            }
          : undefined,
      };
      return report;
    },
    async crash(instanceId): Promise<HelpCrash | null> {
      await delay(120);
      const name = flag("help");
      return name === "problems" ? (mockCrash(instanceId, "mod") as HelpCrash) : null;
    },
    async openClock() {
      await delay(60);
    },
  };
}
