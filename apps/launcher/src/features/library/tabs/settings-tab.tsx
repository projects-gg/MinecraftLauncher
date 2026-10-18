import {
  CaretDown,
  FloppyDisk,
  Lightning,
  Scales,
  SlidersHorizontal,
  Sparkle,
} from "@phosphor-icons/react";
import {
  Button,
  cn,
  easeOutExpo,
  enter,
  exitOf,
  Field,
  Input,
  leave,
  Segmented,
  Skeleton,
  Slider,
  Switch,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, type Ref, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { showError } from "@/features/content/jobs";
import { JavaPicker, javaProblemText, useJavaCheck } from "@/features/java/java-picker";
import { api, type InstanceDetail, type InstanceSettings } from "@/lib/api";
import { formatMemory } from "@/lib/format";
import {
  type SettingsDraft,
  settingsChanged,
  settleSettingsDraft,
  useDrafts,
} from "@/stores/drafts";
import { useLaunch } from "@/stores/launch";
import { ChangeBar, glideBeforeShrink, revealAboveBar } from "../change-bar";
import {
  type GraphicsChoice,
  graphicsChoices,
  hasAdvanced,
  pickFor,
  replacesCustom,
  savedChoice,
} from "./settings-form";

const glideTransition = { duration: 0.3, ease: easeOutExpo };
/** "İleri düzey" fields: rise in like a tab panel, fade out ~30% faster. */
const ADVANCED_IN = 0.22;
const groupClass =
  "grid grid-cols-[200px_1fr] gap-6 border-b border-white/7 py-6 first:pt-1 last-of-type:border-b-0";

function Group({
  title,
  glide,
  children,
  ref,
}: {
  title: string;
  glide: boolean;
  children: ReactNode;
  ref?: Ref<HTMLElement>;
}) {
  return (
    // Groups glide when one above them changes height (Java "Özel", the minimum memory).
    <motion.section
      ref={ref}
      layout={glide ? "position" : false}
      transition={{ layout: glideTransition }}
      className={groupClass}
    >
      <h3 className="font-display text-[15px] font-semibold text-bone-50">{title}</h3>
      {/* Positioned: content that leaves pops out of the flow here (AnimatePresence popLayout). */}
      <div className="relative grid max-w-[560px] gap-4">{children}</div>
    </motion.section>
  );
}

/** Memory sizes on the sliders ("4,5 GB" / "4.5 GB"; they start at 1 GB). */
const gb = (mb: number) => formatMemory(mb);

const graphicsIcons: Record<GraphicsChoice, ReactNode> = {
  smooth: <Lightning size={15} />,
  balanced: <Scales size={15} />,
  quality: <Sparkle size={15} />,
  custom: <SlidersHorizontal size={15} />,
};

/**
 * An instance's settings. Edits are a draft (stores/drafts.ts): it survives tab switches and
 * page visits, and the bar at the bottom saves or discards it; with nothing changed there is no
 * bar. A custom Java must be chosen and usable before the draft can be saved; a graphics preset
 * waits while the game is open (it keeps its options in memory and writes them back).
 */
export function SettingsTab({ instance }: { instance: InstanceDetail }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const draft = useDrafts((state) => state.settings[instance.id]);
  const setDraft = useDrafts((state) => state.setSettings);
  const saved = instance.settings;
  const s: SettingsDraft = draft ?? saved;
  const dirty = draft ? settingsChanged(draft, saved) : false;
  // A form that is back to what is saved is no draft (the bar and the tab's marker go).
  const update = (next: SettingsDraft) =>
    setDraft(instance.id, settingsChanged(next, saved) ? next : null);
  const set = <K extends keyof InstanceSettings>(key: K, value: InstanceSettings[K]) =>
    update({ ...s, [key]: value });

  const systemMb = instance.systemMemoryMb;
  const maxAllowed = Math.max(2048, systemMb - 1024);
  const warnAt = Math.round(systemMb * 0.75);
  const autoMb = Math.min(instance.autoMemoryMb, maxAllowed);
  // "Özel" starts from what "Otomatik" chose, so the thumb stays where it was.
  const chooseMemory = (mode: "auto" | "custom") =>
    update(
      mode === "auto"
        ? { ...s, memoryAuto: true }
        : {
            ...s,
            memoryAuto: false,
            maxMemoryMb: autoMb,
            minMemoryMb: Math.min(s.minMemoryMb, autoMb),
          },
    );
  const { reduce, layout: glide } = useMotionPrefs();
  // Rows below glide when something above comes and goes (not in Performance mode).
  const appear = {
    initial: { opacity: 0 },
    animate: { opacity: 1, transition: { duration: 0.22, ease: easeOutExpo, delay: 0.06 } },
    exit: { opacity: 0, transition: { duration: 0.14, ease: easeOutExpo } },
  };

  // Graphics: what options.txt has, and the player's pick. The game changes the file, so it is
  // read again at every visit (a small file) and once the game has closed.
  const graphics = useQuery({
    queryKey: ["instance", instance.id, "graphics"],
    queryFn: () => api.library.graphics(instance.id),
    staleTime: 0,
  });
  const savedGraphics = graphics.data ? savedChoice(graphics.data) : null;
  const pickedGraphics = s.graphics;
  const chooseGraphics = (value: GraphicsChoice) =>
    update({ ...s, graphics: pickFor(savedGraphics, value) });
  const gameOpen = useLaunch(
    (state) =>
      state.instanceId === instance.id &&
      (state.phase.kind === "working" ||
        state.phase.kind === "launching" ||
        state.phase.kind === "running"),
  );
  const graphicsWaits = gameOpen && pickedGraphics !== undefined;
  const wasOpen = useRef(gameOpen);
  useEffect(() => {
    if (wasOpen.current && !gameOpen) {
      void queryClient.invalidateQueries({ queryKey: ["instance", instance.id, "graphics"] });
    }
    wasOpen.current = gameOpen;
  }, [gameOpen, instance.id, queryClient]);

  const javaPath = s.javaPath ?? "";
  const customJava = s.javaMode === "custom";
  const javaCheck = useJavaCheck(instance.id, javaPath, customJava);
  // "Özel" without a Java, or with one that is gone or does not run, cannot be saved (the
  // picker says why). A Java the game finds too old or too new can: the launch asks then.
  const javaBlocked = customJava && (!javaPath.trim() || Boolean(javaCheck.data?.problem));
  // What the bar says: why "Kaydet" waits, or that the game wants another Java. The picker's own
  // line can sit under the bar, so the bar repeats it.
  const javaNote = customJava ? javaProblemText(t, javaPath, javaCheck.data) : null;
  const barNote = javaNote ?? (graphicsWaits ? t("graphics.waitsForGame") : null);

  const save = useMutation({
    mutationFn: async (sent: SettingsDraft) => {
      const { graphics: picked, ...settings } = sent;
      if (settingsChanged(settings, saved)) {
        await api.updateInstanceSettings(instance.id, settings);
      }
      if (picked) await api.library.setGraphics(instance.id, picked);
    },
    onSuccess: async (_result, sent) => {
      // The saved values first, then the draft goes: the form never flashes back. Typing goes
      // on while the instance is read again; what was typed meanwhile stays a draft.
      await queryClient.invalidateQueries({ queryKey: ["instance", instance.id] });
      settleSettingsDraft(instance.id, sent);
      toast.success(t("instance.settings.saved"));
    },
    onError: (error) => showError(error),
  });
  const canSave = dirty && !save.isPending && !javaBlocked && !graphicsWaits;

  // Ctrl+S saves, as in any editor (only while there is something to save).
  const saveRef = useRef(() => {});
  saveRef.current = () => {
    if (canSave) save.mutate(s);
  };
  useEffect(() => {
    if (!dirty) return;
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "s") {
        event.preventDefault();
        saveRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dirty]);

  // A size below the game's minimum (or a cleared field) snaps to the minimum when the field is
  // left, so a half-typed value is never saved.
  const clampSize = (key: "width" | "height", min: number) => {
    if (!(s[key] >= min)) set(key, min);
  };

  // "İleri düzey" starts closed, or open when something is set in it; then it is the player's.
  const [advanced, setAdvanced] = useState(() => ({ id: instance.id, open: hasAdvanced(s) }));
  if (advanced.id !== instance.id) setAdvanced({ id: instance.id, open: hasAdvanced(s) });
  const advancedOpen = advanced.id === instance.id ? advanced.open : hasAdvanced(s);
  const advancedId = useId();
  const advancedRef = useRef<HTMLElement>(null);
  // Opened by the player at the bottom of the tab: the fields scroll into view, clear of the bar.
  // Closed there: the tab glides up by what the fields leave behind while they fade out.
  const revealAdvanced = useRef(false);
  const toggleAdvanced = () => {
    revealAdvanced.current = !advancedOpen;
    const section = advancedRef.current;
    const title = section?.querySelector("h3");
    if (advancedOpen && section && title) {
      const style = getComputedStyle(section);
      const closedHeight =
        title.getBoundingClientRect().height +
        Number.parseFloat(style.paddingTop) +
        Number.parseFloat(style.paddingBottom);
      const lost = section.getBoundingClientRect().height - closedHeight;
      glideBeforeShrink(section, lost, reduce ? 0 : exitOf(ADVANCED_IN) * 1000);
    }
    setAdvanced({ id: instance.id, open: !advancedOpen });
  };
  useEffect(() => {
    if (!advancedOpen || !revealAdvanced.current) return;
    revealAdvanced.current = false;
    const frame = requestAnimationFrame(() => revealAboveBar(advancedRef.current, reduce));
    return () => cancelAnimationFrame(frame);
  }, [advancedOpen, reduce]);

  // The command palette's "Grafik ayarı" and "Bellek ayarı" land on their group: the control
  // takes the focus and scrolls into view, then the request is dropped from the address.
  const { focus } = useSearch({ from: "/library/$instanceId" });
  const graphicsRef = useRef<HTMLElement>(null);
  const memoryRef = useRef<HTMLElement>(null);
  const graphicsLoaded = !graphics.isPending;
  useEffect(() => {
    if (focus !== "graphics" && focus !== "memory") return;
    if (focus === "graphics" && !graphicsLoaded) return;
    // Two frames: the palette hands the focus back to the page as it closes.
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        const group = (focus === "graphics" ? graphicsRef : memoryRef).current;
        const control =
          group?.querySelector<HTMLElement>('button[data-state="on"]') ??
          group?.querySelector<HTMLElement>("button");
        control?.focus({ preventScroll: true });
        group?.scrollIntoView({ block: "nearest", behavior: reduce ? "auto" : "smooth" });
        void navigate({
          to: "/library/$instanceId",
          params: { instanceId: instance.id },
          search: (prev) => ({ ...prev, focus: undefined }),
          replace: true,
        });
      });
    });
    return () => cancelAnimationFrame(frame);
  }, [focus, graphicsLoaded, instance.id, navigate, reduce]);

  return (
    <div className="flex min-h-full max-w-[880px] flex-col">
      {/* Both states hold the ref from the start: motion keeps the ref it mounted with. */}
      {graphics.isPending ? (
        <Group title={t("graphics.title")} glide={glide} ref={graphicsRef}>
          {/* The three presets' width: the control replaces it without a jump. */}
          <Skeleton className="h-[34px] w-[238px]" />
        </Group>
      ) : savedGraphics === null ? null : (
        <Group title={t("graphics.title")} glide={glide} ref={graphicsRef}>
          <Segmented<GraphicsChoice>
            className="justify-self-start"
            value={pickedGraphics ?? savedGraphics}
            onValueChange={chooseGraphics}
            label={t("graphics.title")}
            options={graphicsChoices(savedGraphics).map((choice) => ({
              value: choice,
              label: choice === "custom" ? t("graphics.custom") : t(`graphics.preset.${choice}`),
              icon: graphicsIcons[choice],
            }))}
          />
          {/* Only a pick over the player's own values says something: saving replaces them. */}
          <AnimatePresence mode="popLayout" initial={false}>
            {replacesCustom(savedGraphics, pickedGraphics) ? (
              <motion.p key="replaces" className="text-[12.5px] text-ash-400" {...appear}>
                {t("graphics.replacesCustom")}
              </motion.p>
            ) : null}
          </AnimatePresence>
        </Group>
      )}

      <Group title={t("instance.settings.memory")} glide={glide} ref={memoryRef}>
        <Segmented
          className="justify-self-start"
          value={s.memoryAuto ? "auto" : "custom"}
          onValueChange={chooseMemory}
          label={t("instance.settings.memory")}
          options={[
            { value: "auto", label: t("instance.settings.memoryAuto") },
            { value: "custom", label: t("instance.settings.memoryCustom") },
          ]}
        />
        {/* "Otomatik" shows its choice on the same track; "Özel" unlocks it where it is. */}
        <Slider
          label={t("instance.settings.maxMemory")}
          value={s.memoryAuto ? autoMb : s.maxMemoryMb}
          onValueChange={(v) => set("maxMemoryMb", Math.max(v, s.minMemoryMb))}
          disabled={s.memoryAuto}
          min={1024}
          max={maxAllowed}
          step={512}
          format={gb}
          marks={[
            { value: 1024, label: "1 GB" },
            // The warning mark only matters for a value the player picks.
            ...(s.memoryAuto ? [] : [{ value: warnAt, label: gb(warnAt), tone: "warn" as const }]),
            { value: maxAllowed, label: gb(maxAllowed) },
          ]}
        />
        <AnimatePresence mode="popLayout" initial={false}>
          {s.memoryAuto ? null : (
            <motion.div key="min" {...appear}>
              <Slider
                label={t("instance.settings.minMemory")}
                value={s.minMemoryMb}
                onValueChange={(v) => set("minMemoryMb", Math.min(v, s.maxMemoryMb))}
                min={1024}
                max={Math.round(systemMb / 2)}
                step={512}
                format={gb}
              />
            </motion.div>
          )}
          {!s.memoryAuto && s.maxMemoryMb >= warnAt ? (
            <motion.p
              key="warn"
              role="alert"
              className="text-[12.5px] text-flare-400"
              layout={glide ? "position" : false}
              {...appear}
            >
              {t("instance.settings.memoryWarn")}
            </motion.p>
          ) : null}
        </AnimatePresence>
      </Group>

      <Group title={t("instance.settings.java")} glide={glide}>
        <Segmented
          className="justify-self-start"
          value={s.javaMode}
          onValueChange={(v) => set("javaMode", v)}
          label={t("instance.settings.java")}
          options={[
            { value: "auto", label: t("instance.settings.javaAuto") },
            { value: "custom", label: t("instance.settings.javaCustom") },
          ]}
        />
        {/* "Otomatik" says what it does; "Özel" brings the picker in. */}
        <AnimatePresence mode="popLayout" initial={false}>
          {customJava ? (
            <motion.div key="custom" {...appear}>
              <JavaPicker
                instanceId={instance.id}
                value={javaPath}
                onChange={(path) => set("javaPath", path)}
              />
            </motion.div>
          ) : null}
        </AnimatePresence>
      </Group>

      <Group title={t("instance.settings.window")} glide={glide}>
        <div className="grid grid-cols-2 gap-4">
          <Field label={t("instance.settings.width")} htmlFor="win-w">
            <Input
              id="win-w"
              type="number"
              min={640}
              value={s.width}
              onChange={(e) => set("width", Number(e.target.value))}
              onBlur={() => clampSize("width", 640)}
              disabled={s.fullscreen}
            />
          </Field>
          <Field label={t("instance.settings.height")} htmlFor="win-h">
            <Input
              id="win-h"
              type="number"
              min={480}
              value={s.height}
              onChange={(e) => set("height", Number(e.target.value))}
              onBlur={() => clampSize("height", 480)}
              disabled={s.fullscreen}
            />
          </Field>
        </div>
        <Switch
          checked={s.fullscreen}
          onCheckedChange={(v) => set("fullscreen", v)}
          label={t("instance.settings.fullscreen")}
        />
      </Group>

      <Group title={t("instance.settings.notes")} glide={glide}>
        <textarea
          aria-label={t("instance.settings.notes")}
          value={s.notes}
          onChange={(e) => set("notes", e.target.value)}
          rows={4}
          className="w-full resize-y rounded-md border border-white/10 bg-obsidian-800/80 px-3 py-2 text-[13.5px] text-bone-50 transition-[border-color,box-shadow] duration-150 hover:border-white/18 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-400/25 focus:outline-none"
        />
      </Group>

      {/* JVM arguments, commands and the wrapper: rarely needed, so out of the way. The title
          opens it like a library section; the fields rise in and fade out in place. */}
      <motion.section
        ref={advancedRef}
        layout={glide ? "position" : false}
        transition={{ layout: glideTransition }}
        className={groupClass}
      >
        <h3 className="self-start">
          <button
            type="button"
            aria-expanded={advancedOpen}
            aria-controls={advancedOpen ? advancedId : undefined}
            onClick={toggleAdvanced}
            className="-ml-1 flex cursor-pointer items-center gap-2 rounded-md px-1 py-0.5 font-display text-[15px] font-semibold text-ash-300 transition-[color,transform] duration-150 ease-out hover:text-bone-50 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none active:scale-[0.97] aria-expanded:text-bone-50"
          >
            <CaretDown
              size={14}
              weight="bold"
              className={cn(
                "transition-transform duration-200 ease-out motion-reduce:transition-none",
                !advancedOpen && "-rotate-90",
              )}
            />
            {t("graphics.advanced")}
          </button>
        </h3>
        <div className="relative grid max-w-[560px] gap-4">
          {/* Leaving fields stay in the flow while they fade: the tab glides up meanwhile. */}
          <AnimatePresence initial={false}>
            {advancedOpen ? (
              <motion.div
                key="fields"
                id={advancedId}
                className="grid gap-4"
                initial={{ opacity: 0, transform: "translateY(4px)" }}
                animate={{
                  opacity: 1,
                  transform: "translateY(0px)",
                  transition: enter(ADVANCED_IN),
                }}
                exit={{ opacity: 0, transition: leave(ADVANCED_IN) }}
              >
                <Field
                  label={t("instance.settings.jvmArgs")}
                  help={t("graphics.jvmHelp")}
                  htmlFor="jvm-args"
                >
                  <Input
                    id="jvm-args"
                    value={s.jvmArgs}
                    onChange={(e) => set("jvmArgs", e.target.value)}
                    spellCheck={false}
                  />
                </Field>
                <Field label={t("instance.settings.preLaunch")} htmlFor="pre">
                  <Input
                    id="pre"
                    value={s.preLaunch}
                    onChange={(e) => set("preLaunch", e.target.value)}
                    spellCheck={false}
                  />
                </Field>
                <Field label={t("instance.settings.postExit")} htmlFor="post">
                  <Input
                    id="post"
                    value={s.postExit}
                    onChange={(e) => set("postExit", e.target.value)}
                    spellCheck={false}
                  />
                </Field>
                <Field
                  label={t("instance.settings.wrapper")}
                  help={t("instance.settings.wrapperHelp")}
                  htmlFor="wrap"
                >
                  <Input
                    id="wrap"
                    value={s.wrapper}
                    onChange={(e) => set("wrapper", e.target.value)}
                    spellCheck={false}
                  />
                </Field>
              </motion.div>
            ) : null}
          </AnimatePresence>
        </div>
      </motion.section>

      <ChangeBar open={dirty || save.isPending} label={t("instance.settings.unsaved")}>
        {/* Why "Kaydet" waits when it does (the Java, the open game) or which Java the game
            wants, else "unsaved changes". The texts cross-fade in place. */}
        <div className="relative min-w-0 flex-1" aria-live="polite">
          <AnimatePresence initial={false} mode="popLayout">
            <motion.p
              key={barNote ?? "unsaved"}
              {...appear}
              className={cn(
                "text-[13.5px]",
                javaBlocked ? "text-flare-300" : barNote ? "text-gold-200" : "text-bone-100",
              )}
            >
              {barNote ?? t("instance.settings.unsaved")}
            </motion.p>
          </AnimatePresence>
        </div>
        <Button
          variant="ghost"
          disabled={save.isPending}
          onClick={() => setDraft(instance.id, null)}
        >
          {t("instance.settings.discard")}
        </Button>
        <Button
          variant="primary"
          icon={<FloppyDisk size={16} weight="fill" />}
          disabled={!canSave}
          onClick={() => save.mutate(s)}
        >
          {t("instance.settings.save")}
        </Button>
      </ChangeBar>
    </div>
  );
}
