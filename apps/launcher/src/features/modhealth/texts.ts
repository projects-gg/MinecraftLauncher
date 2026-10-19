// Sentences of the mod health UI, built from kinds and display names (never ids, file names
// or version requirements). Kept free of React so they can be tested.
import type { TFunction } from "i18next";
import type {
  CrashKind,
  CrashSuspect,
  FixOffer,
  HealthIssue,
  RepairStep,
  StepStatus,
} from "@/lib/api";
import { listFormat } from "@/lib/format";
import { keyClashText } from "./key-texts";

/** "A", "A ve B", "A, B ve C" / "A, B and C" (the interface's language). */
export function joinNames(names: string[]): string {
  const unique = names.filter((name, i) => name && names.indexOf(name) === i);
  try {
    return listFormat().format(unique);
  } catch {
    return unique.join(", ");
  }
}

/**
 * The mods of one sentence: "A, B ve C"; a longer list names two and counts the rest ("A, B ve
 * 4 mod daha"), so a dependency many mods need stays one short line.
 */
export function modNames(t: TFunction, names: string[]): string {
  const unique = names.filter((name, i) => name && names.indexOf(name) === i);
  if (unique.length <= 3) return joinNames(unique);
  return t("modHealth.moreMods", {
    names: unique.slice(0, 2).join(", "),
    count: unique.length - 2,
  });
}

/** One sentence about one issue. */
export function issueText(t: TFunction, issue: HealthIssue, minecraft: string): string {
  const mods = modNames(t, issue.mods);
  const target = issue.target ?? "";
  const k = (key: string, extra: Record<string, unknown> = {}) =>
    t(`modHealth.issues.${key}`, { mods, target, minecraft, ...extra });
  switch (issue.kind) {
    case "duplicate":
      return issue.bundled && issue.target ? k("duplicateBundled") : k("duplicate");
    case "missingDependency":
      return issue.offer?.label === "enable" ? k("missingDisabled") : k("missingDependency");
    case "javaVersion":
      return issue.java ? k("javaVersion", { java: issue.java }) : k("javaNewer");
    case "wrongLoader":
      return issue.madeFor ? k("wrongLoader", { loader: issue.madeFor }) : k("wrongLoaderOther");
    case "keyClash":
      return keyClashText(t, issue.keys ?? []);
    default:
      return k(issue.kind);
  }
}

/** What an issue's button says. */
export function fixLabel(t: TFunction, offer: FixOffer): string {
  switch (offer.label) {
    case "disableNamed":
      return t("modHealth.fix.disableNamed", { name: offer.name ?? "" });
    case "disableOld":
      return t("modHealth.fix.disableOld", { count: offer.count });
    default:
      return t(`modHealth.fix.${offer.label}`);
  }
}

/** The line of a repair step: future tense while pending, then progressive, past, failed. */
export function stepText(t: TFunction, step: RepairStep): string {
  const name = step.name;
  if (step.status === "done" && step.note === "disabledInstead") {
    return t("modHealth.steps.disabledInstead", { name });
  }
  if (step.status === "done" && step.note === "fromPack") {
    return t("modHealth.steps.fromPack", { name });
  }
  const form: Exclude<StepStatus, "skipped"> = step.status === "skipped" ? "pending" : step.status;
  return t(`modHealth.steps.${step.kind}.${form}`, { name });
}

/** The small line under a step, when there is something to say. */
export function stepNote(t: TFunction, step: RepairStep): string | undefined {
  if (!step.note) return undefined;
  // A dependency switched on in the pack already says where it came from.
  if (step.note === "fromPack") return undefined;
  return t(`modHealth.notes.${step.note}`);
}

export interface StepCounts {
  done: number;
  failed: number;
  skipped: number;
  total: number;
}

export function countSteps(steps: RepairStep[]): StepCounts {
  return {
    done: steps.filter((s) => s.status === "done").length,
    failed: steps.filter((s) => s.status === "failed").length,
    skipped: steps.filter((s) => s.status === "skipped").length,
    total: steps.length,
  };
}

/** The step that is running now, else the next one to run. */
export function currentStep(steps: RepairStep[]): RepairStep | undefined {
  return steps.find((s) => s.status === "running") ?? steps.find((s) => s.status === "pending");
}

/** Why a crash suspect is suspected. */
export function suspectText(t: TFunction, suspect: CrashSuspect): string {
  const withRelated = [
    "requiresMissing",
    "missing",
    "requiresOtherVersion",
    "wrongVersion",
    "declaresIncompatible",
    "incompatible",
  ];
  if (withRelated.includes(suspect.reason) && !suspect.related) {
    return t(`modHealth.crash.reasons.${suspect.reason}Any`);
  }
  return t(`modHealth.crash.reasons.${suspect.reason}`, { related: suspect.related ?? "" });
}

/** The crash title: our own short text per kind, the analyzer's text otherwise. */
export function crashTitle(t: TFunction, kind: CrashKind | undefined, fallback: string): string {
  return kind ? t(`modHealth.crash.titles.${kind}`, { defaultValue: fallback }) : fallback;
}

export function crashHint(t: TFunction, kind: CrashKind | undefined, fallback: string): string {
  return kind ? t(`modHealth.crash.hints.${kind}`, { defaultValue: fallback }) : fallback;
}

/** `6144` → "6 GB", `4608` → "4,5 GB" / "4.5 GB", `768` → "768 MB". */
export { formatMemory } from "@/lib/format";

/** Text for "copy details": the issues with their files (support only). */
export function issuesDetails(issues: HealthIssue[]): string {
  return issues
    .map((i) => `${i.severity} ${i.kind}: ${i.mods.join(", ")} [${i.files.join(", ")}]`)
    .join("\n");
}
