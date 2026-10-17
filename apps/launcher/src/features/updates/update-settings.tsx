import {
  ArrowsClockwise,
  CircleNotch,
  DownloadSimple,
  GameController,
} from "@phosphor-icons/react";
import { Button } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { useLaunch } from "@/stores/launch";
import { useUpdate } from "@/stores/update";
import { cancelInstall, checkNow, isGameBusy, requestInstall } from "./install";
import { InstallConfirm } from "./install-confirm";

/**
 * The action of Settings → Güncellemeler: check, download, or restart into the downloaded update
 * the same way the title bar pill does (the AFK question, never under a running game).
 */
export function UpdateSettingsAction() {
  const { t } = useTranslation();
  const phase = useUpdate((s) => s.phase);
  const percent = useUpdate((s) => Math.round(s.percent));
  const step = useUpdate((s) => s.step);
  const asking = useUpdate((s) => s.asking?.from === "settings");
  const gameBusy = useLaunch((s) => isGameBusy(s.phase.kind));
  const spinner = (
    <CircleNotch size={16} weight="bold" className="animate-spin motion-reduce:animate-none" />
  );

  if (phase === "ready" || phase === "applying") {
    const applying = phase === "applying";
    // A game is open: the update waits for it, quietly (like the title bar pill).
    const waiting = !applying && gameBusy;
    let label = t("settings.updates.restart");
    let icon = <ArrowsClockwise size={16} />;
    if (applying) {
      label =
        step === "stoppingAfk" ? t("afkAlerts.update.stopping") : t("afkAlerts.update.installing");
      icon = spinner;
    } else if (waiting) {
      label = t("updates.afterGame");
      icon = <GameController size={16} />;
    }
    return (
      <InstallConfirm from="settings">
        <Button
          variant={waiting ? "secondary" : "primary"}
          icon={icon}
          // Busy keeps full strength (it is working, not unavailable) but takes no second press.
          aria-disabled={applying || undefined}
          aria-expanded={asking}
          disabled={waiting}
          onClick={() => {
            if (applying) return;
            if (asking) cancelInstall();
            else void requestInstall("settings");
          }}
        >
          {label}
        </Button>
      </InstallConfirm>
    );
  }

  if (phase === "available") {
    return (
      <Button
        variant="primary"
        icon={<DownloadSimple size={16} />}
        onClick={() => void checkNow("settings")}
      >
        {t("updates.download")}
      </Button>
    );
  }

  const busy = phase === "checking" || phase === "downloading";
  return (
    <Button
      variant="secondary"
      icon={
        <ArrowsClockwise
          size={16}
          className={busy ? "animate-spin motion-reduce:animate-none" : undefined}
        />
      }
      disabled={busy}
      onClick={() => void checkNow("settings")}
    >
      {phase === "downloading"
        ? t("settings.updates.downloading", { percent })
        : busy
          ? t("settings.updates.checking")
          : t("settings.updates.check")}
    </Button>
  );
}
