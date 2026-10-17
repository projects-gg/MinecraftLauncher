import type { PrepareStatusView } from "@/bindings";
import type { PrepareApi } from "./api";

/**
 * Preview switches for background preparation: `?prepare=42` keeps the bar at "Hazırlanıyor %42",
 * `?prepare=ready` runs to the end and says "Oynamaya hazır". Also from localStorage
 * (`pl.mock.prepare`). Without one nothing is prepared.
 */
function flag(): string {
  try {
    return (
      new URLSearchParams(window.location.search).get("prepare") ??
      localStorage.getItem("pl.mock.prepare") ??
      ""
    );
  } catch {
    return "";
  }
}

const listeners = new Set<(status: PrepareStatusView) => void>();
let current: PrepareStatusView = { state: "idle" };
let started = false;

function emit(status: PrepareStatusView) {
  current = status.state === "ready" ? { state: "idle" } : status;
  for (const listener of listeners) listener(status);
}

function run(instanceId: string) {
  const mode = flag();
  if (!mode || started) return;
  started = true;
  if (mode === "ready") {
    let percent = 64;
    emit({ state: "preparing", instanceId, percent });
    const timer = setInterval(() => {
      percent += 4;
      if (percent < 100) {
        emit({ state: "preparing", instanceId, percent });
        return;
      }
      clearInterval(timer);
      emit({ state: "ready", instanceId });
    }, 250);
    return;
  }
  const percent = Number(mode);
  if (Number.isFinite(percent)) {
    emit({ state: "preparing", instanceId, percent: Math.min(100, Math.max(0, percent)) });
  }
}

export const mockPrepare: PrepareApi = {
  async configure(enabled, instanceId) {
    // A moment later, like the launcher (the page subscribes first).
    if (enabled && instanceId) setTimeout(() => run(instanceId), 300);
  },
  async status() {
    return current;
  },
  onStatus(listener) {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  },
};
