// What screen sharing keeps mounted with voice chat (SHARE): its toasts, the picker, the stage, and
// the launcher window's side of watching: while the room shares, this page attaches its channel
// for its watch connection and tells the launcher whether it is on screen (hidden in the tray or
// minimized, it lets go of the picture after a few seconds and asks again when it is back), plays
// the watched streams' sound (SQOL-U1: stream-env.ts) and says what changes in a polite live region.
// No room that shares: no engine, no channel, no sound, nothing runs.
import { useEffect } from "react";
import { announce } from "@/features/accounts/use-accounts";
import { isTauri } from "@/lib/platform";
import { isWindowAway, onWindowAwayChange } from "@/lib/window-away";
import { shareApi } from "@/share/api";
import { shareKey } from "@/share/types";
import { createWatchEngine } from "@/share/watch";
// SQOL: the notices' toasts and the player's own shares' questions (own-notices.ts).
import { useShareNotices } from "./own-notices";
import { SharePicker } from "./picker";
import { ShareStage } from "./stage";
import { pageEngine, setPageEngine, useShare } from "./store";
import { followMainEnv, startStreamPlayback } from "./stream-env";
import { setStreamSayer, useStreamSayings } from "./stream-say";
// The sharer's person rows in the stream menu (this window only).
import "./stream-person";

/** The launcher window's watch engine and the streams' sound, while the room shares. */
function useMainWatching() {
  const on = useShare().on;
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
    const stopPlayback = startStreamPlayback("main");
    return () => {
      stopPlayback();
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
}

/** What the streams depend on (the output, "Herkesin Sesi", the watching choices), and the words. */
function useStreamEnvAndWords() {
  useEffect(() => followMainEnv(), []);
  useEffect(() => setStreamSayer(announce), []);
  useStreamSayings("main");
}

export function ShareHost() {
  useShareNotices();
  useStreamEnvAndWords();
  useMainWatching();
  return (
    <>
      <SharePicker />
      <ShareStage />
    </>
  );
}
