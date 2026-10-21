// Settings → Grafik ve FPS: the graphics preset of the instance Play launches, one tap away (HOME1),
// and its Iris shader, on or off (SMALL7). The same control and commands as the instance settings
// tab; here a pick applies at once and the toast offers "Geri al", which brings back what the file
// had, the player's own values included.
import { enter, leave, Select, Skeleton, Switch, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { showError } from "@/features/content/jobs";
import { sortInstances, useLibraryView } from "@/features/library/library-view";
import { GraphicsPicker } from "@/features/library/tabs/graphics-picker";
import { type GraphicsChoice, pickFor, savedChoice } from "@/features/library/tabs/settings-form";
import {
  api,
  type GraphicsPreset,
  type GraphicsState,
  type InstanceSummary,
  type ShaderState,
} from "@/lib/api";
import { InstanceIcon } from "@/shell/instance-icon";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { Row, SwapLine } from "./row";

/** One toast at a time: a new pick (or shader tap) replaces the last one's "Geri al". */
const TOAST_ID = "graphics-switch";

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
 * The shader row of one instance (keyed by it): a switch for Iris's shader pack, the thing that
 * costs the most FPS (SMALL7). It is there only while Iris has a pack to draw, on or off: an
 * instance without Iris or without a chosen pack has nothing to turn. A tap applies at once (only
 * `enableShaders` changes) and the toast's "Geri al" turns it back; while the game is prepared or
 * open it waits, as the preset does (Iris keeps its settings in memory).
 */
function ShaderRow({ instance, gameOpen }: { instance: InstanceSummary; gameOpen: boolean }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const queryKey = ["instance", instance.id, "shader"];
  const shader = useQuery({
    queryKey,
    queryFn: () => api.library.shader(instance.id),
    staleTime: 0,
  });
  // The game may change it (Iris's own menu): read again once it has closed.
  const wasOpen = useRef(gameOpen);
  useEffect(() => {
    if (wasOpen.current && !gameOpen) {
      void queryClient.invalidateQueries({ queryKey: ["instance", instance.id, "shader"] });
    }
    wasOpen.current = gameOpen;
  }, [gameOpen, instance.id, queryClient]);

  // The tap on its way: the switch is there at once, and springs back if the write fails.
  const [pending, setPending] = useState<boolean | null>(null);
  const writing = useRef(false);
  const turn = async (on: boolean, offerUndo: boolean) => {
    if (writing.current || gameOpen) return;
    writing.current = true;
    setPending(on);
    try {
      const state = await api.library.setShader(instance.id, on);
      queryClient.setQueryData<ShaderState | null>(queryKey, state);
      if (state && offerUndo) {
        toast.success(t(on ? "small7.shader.turnedOn" : "small7.shader.turnedOff"), {
          id: TOAST_ID,
          duration: 8000,
          action: { label: t("fps.undo"), onClick: () => void turn(!on, false) },
        });
      }
    } catch (error) {
      showError(error);
    } finally {
      writing.current = false;
      setPending(null);
    }
  };

  const data = shader.data;
  return (
    <AnimatePresence initial={false}>
      {data ? (
        // Comes and goes in place: a fade, never a height animation.
        <motion.div
          key="shader"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.2) }}
          exit={{ opacity: 0, transition: leave(0.14) }}
        >
          <Row
            title={t("small7.shader.title")}
            help={<SwapLine lines={[data.name, t("fps.waitsForGame")]} active={gameOpen ? 1 : 0} />}
          >
            <Switch
              checked={pending ?? data.on}
              onCheckedChange={(on) => void turn(on, true)}
              disabled={gameOpen}
              label={t("small7.shader.title")}
              hideLabel
            />
          </Row>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

/**
 * "Grafik ve FPS": which instance Play launches, its graphics preset in one tap, and its shader
 * when Iris has one. While that instance's game is prepared or open the preset and the shader wait
 * (the game writes its options back), as in the instance tab.
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
      <ShaderRow key={`shader:${instance.id}`} instance={instance} gameOpen={gameOpen} />
    </>
  );
}
