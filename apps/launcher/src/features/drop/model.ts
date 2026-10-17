// What a window-wide file drop will do (shown while the files are still being dragged) and what
// it did (the notices after it). Pure functions; the kinds come from the backend, which reads
// every file's contents.
import type { DroppedFile, DropResult } from "@/lib/api";

export type CopyKind = "mod" | "resourcePack" | "shaderPack";
export type RowKind = CopyKind | "modpack" | "unknown";

const rowOrder: RowKind[] = ["mod", "resourcePack", "shaderPack", "modpack", "unknown"];

/** The instance copied files go into. */
export interface DropTarget {
  id: string;
  modded: boolean;
}

export interface DropRow {
  kind: RowKind;
  count: number;
  /** Why these files will not be taken. */
  blocked?: "needsLoader" | "noInstance";
}

export interface DropPlan {
  rows: DropRow[];
  /** The first modpack: it opens the import flow. */
  modpack?: DroppedFile;
  /** Something will happen: a copy or an import. */
  acceptable: boolean;
  /** Only modpacks: the card shows the new instance instead of a target. */
  importOnly: boolean;
}

const isCopy = (kind: RowKind): kind is CopyKind =>
  kind === "mod" || kind === "resourcePack" || kind === "shaderPack";

export function planDrop(files: DroppedFile[], target: DropTarget | null): DropPlan {
  const counts = new Map<RowKind, number>();
  for (const file of files) counts.set(file.kind, (counts.get(file.kind) ?? 0) + 1);
  const rows: DropRow[] = rowOrder
    .filter((kind) => (counts.get(kind) ?? 0) > 0)
    .map((kind) => {
      const row: DropRow = { kind, count: counts.get(kind) ?? 0 };
      if (isCopy(kind) && !target) row.blocked = "noInstance";
      else if (kind === "mod" && target && !target.modded) row.blocked = "needsLoader";
      return row;
    });
  const modpack = files.find((f) => f.kind === "modpack");
  const acceptable = rows.some((r) => (isCopy(r.kind) && !r.blocked) || r.kind === "modpack");
  const importOnly = rows.length > 0 && rows.every((r) => r.kind === "modpack");
  return { rows, modpack, acceptable, importOnly };
}

/** A file name without its extension ("Sodium-0.6.jar" → "Sodium-0.6"). */
export function stem(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/** The instance page tab that shows what a drop added (the most common kind wins). */
export function tabFor(added: DroppedFile[]): "mods" | "resourcePacks" | "shaders" {
  const tally = { mod: 0, resourcePack: 0, shaderPack: 0 };
  for (const file of added) if (isCopy(file.kind)) tally[file.kind] += 1;
  const best = (["mod", "resourcePack", "shaderPack"] as const).reduce((a, b) =>
    tally[b] > tally[a] ? b : a,
  );
  return best === "mod" ? "mods" : best === "resourcePack" ? "resourcePacks" : "shaders";
}

export interface DropNotice {
  tone: "success" | "info" | "error";
  /** i18n key under `drop.notices`. */
  key: string;
  values?: Record<string, string | number>;
  /** "Göster" opens the instance page on this tab. */
  show?: "mods" | "resourcePacks" | "shaders";
}

/** One file by name (mods and packs without their extension, unknown files in full), several
 * by count. */
function counted(
  files: DroppedFile[],
  one: string,
  many: string,
  fullName = false,
): { key: string; values: Record<string, string | number> } {
  const first = files[0];
  return files.length === 1 && first
    ? { key: one, values: { name: fullName ? first.name : stem(first.name) } }
    : { key: many, values: { count: files.length } };
}

/** The notices after a drop, most important first. Modpacks open the import flow instead. */
export function dropNotices(result: DropResult): DropNotice[] {
  const notices: DropNotice[] = [];
  const { added } = result;
  if (added.length > 0) {
    const kinds = new Set(added.map((f) => f.kind));
    const only = kinds.size === 1 ? added[0]?.kind : undefined;
    const message =
      added.length === 1 || !only
        ? counted(added, "addedOne", "addedMany")
        : { key: `added.${only}`, values: { count: added.length } };
    notices.push({ tone: "success", ...message, show: tabFor(added) });
  }
  if (result.existing.length > 0) {
    notices.push({ tone: "info", ...counted(result.existing, "existingOne", "existingMany") });
  }
  if (result.needsLoader.length > 0) notices.push({ tone: "info", key: "needsLoader" });
  if (result.busy.length > 0) notices.push({ tone: "info", key: "busy" });
  if (result.unknown.length > 0) {
    notices.push({ tone: "info", ...counted(result.unknown, "unknownOne", "unknownMany", true) });
  }
  if (result.failed.length > 0) {
    notices.push({ tone: "error", ...counted(result.failed, "failedOne", "failedMany") });
  }
  return notices;
}
