// What screen sharing keeps mounted with voice chat (SHARE): its toasts, the picker, the stage, and
// the launcher window's side of watching: while the room shares, this page attaches its channel
// for its watch connection and tells the launcher whether it is on screen (hidden in the tray or
// minimized, it lets go of the picture after a few seconds and asks again when it is back). No
// room that shares: no engine, no channel, nothing runs.
import { toast } from "@pl/ui";
import { useEffect, useRef } from "react";
import { isTauri } from "@/lib/platform";
import { isWindowAway, onWindowAwayChange } from "@/lib/window-away";
import { shareApi } from "@/share/api";
import { shareKey } from "@/share/types";
import { createWatchEngine } from "@/share/watch";
import { noticeText } from "./actions";
import { SharePicker } from "./picker";
import { ShareStage } from "./stage";
import { pageEngine, setPageEngine, useShare, useShareUi } from "./store";

function useShareNotices() {
  useEffect(
    () =>
      shareApi.onNotice((notice) => {
        const text = noticeText(notice);
        if (text) toast(text);
      }),
    [],
  );
}

/** The launcher window's watch engine, while the room shares. */
function useMainWatching() {
  const on = useShare().on;
  const watching = useShare().watching.length > 0;
  const keys = useShare()
    .watching.map((w) => shareKey(w.person, w.n))
    .join(" ");
  useEffect(() => {
    if (!on || !isTauri()) return;
    const engine = createWatchEngine((report) => void shareApi.report(report));
    setPageEngine(engine);
    void shareApi.attach((cmd) => engine.handle(cmd)).catch(() => {});
    // Away: hidden in the tray, minimized, or covered whole (WebView2 says hidden then: the game
    // in front of a launcher it covers).
    const tell = () => {
      const away = isWindowAway() || document.visibilityState === "hidden";
      void shareApi.visible(!away);
      engine.setShowing(!away);
    };
    tell();
    const stopAway = onWindowAwayChange(tell);
    document.addEventListener("visibilitychange", tell);
    return () => {
      stopAway();
      document.removeEventListener("visibilitychange", tell);
      engine.dispose();
      setPageEngine(null);
    };
  }, [on]);
  // Streams of shares no longer watched go.
  useEffect(() => {
    pageEngine()?.retain(keys ? keys.split(" ") : []);
  }, [keys]);
  // Full screen and the corner end with the last watched share (not before the first came).
  const was = useRef(watching);
  useEffect(() => {
    const ended = was.current && !watching;
    was.current = watching;
    if (!ended) return;
    const ui = useShareUi.getState();
    if (ui.full) {
      ui.setFull(false);
      void shareApi.fullscreen(false).catch(() => {});
    }
    if (ui.small) ui.setSmall(false);
  }, [watching]);
}

export function ShareHost() {
  useShareNotices();
  useMainWatching();
  return (
    <>
      <SharePicker />
      <ShareStage />
    </>
  );
}
