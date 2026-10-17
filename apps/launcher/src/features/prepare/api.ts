import { listen } from "@tauri-apps/api/event";
import { commands, type PrepareStatusView } from "@/bindings";
import { isTauri } from "@/lib/platform";
import { mockPrepare } from "./mock";

/** Background preparation in the launcher (`src-tauri/src/prepare.rs`). */
export interface PrepareApi {
  /**
   * The setting and the instance the command bar shows, with its version ("mc/loader/version"):
   * called whenever one of them changes.
   */
  configure(enabled: boolean, instanceId: string | null, version: string | null): Promise<void>;
  /** What to show now (after the page loaded or came back on screen). */
  status(): Promise<PrepareStatusView>;
  /** Status changes. Returns an unsubscribe function. */
  onStatus(listener: (status: PrepareStatusView) => void): () => void;
}

const tauriPrepare: PrepareApi = {
  configure: (enabled, instanceId, version) =>
    commands.prepareConfigure(enabled, instanceId, version),
  status: () => commands.prepareStatus(),
  onStatus(listener) {
    const unlisten = listen<PrepareStatusView>("prepare-status", (event) =>
      listener(event.payload),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  },
};

export const prepareApi: PrepareApi = isTauri() ? tauriPrepare : mockPrepare;
