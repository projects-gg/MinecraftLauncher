// The "Yayın" window's page (SHARE, SQOL-U1; src-tauri voice/share.rs): the stage alone, filling the
// window, for watching on a second screen or over the game ("Her Zaman Üstte"). It reads voice
// chat's state (`voice_state`, `voice-state`) and who speaks (`voice-speaking`, for "Biri Konuşunca
// Yayın Sesini Kıs"), asks the launcher for the sound's settings (`share_popout_audio`, then
// `share-audio`), watches through its own channel and plays the streams' sound itself; it may call
// only the stage's commands (pl_voice::guard), never the microphone or the camera. Minimized, it
// lets go of the picture after a few seconds and asks again when it is back; closed, watching moves
// back to the launcher's window. WebView2's own right-click menu never opens here (Refresh would
// reload the page).
import { guardNativeContextMenu, setAppReducedMotion, Toaster, TooltipProvider } from "@pl/ui";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { StageBody, stageWatches, useEndedShares } from "@/features/share/stage";
import { markPopout, pageEngine, setPageEngine, useShare } from "@/features/share/store";
import { followPopoutEnv, startStreamPlayback } from "@/features/share/stream-env";
import { setStreamSayer, useStreamSayings } from "@/features/share/stream-say";
import { useVoiceUi } from "@/features/voice/store";
import { voiceApi } from "@/lib/api/voice-api";
import { startLanguage } from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { isWindowAway, onWindowAwayChange, trackWindowAway } from "@/lib/window-away";
import { usePrefs } from "@/stores/prefs";
import { shareApi } from "./api";
import { shareKey } from "./types";
import { createWatchEngine } from "./watch";
import "@/styles.css";

markPopout();

function useVoiceState() {
  useEffect(() => {
    let heard = false;
    const stop = voiceApi.onState((state) => {
      heard = true;
      useVoiceUi.getState().setState(state);
    });
    void voiceApi
      .state()
      .then((state) => {
        if (!heard) useVoiceUi.getState().setState(state);
      })
      .catch(() => {});
    // Who speaks: the launcher sends it here while this window is open (ducking).
    const stopSpeaking = voiceApi.onSpeaking((speaking) =>
      useVoiceUi.getState().setSpeaking(speaking),
    );
    const stopEnv = followPopoutEnv();
    return () => {
      stop();
      stopSpeaking();
      stopEnv();
    };
  }, []);
}

function useWatching() {
  useEffect(() => {
    if (!isTauri()) return;
    const engine = createWatchEngine((report) => void shareApi.report(report));
    setPageEngine(engine);
    void shareApi.attach((cmd) => engine.handle(cmd)).catch(() => {});
    const tell = (away: boolean) => {
      void shareApi.visible(!away);
      engine.setShowing(!away);
    };
    tell(isWindowAway());
    const stopAway = onWindowAwayChange(tell);
    const stopPlayback = startStreamPlayback("popout");
    return () => {
      stopPlayback();
      stopAway();
      engine.dispose();
      setPageEngine(null);
    };
  }, []);
}

/** This window's polite live region (the launcher's window has its own for everything). */
function PopoutWords() {
  const [line, setLine] = useState({ text: "", n: 0 });
  useEffect(() => setStreamSayer((text) => setLine((last) => ({ text, n: last.n + 1 }))), []);
  useStreamSayings("popout");
  return (
    <div role="status" aria-live="polite" className="sr-only">
      <span key={line.n}>{line.text}</span>
    </div>
  );
}

function Popout() {
  const { t } = useTranslation();
  useVoiceState();
  useWatching();
  const share = useShare();
  const me = useVoiceUi((s) => s.state.room?.me.id ?? null);
  const here = share.surface === "popout";
  const watches = here ? stageWatches(share, me) : [];
  const ended = useEndedShares(watches, share.live, share.on);
  const keys = share.watching.map((w) => shareKey(w.person, w.n)).join(" ");
  useEffect(() => {
    pageEngine()?.retain(keys ? keys.split(" ") : []);
  }, [keys]);
  useEffect(() => {
    document.title = t("share.stage.windowTitle");
  }, [t]);
  return (
    <main className="fixed inset-0 bg-obsidian-950 text-bone-100">
      {watches.length || ended.length ? (
        <StageBody watches={watches} ended={ended} popout />
      ) : (
        <p className="flex size-full items-center justify-center text-[13px] text-ash-300">
          {t("share.stage.empty")}
        </p>
      )}
      <PopoutWords />
    </main>
  );
}

async function start() {
  const root = document.getElementById("root");
  if (!root) return;
  const prefs = usePrefs.getState();
  setAppReducedMotion(prefs.reduceMotion || prefs.performanceMode);
  // WebView2's own menu (Back, Refresh, Print) only in text fields and over selected text.
  guardNativeContextMenu();
  if (isTauri()) trackWindowAway(getCurrentWindow());
  await startLanguage(prefs.language).catch(() => {});
  createRoot(root).render(
    <StrictMode>
      <TooltipProvider>
        <Popout />
        <Toaster />
      </TooltipProvider>
    </StrictMode>,
  );
}

void start();
