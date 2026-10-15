import { openUrl } from "@tauri-apps/plugin-opener";
import { commands } from "@/bindings";
import type { LauncherApi } from "./index";
import { mockApi } from "./mock";

// Commands are wired to the Rust backend as each crate lands; until then the UI keeps working
// against the mock implementation for that call.
export const tauriApi: LauncherApi = {
  ...mockApi,
  appInfo: () => commands.appInfo(),
  openUrl: (url) => openUrl(url),
};
