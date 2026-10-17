// Mock `api.modHealth` for the browser preview. Preview-only switches:
// - `localStorage["pl.mock.mods"]`: "blocked" (problems that stop the game), "warnings" (only
//   warnings), "partial" (like "blocked", but one problem survives "Onar"); empty = healthy.
// - `localStorage["pl.mock.crash"]`: the next launch crashes: "mod", "memory", "reserve",
//   "graphics" or "java" (read by the mock launch in mock.ts).
import type {
  CrashKind,
  CrashSuspect,
  HealthIssue,
  MemoryAdvice,
  ModCheck,
  ModHealthApi,
  ReinstallPhase,
  RepairStep,
  RepairStepKind,
  StepNote,
} from "./modhealth-types";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

function flag(key: string): string {
  try {
    return localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

export const mockModsScenario = () => flag("pl.mock.mods");
export const mockCrashScenario = () => flag("pl.mock.crash");

interface MockIssue extends HealthIssue {
  /** Steps "Onar" runs for it. */
  fix: Array<{ kind: RepairStepKind; name: string; note?: StepNote; fails?: boolean }>;
}

function issues(scenario: string): MockIssue[] {
  const blocking: MockIssue[] = [
    {
      key: "missing-sodium",
      severity: "blocking",
      kind: "missingDependency",
      mods: ["Sodium Extra"],
      target: "Sodium",
      bundled: false,
      files: ["sodium-extra-0.6.0.jar"],
      offer: { label: "fix", count: 1 },
      fix: [{ kind: "fetch", name: "Sodium" }],
    },
    {
      key: "duplicate-lithium",
      severity: "blocking",
      kind: "duplicate",
      mods: ["Lithium"],
      bundled: false,
      files: ["lithium-0.12.1.jar", "lithium-0.14.3.jar"],
      offer: { label: "disableOld", count: 1 },
      fix: [{ kind: "disableOld", name: "Lithium" }],
    },
    {
      key: "minecraft-xaero",
      severity: "blocking",
      kind: "incompatibleMinecraft",
      mods: ["Xaero's Minimap"],
      bundled: false,
      files: ["xaeros-minimap-24.2.jar"],
      offer: { label: "fix", count: 1 },
      fix: [
        scenario === "partial"
          ? { kind: "update", name: "Xaero's Minimap", note: "noVersion", fails: true }
          : { kind: "update", name: "Xaero's Minimap" },
      ],
    },
    {
      key: "breaks-optifabric",
      severity: "blocking",
      kind: "breaks",
      mods: ["Iris"],
      target: "OptiFabric",
      bundled: false,
      files: ["iris-1.8.jar", "optifabric-1.14.jar"],
      offer: { label: "disableNamed", name: "OptiFabric", count: 1 },
      fix: [{ kind: "disable", name: "OptiFabric" }],
    },
  ];
  const warnings: MockIssue[] = [
    {
      key: "loader-create",
      severity: "warning",
      kind: "wrongLoader",
      mods: ["Create"],
      madeFor: "Forge",
      bundled: false,
      files: ["create-1.21.1-6.0.jar"],
      offer: { label: "fix", count: 1 },
      fix: [{ kind: "update", name: "Create", note: "disabledInstead" }],
    },
    {
      key: "conflicts-cit",
      severity: "warning",
      kind: "conflicts",
      mods: ["Continuity"],
      target: "CIT Resewn",
      bundled: false,
      files: ["continuity-3.0.jar", "citresewn-1.2.jar"],
      offer: { label: "disableNamed", name: "CIT Resewn", count: 1 },
      fix: [{ kind: "disable", name: "CIT Resewn" }],
    },
  ];
  switch (scenario) {
    case "blocked":
    case "partial":
      return [...blocking, ...warnings.slice(0, 1)];
    case "warnings":
      return warnings;
    default:
      return [];
  }
}

/** Issues fixed in this preview session, per instance, and issues "Onar" already tried. */
const fixed = new Map<string, Set<string>>();
const attempted = new Map<string, Set<string>>();
const cancelled = new Set<string>();

function open(instanceId: string): MockIssue[] {
  const done = fixed.get(instanceId) ?? new Set<string>();
  return issues(mockModsScenario()).filter((i) => !done.has(i.key));
}

function check(instanceId: string): ModCheck {
  const list = open(instanceId);
  return {
    instanceId,
    minecraft: "1.21.1",
    loader: "fabric",
    issues: list.map(({ fix: _fix, ...issue }) => issue),
    blocking: list.filter((i) => i.severity === "blocking").length,
    checkedFiles: 38,
    repairable: list.some((i) => !attempted.get(instanceId)?.has(i.key)),
    hasMods: true,
  };
}

function markFixed(instanceId: string, key: string) {
  const set = fixed.get(instanceId) ?? new Set<string>();
  set.add(key);
  fixed.set(instanceId, set);
}

export const mockModHealth: ModHealthApi = {
  async check(instanceId) {
    await delay(260);
    return check(instanceId);
  },
  async repair(instanceId, scope, jobId, onProgress) {
    const crash = scope.kind === "crash";
    const targets =
      scope.kind === "issue"
        ? open(instanceId).filter((i) => i.key === scope.key)
        : open(instanceId);
    const plan: Array<{ step: RepairStep; issue?: string; fails?: boolean }> = [];
    if (scope.kind !== "issue") {
      plan.push({ step: { key: "pack", kind: "verifyPack", name: "", status: "pending" } });
    }
    if (crash) {
      plan.push({
        step: { key: "disable:sodium.jar", kind: "disable", name: "Sodium", status: "pending" },
      });
      plan.push({
        step: { key: "fetch:indium", kind: "fetch", name: "Indium", status: "pending" },
      });
    }
    for (const issue of targets) {
      for (const s of issue.fix) {
        plan.push({
          step: { key: `${issue.key}:${s.kind}`, kind: s.kind, name: s.name, status: "pending" },
          issue: issue.key,
          fails: s.fails,
        });
      }
    }
    const tried = attempted.get(instanceId) ?? new Set<string>();
    for (const issue of targets) tried.add(issue.key);
    attempted.set(instanceId, tried);
    const notes = new Map<string, StepNote | undefined>();
    for (const issue of targets) {
      for (const s of issue.fix) notes.set(`${issue.key}:${s.kind}`, s.note);
    }
    const order: RepairStepKind[] = [
      "verifyPack",
      "redownloadPack",
      "revertPack",
      "redownload",
      "enable",
      "disable",
      "disableOld",
      "loader",
      "java",
      "update",
      "fetch",
    ];
    plan.sort((a, b) => order.indexOf(a.step.kind) - order.indexOf(b.step.kind));
    const steps = () => plan.map((p) => ({ ...p.step }));
    const emit = (percent?: number, checking = false) =>
      onProgress({ steps: steps(), percent, checking });
    emit();
    await delay(420);
    let checked = false;
    for (const entry of plan) {
      if (cancelled.has(jobId)) break;
      const network = entry.step.kind === "update" || entry.step.kind === "fetch";
      if (network && !checked) {
        checked = true;
        emit(undefined, true);
        await delay(650);
      }
      entry.step.status = "running";
      emit();
      if (network || entry.step.kind === "verifyPack") {
        for (let p = 8; p <= 100; p += 18) {
          if (cancelled.has(jobId)) break;
          await delay(160);
          emit(Math.min(p, 100));
        }
      } else {
        await delay(380);
      }
      if (cancelled.has(jobId)) break;
      entry.step.status = entry.fails ? "failed" : "done";
      entry.step.note = notes.get(entry.step.key);
      if (!entry.fails && entry.issue) markFixed(instanceId, entry.issue);
      emit();
      await delay(140);
    }
    const stopped = cancelled.delete(jobId);
    if (stopped) {
      for (const entry of plan) {
        if (entry.step.status === "pending" || entry.step.status === "running") {
          entry.step.status = "skipped";
          entry.step.note = "cancelled";
        }
      }
    }
    emit(undefined, true);
    await delay(420);
    return { steps: steps(), check: check(instanceId), cancelled: stopped };
  },
  async reinstallPreview() {
    await delay(320);
    return {
      minecraft: "1.21.1",
      resetsSettings: true,
      kept: ["Better Clouds", "Sodium Extra"],
      packMods: 24,
      modrinthMods: 6,
      needsNetwork: false,
    };
  },
  async reinstall(instanceId, jobId, onProgress) {
    const phases: ReinstallPhase[] = ["keepAside", "trash", "pack", "modrinth", "check"];
    for (const phase of phases) {
      const cancellable = phase === "keepAside";
      if (cancellable && cancelled.delete(jobId)) {
        return {
          kept: [],
          packSynced: false,
          reinstalled: 0,
          restored: [],
          settingsReset: false,
          check: check(instanceId),
          cancelled: true,
        };
      }
      onProgress({ phases, phase, cancellable });
      if (phase === "pack" || phase === "modrinth") {
        for (let p = 6; p <= 100; p += 9) {
          await delay(110);
          onProgress({ phases, phase, percent: Math.min(p, 100), cancellable: false });
        }
      } else {
        await delay(520);
      }
    }
    for (const issue of issues(mockModsScenario())) markFixed(instanceId, issue.key);
    return {
      kept: ["Better Clouds", "Sodium Extra"],
      keptFolder: ".projects/kept-mods/1.21.1",
      packSynced: true,
      reinstalled: 30,
      restored: mockModsScenario() === "partial" ? ["Xaero's Minimap"] : [],
      settingsReset: true,
      check: check(instanceId),
      cancelled: false,
    };
  },
  async setMaxMemory(_instanceId, maxMb) {
    await delay(120);
    return maxMb;
  },
  async setJvmArgs(_instanceId, _jvmArgs) {
    await delay(120);
    return "-XX:+UseFancyThing";
  },
  async cancel(jobId) {
    cancelled.add(jobId);
  },
};

/** The crash a mock launch reports for `localStorage["pl.mock.crash"]`. */
export function mockCrash(instanceId: string, scenario: string) {
  const base = {
    instanceId,
    logFile: "C:\\Users\\Oyuncu\\AppData\\Roaming\\ProjectsLauncher\\logs\\game-projects.log",
    crashReport: null as string | null,
    actions: [] as string[],
    mods: [] as string[],
    suspects: [] as CrashSuspect[],
    repairable: false,
    turnOff: null as { file: string; name: string } | null,
    reinstall: false,
    memory: null as MemoryAdvice | null,
  };
  const kinds: Record<string, CrashKind> = {
    mod: "modError",
    memory: "outOfMemory",
    reserve: "memoryReserve",
    graphics: "graphicsDriver",
    java: "javaTooOld",
  };
  const kind = kinds[scenario] ?? "unknown";
  switch (scenario) {
    case "mod":
      return {
        ...base,
        kind,
        title: "Bir mod oyunun açılmasını engelledi",
        hint: "Şu mod oyunun açılmasını engelledi: Sodium.",
        mods: ["Sodium"],
        suspects: [
          { name: "Sodium", reason: "mixinFailed" as const, strong: true },
          {
            name: "Iris",
            reason: "declaresIncompatible" as const,
            related: "OptiFabric",
            strong: true,
          },
          { name: "Indium", reason: "inStackTrace" as const, strong: false },
        ],
        repairable: true,
        turnOff: { file: "sodium-fabric-0.6.0.jar", name: "Sodium" },
        reinstall: true,
      };
    case "memory":
      return {
        ...base,
        kind,
        title: "Oyunun belleği yetmedi",
        hint: "Oyun, ayrılan bellek dolduğu için kapandı.",
        memory: { currentMb: 4096, suggestedMb: 6144, increase: true },
      };
    case "reserve":
      return {
        ...base,
        kind,
        title: "Ayrılan bellek çok yüksek",
        hint: "En yüksek bellek, bilgisayarının verebileceğinden fazla.",
        memory: { currentMb: 12288, suggestedMb: 6144, increase: false },
      };
    case "graphics":
      return {
        ...base,
        kind,
        title: "Ekran kartı hazırlanamadı",
        hint: "Ekran kartı sürücünü güncelleyip yeniden dene.",
      };
    case "java":
      return {
        ...base,
        kind,
        title: "Java sürümü bu oyun için eski",
        hint: "Bu sürüm Java 21 veya daha yenisini gerektiriyor.",
        repairable: true,
      };
    default:
      return { ...base, kind, title: "Oyun beklenmedik şekilde kapandı", hint: "" };
  }
}
