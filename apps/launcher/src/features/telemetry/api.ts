import { queryOptions } from "@tanstack/react-query";
import { commands, type TelemetryView } from "@/bindings";
import { isTauri } from "@/lib/platform";
import { mockTelemetry } from "./mock";

/**
 * Usage data in the launcher (TELEMETRY; `src-tauri/src/telemetry.rs`, `crates/pl-telemetry`). The
 * backend keeps the player's choice in the data folder, so nothing goes out before the page has
 * run, and nothing at all while Studio's `features.telemetry` is off.
 */
export interface TelemetryApi {
  /** Whether Settings → Veri shows the switch, and where it stands. */
  state(): Promise<TelemetryView>;
  /** The player's switch; off forgets what was not sent yet. */
  configure(enabled: boolean): Promise<TelemetryView>;
  /** The window showed after the first paint: the start's time (once per run). */
  pageReady(): Promise<void>;
  /** An error the page threw that nothing caught (only its scrubbed shape is kept). */
  pageError(message: string, source: string, line: number, column: number): Promise<void>;
}

const tauriTelemetry: TelemetryApi = {
  state: () => commands.telemetryState(),
  configure: (enabled) => commands.telemetryConfigure(enabled),
  pageReady: () => commands.telemetryPageReady(),
  pageError: (message, source, line, column) =>
    commands.telemetryPageError(message, source, line, column),
};

export const telemetryApi: TelemetryApi = isTauri() ? tauriTelemetry : mockTelemetry;

export const telemetryStateQuery = queryOptions({
  queryKey: ["telemetry-state"],
  queryFn: () => telemetryApi.state(),
  staleTime: 60_000,
});
