import { listen } from "@tauri-apps/api/event";
import {
  commands,
  type OverlayConfig,
  type OverlayStateView,
  type OverlayStatus,
} from "@/bindings";
import { isTauri } from "@/lib/platform";
import { mockOverlay } from "./mock";

/** The in-game overlay in the launcher (`src-tauri/src/overlay.rs`, `crates/pl-overlay`). */
export interface OverlayApi {
  /** The page's choices: called at its start and whenever one of them changes. */
  configure(config: OverlayConfig): Promise<void>;
  /** What the launcher knows now (after the page loaded). */
  state(): Promise<OverlayStateView>;
  /** The mod in the running game connected or went away. Returns an unsubscribe function. */
  onStatus(listener: (status: OverlayStatus) => void): () => void;
  /** The switch changed without the page (the crash help turned the overlay's mod off). */
  onEnabled(listener: (enabled: boolean) => void): () => void;
}

function subscribe<T>(event: string, listener: (payload: T) => void): () => void {
  const unlisten = listen<T>(event, (e) => listener(e.payload));
  return () => {
    void unlisten.then((off) => off());
  };
}

const tauriOverlay: OverlayApi = {
  configure: (config) => commands.overlayConfigure(config),
  state: () => commands.overlayState(),
  onStatus: (listener) => subscribe<OverlayStatus>("overlay-status", listener),
  onEnabled: (listener) => subscribe<boolean>("overlay-enabled", listener),
};

export const overlayApi: OverlayApi = isTauri() ? tauriOverlay : mockOverlay;
