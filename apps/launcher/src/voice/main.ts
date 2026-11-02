// The hidden voice chat window's page (src-tauri voice/): never shown, no React, and nothing runs
// until the launcher asks. It hands the launcher a channel for its commands (`EngineCommand`) and
// reports back with `voice_page_report` (`PageReport`); these two are the only commands this
// window may call (pl_voice::guard). The engine behind them is engine.ts.
//
// Debug builds started with PROJECTS_VOICE_PROBE run VOICE-0's probe instead: only then may this
// window use events (the probe grants them at run time), so a refused listener means the product.
// Release builds never load the probe (the import below is gone from their bundle).
import { Channel, invoke } from "@tauri-apps/api/core";
import type { EngineCommand, EngineReport } from "./boundary";
import { createEngine } from "./engine";

async function probe(): Promise<boolean> {
  if (!import.meta.env.DEV && import.meta.env.TAURI_ENV_DEBUG !== "true") return false;
  const { startProbe } = await import("./probe");
  return startProbe();
}

function report(value: EngineReport): void {
  void invoke("voice_page_report", { report: value }).catch(() => {
    // The launcher closed the window meanwhile.
  });
}

async function main(): Promise<void> {
  if (await probe()) return;
  const engine = createEngine(report);
  const channel = new Channel<EngineCommand>();
  channel.onmessage = (command) => engine.handle(command);
  await invoke("voice_page_attach", { onCommand: channel });
  report({ t: "ready" });
}

void main();
