// "Ekran görüntülerini panoya kopyala" (Settings → Genel): the backend copies each new
// screenshot while the game runs (src-tauri/src/shot_copy.rs). The launcher speaks up only when
// one could not be copied, once per game: the player is usually in the game then, so a system
// notification while the window is away (if notifications are on), else a quiet toast. The
// session's "N yeni ekran görüntüsü" notice still offers "Kopyala" afterwards.
import { toast } from "@pl/ui";
import { notify, windowAway } from "@/lib/attention";
import i18n from "@/lib/i18n";
import { usePrefs } from "@/stores/prefs";

export function announceShotNotCopied() {
  const title = i18n.t("shotCopy.failedTitle");
  const body = i18n.t("shotCopy.failedBody");
  if (windowAway() && usePrefs.getState().desktopNotifications) {
    void notify(title, body);
    return;
  }
  toast.error(title, { id: "shot-not-copied", description: body, duration: 10_000 });
}
