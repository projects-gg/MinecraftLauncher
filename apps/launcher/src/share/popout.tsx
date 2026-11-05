// The "Yayın" window's page (SHARE; src-tauri voice/share.rs): the stage alone, filling the window,
// for watching on a second screen or over the game ("Her zaman üstte"). It reads voice chat's state
// (`voice_state`, `voice-state`) and watches through its own channel; it may call only the stage's
// commands (pl_voice::guard), never the microphone or the camera. Minimized, it lets go of the
// picture after a few seconds and asks again when it is back; closed, watching moves back to the
// launcher's window.
import { setAppReducedMotion, Toaster, TooltipProvider } from "@pl/ui";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { markPopout, StageBody, stageWatches } from "@/features/share/stage";
import { pageEngine, setPageEngine, useShare } from "@/features/share/store";
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
    return stop;
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
    return () => {
      stopAway();
      engine.dispose();
      setPageEngine(null);
    };
  }, []);
}

function Popout() {
  const { t } = useTranslation();
  useVoiceState();
  useWatching();
  const share = useShare();
  const me = useVoiceUi((s) => s.state.room?.me.id ?? null);
  const watches = share.surface === "popout" ? stageWatches(share, me) : [];
  const keys = share.watching.map((w) => shareKey(w.person, w.n)).join(" ");
  useEffect(() => {
    pageEngine()?.retain(keys ? keys.split(" ") : []);
  }, [keys]);
  useEffect(() => {
    document.title = t("share.stage.windowTitle");
  }, [t]);
  return (
    <main className="fixed inset-0 bg-obsidian-950 text-bone-100">
      {watches.length ? (
        <StageBody watches={watches} popout />
      ) : (
        <p className="flex size-full items-center justify-center text-[13px] text-ash-300">
          {t("share.stage.empty")}
        </p>
      )}
    </main>
  );
}

async function start() {
  const root = document.getElementById("root");
  if (!root) return;
  const prefs = usePrefs.getState();
  setAppReducedMotion(prefs.reduceMotion || prefs.performanceMode);
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
