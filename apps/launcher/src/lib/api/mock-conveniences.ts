// Mock of the small conveniences (static previews and the browser): dropped files, "Devam et",
// desktop shortcuts and the screenshots a game session took.
// Preview switches (`localStorage["pl.mock.<name>"]` or `?<name>=` in the page URL):
// - `resume`: "world" (default), "server" or "none".
// - `shots`: screenshots the next mock game session takes (e.g. "3").
// - `runSecs`: how long a mock game session lasts (default 25).
// - `window.__plMockDrag({ type: "enter", paths: [...], position: { x, y } })` (then "over",
//   "drop" or "leave", with the cursor in CSS pixels) drags files over the window; names decide
//   the kind (".jar" mod, ".mrpack" modpack, names with "shader"/"BSL"/"Complementary" shader
//   pack, other ".zip" resource pack). Real files dragged onto the browser preview work too.

import coverBuild from "@/assets/art/news-build-contest.webp";
import coverPvp from "@/assets/art/news-pvp-arena.webp";
import coverSpawn from "@/assets/art/news-spawn-city.webp";
import logo from "@/assets/brand/logo-96.webp";
import type {
  DroppedFile,
  DroppedKind,
  DropResult,
  FileDragEvent,
  LoaderKind,
  ResumeTarget,
} from "./types";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

