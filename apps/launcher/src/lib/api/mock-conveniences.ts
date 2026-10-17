// Mock of the small conveniences (static previews and the browser): dropped files, "Devam et",
// desktop shortcuts and the screenshots a game session took.
// Preview switches (`localStorage["pl.mock.<name>"]` or `?<name>=` in the page URL):
// - `resume`: "world" (default), "server" or "none".
// - `shots`: screenshots the next mock game session takes (e.g. "3").
// - `runSecs`: how long a mock game session lasts (default 25).
// - `window.__plMockDrag({ type: "enter", paths: [...] })` (then "over", "drop" or "leave")
//   drags files over the window; names decide the kind (".jar" mod, ".mrpack" modpack, names with
//   "shader"/"BSL"/"Complementary" shader pack, other ".zip" resource pack).

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

export function createMockConveniences(loaderOf: (instanceId: string) => LoaderKind | undefined) {
  return {
    onFileDrag(listener: (event: FileDragEvent) => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
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
