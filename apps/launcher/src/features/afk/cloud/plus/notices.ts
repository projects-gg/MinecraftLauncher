// Cloud AFK's notices in the launcher (afkplus.md §8.5, services/social README "AFK notices"). The
// friends service's socket brings them as data (`afkNotice`); the launcher words them in its own
// language, as the in-game card does (pl-overlay afk.rs): the bot account's name over one line.
// - The window in front: a toast now, and the notice counts as seen (`afk_seen`: the phone's
//   backup for an approval or the night's digest is called off).
// - The window away: a Windows notification (not for a `quiet` one, nor while the game's overlay
//   shows its own card), and the toast waits for the window to come back; only then is it seen.
// - A newer notice with the same tag takes the older one's place; `approval_closed` takes the
//   approval's away (toast and waiting copy).
// - Friends' "Rahatsız etmeyin" does not apply: it is a friends status, AFK has its own quiet
//   hours (the service marks those notices `quiet`).
import { toast } from "@pl/ui";
import { shownInGame } from "@/features/overlay/store";
import type { AfkNotice } from "@/lib/api/social-types";
import { showNotification, windowAway } from "@/lib/attention";
import i18n from "@/lib/i18n";
import { onWindowAwayChange } from "@/lib/window-away";
import { usePrefs } from "@/stores/prefs";
import { plusApi } from "./api";
import { type NoticeWords, noticeWords, type Target } from "./notice-words";
import { usePlusUi } from "./use-plus";
import type { Lang } from "./words";

/** The in-game card shows these itself (pl-overlay afk.rs); the rest stay in the launcher. */
const IN_GAME_EVENTS = new Set([
  "approval",
  "deal_done",
  "deal_failed",
  "found",
  "stopped",
  "expired",
  "renewal",
  "quota",
  "incident",
  "message",
  "paused",
]);

/** Opens what a notice is about: the AFK page, and the account's AFK+ panel on its section. */
function open(notice: AfkNotice, target: Target) {
  const account = notice.data.acct;
  if ("panel" in target && account) usePlusUi.getState().openPanel(account, target.panel);
  void import("@/router").then(({ router }) => router.navigate({ to: "/afk" }));
}

const toastId = (notice: AfkNotice) => `afk:${notice.tag ?? notice.id}`;

/** Notices that waited while the window was away (by toast id: the newest of a tag). */
const waiting = new Map<string, AfkNotice>();
let listening = false;

function seen(notice: AfkNotice) {
  void plusApi.noticeSeen(notice.id).catch(() => undefined);
}

function showToast(notice: AfkNotice, words: NoticeWords) {
  const id = toastId(notice);
  const options = {
    id,
    description: words.body,
    // An approval waits for an answer: it stays longer than a passing note.
    duration: notice.event === "approval" ? 20_000 : 8_000,
    // The toaster's quiet button (its `cancel` slot), not the gold action: right above OYNA a gold
    // "Aç" competed with it, and a notice is not the screen's one loud thing.
    cancel: { label: i18n.t("afkPlus.notices.open"), onClick: () => open(notice, words.target) },
  };
  if (notice.event === "stopped" || notice.event === "incident") toast.error(words.title, options);
  else toast(words.title, options);
  seen(notice);
}

/** The window came back: what waited shows now (newest last) and counts as seen. */
function flush() {
  if (windowAway() || !waiting.size) return;
  const lang: Lang = i18n.language === "en" ? "en" : "tr";
  const pending = [...waiting.values()].sort((a, b) => a.at - b.at);
  waiting.clear();
  for (const notice of pending.slice(-4)) {
    const words = noticeWords(notice, i18n.t, lang);
    if (words) showToast(notice, words);
  }
  // Older ones beyond the last four are in the panel's feed; they were seen with it.
  for (const notice of pending.slice(0, -4)) seen(notice);
}

function listen() {
  if (listening || typeof window === "undefined") return;
  listening = true;
  onWindowAwayChange((away) => {
    if (!away) flush();
  });
  window.addEventListener("focus", flush);
}

/** Browser previews (`&notice=<event>`): the toast as the window in front shows it. */
export function showAfkNoticeToast(notice: AfkNotice) {
  const lang: Lang = i18n.language === "en" ? "en" : "tr";
  const words = noticeWords(notice, i18n.t, lang);
  if (words) showToast(notice, words);
}

/** One notice from the friends service's socket. */
export function handleAfkNotice(notice: AfkNotice) {
  listen();
  const id = toastId(notice);
  if (notice.event === "approval_closed") {
    toast.dismiss(id);
    waiting.delete(id);
    return;
  }
  const lang: Lang = i18n.language === "en" ? "en" : "tr";
  const words = noticeWords(notice, i18n.t, lang);
  if (!words) return;
  if (!windowAway()) {
    showToast(notice, words);
    return;
  }
  // In game with the overlay on, its card says it (and tells the service it was seen).
  if (shownInGame("afkNotice") && IN_GAME_EVENTS.has(notice.event)) return;
  waiting.set(id, notice);
  if (notice.quiet || !usePrefs.getState().desktopNotifications) return;
  void showNotification(words.title, words.body, () => {
    // The player came for it: the panel opens, the waiting toast is not needed.
    waiting.delete(id);
    seen(notice);
    open(notice, words.target);
  });
}
