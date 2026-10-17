import { toast } from "@pl/ui";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import i18n from "./i18n";
import { isTauri } from "./platform";

/**
 * Put text on the clipboard. In the app this goes through the clipboard plugin, which does not
 * depend on the webview's user-activation rules; the browser preview uses the web API.
 */
export async function copyText(text: string): Promise<void> {
  if (isTauri()) await writeText(text);
  else await navigator.clipboard.writeText(text);
}

/** "Ayrıntıları kopyala": technical details for support, with a short confirmation. */
export async function copyDetails(text: string): Promise<void> {
  try {
    await copyText(text);
    toast.success(i18n.t("common.detailsCopied"));
  } catch {
    toast.error(i18n.t("common.copyFailed"));
  }
}
