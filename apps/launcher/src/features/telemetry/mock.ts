import type { TelemetryView } from "@/bindings";
import type { TelemetryApi } from "./api";

/**
 * Usage data in the browser preview: the switch shows (as with Studio's flag on); `?telemetry=off`
 * (the flag off) hides it; localStorage `pl.mock.telemetry` does the same.
 */
function shown(): boolean {
  try {
    const mode =
      new URLSearchParams(window.location.search).get("telemetry") ??
      localStorage.getItem("pl.mock.telemetry");
    return mode !== "off";
  } catch {
    return true;
  }
}

let enabled = true;

export const mockTelemetry: TelemetryApi = {
  async state(): Promise<TelemetryView> {
    return { shown: shown(), enabled };
  },
  async configure(on): Promise<TelemetryView> {
    enabled = on;
    return { shown: shown(), enabled };
  },
  async pageReady() {},
  async pageError() {},
};
