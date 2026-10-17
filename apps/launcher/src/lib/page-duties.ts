// Work the page still does while the window is out of sight, told to the backend so it does not
// suspend the WebView under it (src-tauri/src/webview_rest.rs): a reminder waiting to ring. Only
// changes are sent, as the plain `page-duties` event.
import { emit } from "@tauri-apps/api/event";
import { isTauri } from "./platform";

const duties = new Map<string, boolean>();
let sent: boolean | undefined;

/** Marks one duty (by name) as waiting or done. */
export function setPageDuty(name: string, waiting: boolean) {
  duties.set(name, waiting);
  const busy = [...duties.values()].some(Boolean);
  if (busy === sent || !isTauri()) return;
  sent = busy;
  emit("page-duties", { busy }).catch(() => {
    sent = undefined;
  });
}
