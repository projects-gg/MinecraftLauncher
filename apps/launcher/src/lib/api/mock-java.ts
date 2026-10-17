// Browser preview of Java (J1): Settings → Java and the question before a launch.
//
// Preview switches in the address:
//   ?java=updating   an update of Java 25 is downloading (held at 37%)
//   ?java=staged     a game runs; the update waits for it to close
//   ?java=empty      nothing downloaded yet, no Java on the computer
//   ?javaPrompt=tooOld | tooNew | missing | broken | mods
//                    OYNA stops with the Java question (read by the mock launch in mock.ts)
import type { JavaApi, JavaOverview, JavaPrompt, JavaRuntime } from "./java-types";

const params = () => new URLSearchParams(window.location.search);
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

const changeListeners = new Set<() => void>();
const progressListeners = new Set<(id: string, percent: number) => void>();
const emitChange = () => {
  for (const listener of changeListeners) listener();
};
const emitProgress = (id: string, percent: number) => {
  for (const listener of progressListeners) listener(id, percent);
};

const managed = (
  major: number,
  component: string,
  version: string,
  extra: Partial<JavaRuntime> = {},
): JavaRuntime => ({
  id: `managed:${component}/windows-x64`,
  kind: "managed",
  major,
  version,
  vendor: "Mojang",
  location: `C:\\Users\\Ejder\\AppData\\Roaming\\ProjectsLauncher\\runtimes\\${component}\\windows-x64`,
  executable: `C:\\Users\\Ejder\\AppData\\Roaming\\ProjectsLauncher\\runtimes\\${component}\\windows-x64\\bin\\javaw.exe`,
  bits32: false,
  users: [],
  component,
  platform: "windows-x64",
  staged: false,
  queued: false,
  updateFailed: false,
  ...extra,
});

function initialRuntimes(): JavaRuntime[] {
  const scenario = params().get("java");
  if (scenario === "empty") return [];
  return [
    managed(25, "java-runtime-epsilon", "25.0.1", {
      users: [
        { instanceId: "projects", name: "Projects", automatic: true },
        { instanceId: "vanilla", name: "Vanilla", automatic: true },
      ],
      update: "25.0.2",
      updating: scenario === "updating" ? 37 : undefined,
      staged: scenario === "staged",
    }),
    managed(21, "java-runtime-delta", "21.0.7", {
      users: [{ instanceId: "yaratici", name: "Yaratıcı dünya", automatic: true }],
    }),
    managed(8, "jre-legacy", "8u51", { removableBytes: 224_395_264 }),
    {
      id: "path:C:\\Program Files\\Eclipse Adoptium\\jdk-17.0.12.7-hotspot\\bin\\javaw.exe",
      kind: "system",
      major: 17,
      version: "17.0.12",
      vendor: "Eclipse Temurin",
      location: "C:\\Program Files\\Eclipse Adoptium\\jdk-17.0.12.7-hotspot",
      executable: "C:\\Program Files\\Eclipse Adoptium\\jdk-17.0.12.7-hotspot\\bin\\javaw.exe",
      bits32: false,
      users: [],
      staged: false,
      queued: false,
      updateFailed: false,
    },
    {
      id: "path:C:\\Program Files (x86)\\Java\\jre1.8.0_431\\bin\\javaw.exe",
      kind: "system",
      major: 8,
      version: "1.8.0_431",
      vendor: "Oracle",
      location: "C:\\Program Files (x86)\\Java\\jre1.8.0_431",
      executable: "C:\\Program Files (x86)\\Java\\jre1.8.0_431\\bin\\javaw.exe",
      bits32: true,
      users: [{ instanceId: "hardcore", name: "Hardcore macera", automatic: false, fit: "tooOld" }],
      staged: false,
      queued: false,
      updateFailed: false,
    },
    {
      id: "path:D:\\Oyunlar\\java\\bin\\javaw.exe",
      kind: "custom",
      location: "D:\\Oyunlar\\java",
      executable: "D:\\Oyunlar\\java\\bin\\javaw.exe",
      bits32: false,
      users: [{ instanceId: "eski-test", name: "Eski test", automatic: false }],
      staged: false,
      queued: false,
      updateFailed: false,
      problem: "missing",
    },
  ];
}

let runtimes: JavaRuntime[] | null = null;
const state = () => {
  runtimes ??= initialRuntimes();
  return runtimes;
};

