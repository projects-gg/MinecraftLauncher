import { cn, SwapLabel, Switch, toast } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { officialStatusQuery } from "@/features/library/official";
import { useInstallOfficial } from "@/features/library/official-ui";
import { Row } from "@/features/settings/row";
import { api } from "@/lib/api";
import { usePrefs } from "@/stores/prefs";
import { officialMissing, overlayLine } from "./line";

/** The line that carries "Kur" (`overlayLine`). */
const PACK_MISSING = 2;

/**
 * Settings → Genel "Oyun içi katman" (OVL2, on by default): the official pack's overlay mod and a
 * bridge to it at each game start. Its one place: the mod center does not list the mod. The line
 * under it says how it opens; while the switch is on but the overlay cannot work it says why
 * instead, in the same cell, so nothing below moves: no official pack (LIB1: the player deleted
 * it), with "Kur" as the library offers it, or "Oyun açılınca launcher: Kapansın". The shortcut
 * itself is set in the game's Controls.
 */
export function OverlayRow() {
  const { t } = useTranslation();
  const on = usePrefs((s) => s.inGameOverlay);
  const leaves = usePrefs((s) => s.onGameStart === "close");
  const set = usePrefs((s) => s.set);
  const { data: instances } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const { data: official } = useQuery(officialStatusQuery);
  const install = useInstallOfficial();
  const installing = install.isPending;
  const active = overlayLine(on, leaves, officialMissing(instances, official));
  const lines = [t("overlay.settingHelp"), t("overlay.closeHelp"), t("overlay.packMissing")];
  return (
    <Row
      title={t("overlay.setting")}
      help={
        <>
          {/* Heard when the switch, the close choice or the pack changes the line; first, so the
              switch's description reads the line before "Kur". */}
          <span className="sr-only" aria-live="polite">
            {lines[active]}
          </span>
          {/* As `SwapLine` does, with an action in a line: hidden lines are inert. */}
          <span className="grid">
            {lines.map((line, i) => (
              <span
                key={line}
                inert={i !== active}
                className={cn(
                  "[grid-area:1/1] transition-opacity",
                  i === active ? "opacity-100 delay-[60ms] duration-150" : "opacity-0 duration-100",
                )}
              >
                <span aria-hidden>{line}</span>
                {i === PACK_MISSING ? (
                  <button
                    type="button"
                    aria-label={t("lib1.install")}
                    aria-busy={installing || undefined}
                    // The words start right after the sentence (`SwapLabel` centres them).
                    className="ml-1.5 cursor-pointer rounded-sm font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300 [&_.swap-label]:justify-items-start"
                    onClick={(event) => {
                      // Inside the row's label: the switch must not flip with it.
                      event.preventDefault();
                      if (installing) return;
                      install.mutate(undefined, {
                        onSuccess: (made) =>
                          toast.success(t("official.added", { name: made.name })),
                      });
                    }}
                  >
                    <SwapLabel
                      labels={[t("lib1.installShort"), t("lib1.installing")]}
                      active={installing ? 1 : 0}
                    />
                  </button>
                ) : null}
              </span>
            ))}
          </span>
        </>
      }
    >
      <Switch
        checked={on}
        onCheckedChange={(v) => set("inGameOverlay", v)}
        label={t("overlay.setting")}
        hideLabel
      />
    </Row>
  );
}
