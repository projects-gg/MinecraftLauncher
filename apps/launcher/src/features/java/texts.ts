// Sentences and labels of the Java UI (J1). Kept free of React so they can be tested.
import type { TFunction } from "i18next";
import type { JavaPrompt, JavaRuntime } from "@/lib/api";

/** The question's title: what the game needs, or what is wrong with the instance's Java. */
export function promptTitle(t: TFunction, prompt: JavaPrompt): string {
  return t(`javaPrompt.title.${prompt.problem}`, { need: prompt.need });
}

/** The one sentence under the title: what "Kur ve oyna" changes. */
export function promptBody(t: TFunction, prompt: JavaPrompt): string {
  if (!prompt.fixable) return t("javaPrompt.body.unfixable");
  return prompt.automatic
    ? t("javaPrompt.body.automatic")
    : t("javaPrompt.body.managed", { need: prompt.need });
}

/**
 * A Java's version as a player reads it: the release without vendor build tags
 * ("8u51-cacert462b08" → "8u51", "25.0.3+9.1" → "25.0.3", "21.0.7" stays).
 */
export function readableVersion(version: string): string {
  return version.split(/[-+]/)[0]?.trim() || version;
}

/** A row's title: "Java 21", or what is wrong with a custom path. */
export function runtimeTitle(t: TFunction, runtime: JavaRuntime): string {
  if (runtime.problem === "missing") return t("java.missing");
  if (runtime.problem === "broken" || runtime.major === undefined) return t("java.broken");
  return t("java.title", { major: runtime.major });
}

/**
 * A folder path a player can read at a glance: the drive and the last two folders
 * (`C:\…\Eclipse Adoptium\jdk-17.0.12`). Short paths stay whole.
 */
export function shortPath(path: string): string {
  const separator = path.includes("\\") ? "\\" : "/";
  const parts = path.split(/[\\/]+/).filter((part, i) => part !== "" || i === 0);
  if (parts.length <= 4) return path;
  const head = parts[0] === "" ? "" : parts[0];
  return [head, "…", ...parts.slice(-2)].join(separator);
}

/**
 * The Java folder a path points into, for comparing (`…\jdk-21\bin\javaw.exe` → `…/jdk-21`,
 * any case, either slash).
 */
export function javaHome(path: string): string {
  return path
    .trim()
    .replace(/[\\/]+/g, "/")
    .replace(/\/$/, "")
    .toLowerCase()
    .replace(/\/bin\/javaw?(\.exe)?$/, "")
    .replace(/\/bin$/, "");
}

/** Two paths name the same Java (its `java.exe`, `javaw.exe`, `bin` folder or folder). */
export function sameJava(a: string, b: string): boolean {
  return Boolean(a.trim()) && javaHome(a) === javaHome(b);
}

/** Runtimes the official Minecraft Launcher downloaded (`.minecraft\runtime`, its Store app). */
const MINECRAFT_LAUNCHER =
  /[\\/](?:\.minecraft|Minecraft Launcher)[\\/]runtime[\\/]|Microsoft\.4297127D64EC6_8wekyb3d8bbwe/i;

/** Where a Java lives, as a player reads it. */
export function locationLabel(t: TFunction, runtime: JavaRuntime): string {
  if (runtime.kind === "managed") return t("java.launcherFolder");
  if (MINECRAFT_LAUNCHER.test(runtime.location)) return t("java.minecraftLauncher");
  return shortPath(runtime.location);
}

/** Launcher runtimes first (newest Java first), then the computer's, then custom paths. */
export function groupRuntimes(runtimes: JavaRuntime[]) {
  return {
    managed: runtimes.filter((r) => r.kind === "managed"),
    system: runtimes.filter((r) => r.kind === "system"),
    custom: runtimes.filter((r) => r.kind === "custom"),
  };
}

/** Instances on a custom or system Java that "Otomatik'e geç" moves. */
export function switchableUsers(runtime: JavaRuntime): string[] {
  if (runtime.kind === "managed") return [];
  return runtime.users.filter((user) => !user.automatic).map((user) => user.instanceId);
}

/** What the row's action area shows. */
export type RuntimeAction =
  | { kind: "updating"; percent: number }
  | { kind: "waiting" }
  | { kind: "failed" }
  | { kind: "update"; version: string }
  | { kind: "remove"; bytes: number }
  | { kind: "useAuto"; instanceIds: string[] }
  | { kind: "none" };

export function runtimeAction(runtime: JavaRuntime, livePercent?: number): RuntimeAction {
  if (runtime.kind === "managed") {
    const percent = livePercent ?? runtime.updating;
    if (percent !== undefined) return { kind: "updating", percent };
    if (runtime.staged || runtime.queued) return { kind: "waiting" };
    if (runtime.updateFailed) return { kind: "failed" };
    // Nobody uses it: removing it is the useful action, not updating it.
    if (runtime.removableBytes !== undefined) {
      return { kind: "remove", bytes: runtime.removableBytes };
    }
    if (runtime.update) return { kind: "update", version: runtime.update };
    return { kind: "none" };
  }
  const instanceIds = switchableUsers(runtime);
  return instanceIds.length > 0 ? { kind: "useAuto", instanceIds } : { kind: "none" };
}
