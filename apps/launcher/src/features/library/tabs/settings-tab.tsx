import { FloppyDisk } from "@phosphor-icons/react";
import {
  Button,
  cn,
  easeOutExpo,
  Field,
  Input,
  Segmented,
  Slider,
  Switch,
  toast,
  usePerf,
} from "@pl/ui";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { type ReactNode, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { showError } from "@/features/content/jobs";
import { JavaPicker, javaProblemText, useJavaCheck } from "@/features/java/java-picker";
import { api, type InstanceDetail, type InstanceSettings } from "@/lib/api";
import { settingsChanged, settleSettingsDraft, useDrafts } from "@/stores/drafts";
import { ChangeBar } from "../change-bar";

const glideTransition = { duration: 0.3, ease: easeOutExpo };

function Group({ title, glide, children }: { title: string; glide: boolean; children: ReactNode }) {
  return (
    // Groups glide when one above them changes height (Java "Özel", the minimum memory).
    <motion.section
      layout={glide ? "position" : false}
      transition={{ layout: glideTransition }}
      className="grid grid-cols-[200px_1fr] gap-6 border-b border-white/7 py-6 first:pt-1 last-of-type:border-b-0"
    >
      <h3 className="font-display text-[15px] font-semibold text-bone-50">{title}</h3>
      {/* Positioned: content that leaves pops out of the flow here (AnimatePresence popLayout). */}
      <div className="relative grid max-w-[560px] gap-4">{children}</div>
    </motion.section>
  );
}

const gb = (mb: number) =>
  `${(mb / 1024).toLocaleString("tr-TR", { maximumFractionDigits: 1 })} GB`;

/**
 * An instance's settings. Edits are a draft (stores/drafts.ts): it survives tab switches and
 * page visits, and the bar at the bottom saves or discards it; with nothing changed there is no
 * bar. A custom Java must be chosen and usable before the draft can be saved.
 */
export function SettingsTab({ instance }: { instance: InstanceDetail }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const draft = useDrafts((state) => state.settings[instance.id]);
  const setDraft = useDrafts((state) => state.setSettings);
  const saved = instance.settings;
  const s = draft ?? saved;
  const dirty = draft ? settingsChanged(draft, saved) : false;
  // A form that is back to what is saved is no draft (the bar and the tab's marker go).
  const update = (next: InstanceSettings) =>
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
  const reduce = useReducedMotion() ?? false;
  const { performanceMode } = usePerf();
  // Rows below glide when something above comes and goes (not in Performance mode).
  const glide = !reduce && !performanceMode;
  const appear = {
    initial: { opacity: 0 },
    animate: { opacity: 1, transition: { duration: 0.22, ease: easeOutExpo, delay: 0.06 } },
    exit: { opacity: 0, transition: { duration: 0.14, ease: easeOutExpo } },
  };

  const javaPath = s.javaPath ?? "";
  const customJava = s.javaMode === "custom";
  const javaCheck = useJavaCheck(instance.id, javaPath, customJava);
  // "Özel" without a Java, or with one that is gone or does not run, cannot be saved (the
  // picker says why). A Java the game finds too old or too new can: the launch asks then.
  const javaBlocked = customJava && (!javaPath.trim() || Boolean(javaCheck.data?.problem));
  // What the bar says about the Java: why "Kaydet" waits, or that the game wants another Java.
  // The picker's own line can sit under the bar, so the bar repeats it.
  const javaNote = customJava ? javaProblemText(t, javaPath, javaCheck.data) : null;

  const save = useMutation({
    mutationFn: (settings: InstanceSettings) => api.updateInstanceSettings(instance.id, settings),
    onSuccess: async (_result, sent) => {
      // The saved values first, then the draft goes: the form never flashes back. Typing goes
      // on while the instance is read again; what was typed meanwhile stays a draft.
      await queryClient.invalidateQueries({ queryKey: ["instance", instance.id] });
      settleSettingsDraft(instance.id, sent);
      toast.success(t("instance.settings.saved"));
    },
    onError: (error) => showError(error),
  });
  const canSave = dirty && !save.isPending && !javaBlocked;

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

  return (
    <div className="flex min-h-full max-w-[880px] flex-col">
      <Group title={t("instance.settings.java")} glide={glide}>
        <Segmented
          value={s.javaMode}
          onValueChange={(v) => set("javaMode", v)}
          label={t("instance.settings.java")}
          options={[
            { value: "auto", label: t("instance.settings.javaAuto") },
            { value: "custom", label: t("instance.settings.javaCustom") },
          ]}
        />
        <AnimatePresence mode="popLayout" initial={false}>
          {customJava ? (
            <motion.div key="custom" {...appear}>
              <JavaPicker
                instanceId={instance.id}
                value={javaPath}
                onChange={(path) => set("javaPath", path)}
              />
            </motion.div>
          ) : (
            <motion.p key="auto" className="text-[12.5px] text-ash-400" {...appear}>
              {t("instance.settings.javaAutoHelp")}
            </motion.p>
          )}
        </AnimatePresence>
      </Group>

      <Group title={t("instance.settings.memory")} glide={glide}>
        <Segmented
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
          <motion.div key="jvm" layout={glide ? "position" : false} transition={glideTransition}>
            <Field
              label={t("instance.settings.jvmArgs")}
              help={t("instance.settings.jvmHelp")}
              htmlFor="jvm-args"
            >
              <Input
                id="jvm-args"
                value={s.jvmArgs}
                onChange={(e) => set("jvmArgs", e.target.value)}
                spellCheck={false}
              />
            </Field>
          </motion.div>
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

      <Group title={t("instance.settings.hooks")} glide={glide}>
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

      <ChangeBar open={dirty || save.isPending} label={t("instance.settings.unsaved")}>
        {/* The Java's note when there is one (why "Kaydet" waits, or which Java the game wants),
            else "unsaved changes". The texts cross-fade in place. */}
        <div className="relative min-w-0 flex-1" aria-live="polite">
          <AnimatePresence initial={false} mode="popLayout">
            <motion.p
              key={javaNote ?? "unsaved"}
              {...appear}
              className={cn(
                "text-[13.5px]",
                javaBlocked ? "text-flare-300" : javaNote ? "text-gold-200" : "text-bone-100",
              )}
            >
              {javaNote ?? t("instance.settings.unsaved")}
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
