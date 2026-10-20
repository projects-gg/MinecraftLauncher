// Settings → Grafik ve FPS: the graphics preset of the instance Play launches, one tap away (HOME1).
// The same control and commands as the instance settings tab; here a pick applies at once and the
// toast offers "Geri al", which brings back what the file had, the player's own values included.
import { cn, Select, Skeleton, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { showError } from "@/features/content/jobs";
import { sortInstances, useLibraryView } from "@/features/library/library-view";
import { GraphicsPicker } from "@/features/library/tabs/graphics-picker";
import { type GraphicsChoice, pickFor, savedChoice } from "@/features/library/tabs/settings-form";
import { api, type GraphicsPreset, type GraphicsState, type InstanceSummary } from "@/lib/api";
import { InstanceIcon } from "@/shell/instance-icon";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { Row } from "./row";

/** One toast at a time: a new pick replaces the last one's "Geri al". */
const TOAST_ID = "graphics-switch";

/**
 * Lines that take turns in one grid cell: the cell keeps the height of the tallest, so nothing
 * below moves; the old line fades out (100 ms), the new one in (150 ms after 60 ms), as
 * `SwapLabel` does for buttons. Screen readers get the shown line from a live text beside it.
 */
function SwapLine({ lines, active }: { lines: string[]; active: number }) {
  return (
    <>
      <span aria-hidden className="grid">
        {lines.map((line, i) => (
          <span
            key={line}
            className={cn(
              "[grid-area:1/1] transition-opacity",
              i === active ? "opacity-100 delay-[60ms] duration-150" : "opacity-0 duration-100",
            )}
          >
            {line}
          </span>
        ))}
      </span>
      {/* Heard when the choice, or the game opening, changes the line. */}
      <span className="sr-only" aria-live="polite">
        {lines[active]}
      </span>
    </>
  );
}

/** The instance whose graphics the section shows: Play's (the command bar's rule). */
function usePlayInstance(instances: InstanceSummary[]) {
  const selectedId = usePrefs((s) => s.selectedInstanceId);
  const phaseKind = useLaunch((s) => s.phase.kind);
  const launchId = useLaunch((s) => s.instanceId);
  // While a game is prepared or played the command bar stays on that instance.
  const busy = phaseKind === "working" || phaseKind === "launching" || phaseKind === "running";
  const instance =
    (busy ? instances.find((i) => i.id === launchId) : undefined) ??
    instances.find((i) => i.id === selectedId) ??
    instances[0];
  return { instance, busy, gameOpen: busy && instance !== undefined && launchId === instance.id };
}

/** The instance row: the one Play launches, chosen here as in the command bar's picker. */
function InstanceRow({
  instances,
  instance,
  locked,
}: {
  instances: InstanceSummary[];
  instance: InstanceSummary;
  /** A game is prepared or played: the command bar stays on it. */
  locked: boolean;
}) {
  const { t } = useTranslation();
  const selectInstance = usePrefs((s) => s.selectInstance);
  // The library's order, official packs first, like the command bar's picker.
  const sort = useLibraryView((s) => s.sort);
  const ordered = useMemo(() => {
    const sorted = sortInstances(instances, sort);
    return [...sorted.filter((i) => i.managed), ...sorted.filter((i) => !i.managed)];
  }, [instances, sort]);
  return (
    <Row title={t("fps.instance")}>
      <div className="flex items-center gap-2.5">
        <InstanceIcon icon={instance.icon} size={28} />
        {ordered.length > 1 ? (
          <div className="w-56">
            <Select<string>
              value={instance.id}
              onValueChange={selectInstance}
              label={t("fps.instance")}
              disabled={locked}
              options={ordered.map((i) => ({ value: i.id, label: i.name, hint: i.mcVersion }))}
            />
          </div>
        ) : (
          <span className="text-[13.5px] font-semibold text-bone-50">{instance.name}</span>
        )}
      </div>
    </Row>
  );
}

const EFFECT_ORDER: readonly GraphicsChoice[] = ["smooth", "balanced", "quality", "custom"];

/** The preset row of one instance (keyed by it: another instance starts fresh). */
function PresetRow({ instance, gameOpen }: { instance: InstanceSummary; gameOpen: boolean }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const helpId = useId();
  const queryKey = ["instance", instance.id, "graphics"];
  // The instance tab's query: the game changes the file, so it is read again at every visit.
  const graphics = useQuery({
    queryKey,
    queryFn: () => api.library.graphics(instance.id),
    staleTime: 0,
  });
  // And once the game has closed (it writes its options back).
  const wasOpen = useRef(gameOpen);
  useEffect(() => {
    if (wasOpen.current && !gameOpen) {
      void queryClient.invalidateQueries({ queryKey: ["instance", instance.id, "graphics"] });
    }
    wasOpen.current = gameOpen;
  }, [gameOpen, instance.id, queryClient]);

  // The pick on its way: the plate is there at once, and glides back if the write fails.
  const [pending, setPending] = useState<GraphicsPreset | null>(null);
  const writing = useRef(false);
  const saved = graphics.data ? savedChoice(graphics.data) : null;

  const settle = (state: GraphicsState) => queryClient.setQueryData(queryKey, state);
  const undo = async (before: GraphicsState, backup: string | undefined) => {
    try {
      if (backup) {
        const state = await api.library.undoGraphics(instance.id, backup);
        if (state) settle(state);
        else await queryClient.invalidateQueries({ queryKey });
      } else if (before.kind === "preset" || before.kind === "firstLaunch") {
        // No file yet: the first launch's pick goes back to the one before.
        settle(await api.library.setGraphics(instance.id, before.preset));
      }
    } catch (error) {
      showError(error);
    }
  };

  const choose = async (value: GraphicsChoice) => {
    const before = graphics.data;
    const preset = pickFor(saved, value);
    if (!before || !preset || writing.current || gameOpen) return;
    writing.current = true;
    setPending(preset);
    try {
      const result = await api.library.switchGraphics(instance.id, preset);
      settle(result.state);
      toast.success(t("fps.switched", { preset: t(`graphics.preset.${preset}`) }), {
        id: TOAST_ID,
        duration: 8000,
        action: {
          label: t("fps.undo"),
          onClick: () => void undo(before, result.backup),
        },
      });
    } catch (error) {
      showError(error);
    } finally {
      writing.current = false;
      setPending(null);
    }
  };

  if (graphics.isPending) {
    // The three presets' width and the help line's height: nothing moves when they come.
    return (
      <Row title={t("fps.preset")} help={<Skeleton className="my-[3px] h-3 w-28" />}>
        <Skeleton className="h-[34px] w-[238px]" />
      </Row>
    );
  }
  // A file that cannot be read: no control, as in the instance tab.
  if (!graphics.data) return null;
  if (saved === null) {
    return (
      <Row title={t("fps.preset")} help={t("fps.unsupported")}>
        {null}
      </Row>
    );
  }
  const shown: GraphicsChoice = pending ?? saved;
  const lines = [
    t("fps.effect.smooth"),
    t("fps.effect.balanced"),
    t("fps.effect.quality"),
    t("fps.effect.custom"),
    t("fps.waitsForGame"),
  ];
  const active = gameOpen ? lines.length - 1 : EFFECT_ORDER.indexOf(shown);
  return (
    <Row title={t("fps.preset")} helpId={helpId} help={<SwapLine lines={lines} active={active} />}>
      <GraphicsPicker
        saved={saved}
        value={shown}
        onValueChange={(value) => void choose(value)}
        disabled={gameOpen}
        describedBy={helpId}
      />
    </Row>
  );
}

/**
 * "Grafik ve FPS": which instance Play launches, and its graphics preset in one tap. While that
 * instance's game is prepared or open the preset waits (the game writes its options back), as
 * in the instance tab.
 */
export function GraphicsSection() {
  const { t } = useTranslation();
  const { data: instances = [], isPending } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const { instance, busy, gameOpen } = usePlayInstance(instances);
  if (!instance) {
    return isPending ? null : (
      <p className="py-3.5 text-[13.5px] text-ash-300">{t("fps.noInstance")}</p>
    );
  }
  return (
    <>
      <InstanceRow instances={instances} instance={instance} locked={busy} />
      <PresetRow key={instance.id} instance={instance} gameOpen={gameOpen} />
    </>
  );
}