function flag(key: string): string {
  try {
    const fromUrl = new URLSearchParams(window.location.search).get(key.replace(/^pl\.mock\./, ""));
    return fromUrl ?? localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

const listeners = new Set<(event: FileDragEvent) => void>();

declare global {
  interface Window {
    /** Preview helper: drag files over the window (see the header of mock-conveniences.ts). */
    __plMockDrag?: (event: FileDragEvent) => void;
  }
}

if (typeof window !== "undefined") {
  window.__plMockDrag = (event) => {
    for (const listener of listeners) listener(event);
  };
}

/** Real files dropped onto the browser preview, by the path the mock gave them. */
const droppedFiles = new Map<string, File>();

/** A real file dropped onto the browser preview (the skin preview reads it). */
export function mockDroppedFile(path: string): File | undefined {
  return droppedFiles.get(path);
}

/** Until the drop a page sees only the types of dragged files: enough to guess their kind. */
function guessedName(type: string, index: number): string {
  const extension =
    type === "image/png"
      ? ".png"
      : type.includes("java-archive")
        ? ".jar"
        : type.includes("zip")
          ? ".zip"
          : "";
  return `C:/Mock/dosya-${index + 1}${extension}`;
}

/** Files dragged onto the browser preview itself, as the app's drag events. */
function listenToPageDrags(emit: (event: FileDragEvent) => void): () => void {
  let depth = 0;
  const carriesFiles = (e: DragEvent) => Array.from(e.dataTransfer?.types ?? []).includes("Files");
  const at = (e: DragEvent) => ({ x: e.clientX, y: e.clientY });
  const enter = (e: DragEvent) => {
    if (!carriesFiles(e)) return;
    e.preventDefault();
    depth += 1;
    if (depth > 1) return;
    const items = Array.from(e.dataTransfer?.items ?? []).filter((i) => i.kind === "file");
    emit({ type: "enter", paths: items.map((i, n) => guessedName(i.type, n)), position: at(e) });
  };
  const over = (e: DragEvent) => {
    if (!carriesFiles(e)) return;
    e.preventDefault();
    emit({ type: "over", position: at(e) });
  };
  const leave = (e: DragEvent) => {
    if (!carriesFiles(e)) return;
    depth = Math.max(0, depth - 1);
    if (depth === 0) emit({ type: "leave" });
  };
  const drop = (e: DragEvent) => {
    if (!carriesFiles(e)) return;
    e.preventDefault();
    depth = 0;
    const paths = Array.from(e.dataTransfer?.files ?? []).map((file) => {
      const path = `C:/Mock/${file.name}`;
      droppedFiles.set(path, file);
      return path;
    });
    emit({ type: "drop", paths, position: at(e) });
  };
  window.addEventListener("dragenter", enter);
  window.addEventListener("dragover", over);
  window.addEventListener("dragleave", leave);
  window.addEventListener("drop", drop);
  return () => {
    window.removeEventListener("dragenter", enter);
    window.removeEventListener("dragover", over);
    window.removeEventListener("dragleave", leave);
    window.removeEventListener("drop", drop);
  };
}

function kindOf(path: string): DroppedKind {
  const name = path.split(/[\\/]/).pop()?.toLowerCase() ?? "";
  if (name.endsWith(".mrpack")) return "modpack";
  if (name.endsWith(".jar") || name.endsWith(".litemod")) return "mod";
  if (name.endsWith(".zip")) {
    return /shader|bsl|complementary|sildur/.test(name) ? "shaderPack" : "resourcePack";
  }
  return "unknown";
}

function file(path: string): DroppedFile {
  return { path, name: path.split(/[\\/]/).pop() ?? path, kind: kindOf(path) };
}

/** Thumbnails of the screenshots the next mock session takes (empty when none). */
export function mockNewScreenshots(): { count: number; recent: string[] } | null {
  const count = Number.parseInt(flag("pl.mock.shots"), 10);
  if (!Number.isFinite(count) || count <= 0) return null;
  return { count, recent: [coverSpawn, coverPvp, coverBuild].slice(0, Math.min(3, count)) };
}

/** How long a mock game session runs. */
export function mockSessionMs(): number {
  const secs = Number.parseFloat(flag("pl.mock.runSecs"));
  return (Number.isFinite(secs) && secs > 0 ? secs : 25) * 1000;
}

export function createMockConveniences(
  loaderOf: (instanceId: string) => LoaderKind | undefined,
  /** Mods a drop added (the instance page lists them). */
  onModsAdded?: (instanceId: string, files: DroppedFile[]) => void,
) {
  return {
    onFileDrag(listener: (event: FileDragEvent) => void) {
      listeners.add(listener);
      const stopPageDrags = listenToPageDrags(listener);
      return () => {
        listeners.delete(listener);
        stopPageDrags();
      };
    },
    async classifyFiles(paths: string[]): Promise<DroppedFile[]> {
      await delay(60);
      return paths.map(file);
    },
    async dropFiles(instanceId: string, paths: string[]): Promise<DropResult> {
      await delay(260);
      const files = paths.map(file);
      const modded = loaderOf(instanceId) !== "vanilla";
      const result: DropResult = {
        added: [],
        existing: [],
        unknown: [],
        needsLoader: [],
        busy: [],
        modpacks: [],
        failed: [],
      };
      for (const f of files) {
        if (f.kind === "unknown") result.unknown.push(f);
        else if (f.kind === "modpack") result.modpacks.push(f);
        else if (f.kind === "mod" && !modded) result.needsLoader.push(f);
        else if (/faithful/i.test(f.name)) result.existing.push(f);
        else result.added.push(f);
      }
      const mods = result.added.filter((f) => f.kind === "mod");
      if (mods.length > 0) onModsAdded?.(instanceId, mods);
      return result;
    },
    async resumeTarget(instanceId: string): Promise<ResumeTarget | null> {
      await delay(120);
      const mode = flag("pl.mock.resume") || "world";
      if (mode === "none" || loaderOf(instanceId) === undefined) return null;
      if (mode === "server") {
        return {
          kind: "server",
          name: "Hypixel",
          lastPlayedAt: new Date(Date.now() - 5 * 3_600_000).toISOString(),
          iconUrl: logo,
          quickPlay: { kind: "multiplayer", host: "mc.hypixel.net", port: null },
        };
      }
      return {
        kind: "world",
        name: "Ejderha Adası",
        lastPlayedAt: new Date(Date.now() - 20 * 3_600_000).toISOString(),
        iconUrl: coverSpawn,
        quickPlay: { kind: "singleplayer", world: "Ejderha Adasi" },
      };
    },
    async createShortcut(_instanceId: string, _iconPng?: string): Promise<void> {
      await delay(420);
    },
  };
}
