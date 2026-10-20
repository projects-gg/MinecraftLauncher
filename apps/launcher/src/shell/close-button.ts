// The window's close button, Alt+F4 and the taskbar's "Close window" (TRAY1). The shell decides
// what closing does (src-tauri/src/shell.rs, pl_desktop::close): it quits unless a game, AFK
// accounts or the player's choice keep the launcher in the tray, and a job the player started
// finishes first. The page tells the shell that choice and hears when the window went away.
import { emit, listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { notify } from "@/lib/attention";
import { isTauri } from "@/lib/platform";
import { refreshWindowAway } from "@/lib/window-away";
import { type CloseBehavior, usePrefs } from "@/stores/prefs";

/** The shell's names for the two messages (pl_desktop::close::{CHOICE_EVENT, CLOSED_EVENT}). */
const CHOICE_EVENT = "close-choice";
const CLOSED_EVENT = "window-closed";

/** The shell put the window away; `hint` is the one-time notice when the launcher stays. */
export interface WindowClosed {
  hint: "tray" | "afk" | null;
}

/** The part of the preferences store the choice comes from (`usePrefs`). */
interface ChoiceSource {
  getState(): { closeBehavior: CloseBehavior };
  subscribe(
    listener: (
      now: { closeBehavior: CloseBehavior },
      before: { closeBehavior: CloseBehavior },
    ) => void,
  ): () => void;
}

/** Sends the player's choice now and after each change of it. Returns the stop function. */
export function syncCloseChoice(
  source: ChoiceSource,
  send: (choice: CloseBehavior) => void,
): () => void {
  send(source.getState().closeBehavior);
  return source.subscribe((now, before) => {
    if (now.closeBehavior !== before.closeBehavior) send(now.closeBehavior);
  });
}

/** The notice for a close: only while the launcher stays in the tray, and once ever. */
export function trayHintKey(
  closed: WindowClosed,
  hintShown: boolean,
): "tray.hint" | "tray.hintAfk" | null {
  if (hintShown || !closed.hint) return null;
  return closed.hint === "afk" ? "tray.hintAfk" : "tray.hint";
}

/** Desktop glue for the close button (shell/use-desktop-events.ts). */
export function useCloseButton() {
  const { t } = useTranslation();

  useEffect(() => {
    if (!isTauri()) return;
    return syncCloseChoice(usePrefs, (choice) => {
      emit(CHOICE_EVENT, { choice }).catch(() => undefined);
    });
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    const stop = listen<WindowClosed>(CLOSED_EVENT, ({ payload }) => {
      // Hiding sends the page no window event: ambient motion stops on this answer.
      void refreshWindowAway();
      const prefs = usePrefs.getState();
      const key = trayHintKey(payload, prefs.trayHintShown);
      if (!key) return;
      prefs.set("trayHintShown", true);
      void notify(t("tray.hintTitle"), t(key));
    });
    return () => {
      void stop.then((fn) => fn());
    };
  }, [t]);
}
