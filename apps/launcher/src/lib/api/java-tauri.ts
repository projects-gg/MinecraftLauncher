// Tauri implementation of `api.java`: maps the generated bindings onto the UI shapes.
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  type CmdError,
  commands,
  type JavaProgressEvent,
  type JavaPromptView,
  type JavaRuntimeView,
} from "@/bindings";
import { detectOs } from "@/lib/platform";
import type { JavaApi, JavaPrompt, JavaRuntime } from "./java-types";
import { ModCenterCommandError } from "./modcenter-tauri";

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new ModCenterCommandError(result.error);
  return result.data;
}

const runtime = (r: JavaRuntimeView): JavaRuntime => ({
  ...r,
  major: r.major ?? undefined,
  version: r.version ?? undefined,
  vendor: r.vendor ?? undefined,
  users: r.users.map((u) => ({ ...u, fit: u.fit ?? undefined })),
  component: r.component ?? undefined,
  platform: r.platform ?? undefined,
  update: r.update ?? undefined,
  updating: r.updating ?? undefined,
  problem: r.problem ?? undefined,
  removableBytes: r.removableBytes ?? undefined,
});

export const javaPrompt = (p: JavaPromptView): JavaPrompt => ({
  ...p,
  have: p.have ?? undefined,
});

/** Subscribes to a plain Tauri event; the returned function unsubscribes. */
function subscribe<T>(event: string, handler: (payload: T) => void): () => void {
  const unlisten = listen<T>(event, (e) => handler(e.payload));
  return () => {
    void unlisten.then((fn) => fn());
  };
}

export const tauriJava: JavaApi = {
  async check(instanceId, path) {
    const view = await ok(commands.javaCheck(instanceId, path));
    return {
      ...view,
      major: view.major ?? undefined,
      version: view.version ?? undefined,
      vendor: view.vendor ?? undefined,
      problem: view.problem ?? undefined,
      fit: view.fit ?? undefined,
      need: view.need ?? undefined,
    };
  },
  async pickExecutable() {
    const chosen = await open({
      multiple: false,
      directory: false,
      // Windows: java.exe or javaw.exe; elsewhere the program has no extension.
      filters: detectOs() === "windows" ? [{ name: "Java", extensions: ["exe"] }] : undefined,
    });
    return typeof chosen === "string" ? chosen : null;
  },
  async overview(rescan = false) {
    const view = await ok(commands.javaOverview(rescan));
    return {
      runtimes: view.runtimes.map(runtime),
      systemIncomplete: view.systemIncomplete,
      checkedAt: view.checkedAt ?? undefined,
      gameActive: view.gameActive,
    };
  },
  async update(component, platform) {
    await ok(commands.javaRuntimeUpdate(component, platform));
  },
  async remove(component, platform) {
    return ok(commands.javaRuntimeRemove(component, platform));
  },
  async switchToAuto(instanceIds) {
    return ok(commands.javaUseAuto(instanceIds));
  },
  async openFolder(id) {
    await ok(commands.javaOpenFolder(id));
  },
  onChange(listener) {
    return subscribe<null>("java-changed", () => listener());
  },
  onProgress(listener) {
    // f64 crosses as `number | null` (NaN would be null).
    return subscribe<JavaProgressEvent>("java-progress", (p) => listener(p.id, p.percent ?? 0));
  },
};