/** Plays an update: progress ticks, then the new build replaces the old one. */
function playUpdate(id: string) {
  let percent = 0;
  const tick = () => {
    const row = state().find((r) => r.id === id);
    if (!row) return;
    percent = Math.min(100, percent + 2.5 + Math.random() * 3);
    row.updating = percent;
    emitProgress(id, percent);
    if (percent < 100) {
      setTimeout(tick, 120);
      return;
    }
    setTimeout(() => {
      row.version = row.update ?? row.version;
      row.update = undefined;
      row.updating = undefined;
      emitChange();
    }, 350);
  };
  setTimeout(tick, 120);
}

/** Java picked with "Gözat" in the preview (`?javaPick=broken` picks a file that is no Java). */
const PICKED_JAVA = "D:\\Araçlar\\jdk-21.0.4\\bin\\javaw.exe";

export const mockJava: JavaApi = {
  async check(_instanceId, path) {
    await delay(260);
    const known = state().find((r) => r.executable === path);
    // The preview's instances play 1.21.x: they need Java 21.
    const need = 21;
    if (path.includes("bozuk") || known?.problem) {
      return {
        executable: path,
        bits32: false,
        problem: known?.problem ?? "broken",
        need,
        forMods: false,
      };
    }
    const major = known?.major ?? (path === PICKED_JAVA ? 21 : 17);
    return {
      executable: path,
      major,
      version: known?.version ?? (major === 21 ? "21.0.4" : "17.0.12"),
      vendor: known?.vendor ?? "Eclipse Temurin",
      bits32: known?.bits32 ?? false,
      fit: major < need ? "tooOld" : undefined,
      need,
      forMods: false,
    };
  },
  async pickExecutable() {
    await delay(120);
    return params().get("javaPick") === "broken" ? "D:\\İndirilenler\\bozuk.exe" : PICKED_JAVA;
  },
  async overview(rescan) {
    await delay(rescan ? 900 : 380);
    return {
      runtimes: state().map((r) => ({ ...r, users: [...r.users] })),
      systemIncomplete: false,
      checkedAt: Date.now() - 3 * 3_600_000,
      gameActive: params().get("java") === "staged",
    } satisfies JavaOverview;
  },
  async update(component, platform) {
    await delay(120);
    const id = `managed:${component}/${platform}`;
    const row = state().find((r) => r.id === id);
    if (!row) return;
    row.updateFailed = false;
    row.updating = 0;
    emitChange();
    playUpdate(id);
  },
  async remove(component, platform) {
    await delay(260);
    const id = `managed:${component}/${platform}`;
    const index = state().findIndex((r) => r.id === id);
    if (index < 0) return 0;
    const [row] = state().splice(index, 1);
    return row?.removableBytes ?? 0;
  },
  async switchToAuto(instanceIds) {
    await delay(220);
    let count = 0;
    const target = state().find((r) => r.id === "managed:java-runtime-delta/windows-x64");
    for (const row of state()) {
      const moving = row.users.filter((u) => !u.automatic && instanceIds.includes(u.instanceId));
      if (moving.length === 0) continue;
      row.users = row.users.filter((u) => !moving.includes(u));
      count += moving.length;
      target?.users.push(...moving.map((u) => ({ ...u, automatic: true, fit: undefined })));
    }
    // A custom path nobody uses any more leaves the list.
    runtimes = state().filter((r) => r.kind !== "custom" || r.users.length > 0);
    return count;
  },
  async openFolder() {},
  onChange(listener) {
    changeListeners.add(listener);
    return () => changeListeners.delete(listener);
  },
  onProgress(listener) {
    progressListeners.add(listener);
    return () => progressListeners.delete(listener);
  },
};

/**
 * Instances use a custom Java in the preview while the Java question or a Java crash
 * (`localStorage["pl.mock.crash"] = "java"`, mock-modhealth.ts) is being previewed.
 */
export function mockCustomJava(): boolean {
  try {
    return params().has("javaPrompt") || localStorage.getItem("pl.mock.crash") === "java";
  } catch {
    return false;
  }
}

/** The question the mock launch asks (`?javaPrompt=…`), or null. */
export function mockJavaPrompt(instanceId: string, instanceName: string): JavaPrompt | null {
  const scenario = params().get("javaPrompt");
  const base = { instanceId, instanceName, forMods: false, fixable: true, automatic: true };
  switch (scenario) {
    case "tooOld":
      return { ...base, problem: "tooOld", have: 17, need: 25 };
    case "mods":
      return { ...base, problem: "tooOld", have: 17, need: 21, forMods: true, automatic: false };
    case "tooNew":
      return { ...base, problem: "tooNew", have: 21, need: 8 };
    case "missing":
      return { ...base, problem: "missing", need: 25 };
    case "broken":
      return { ...base, problem: "broken", need: 25 };
    default:
      return null;
  }
}
