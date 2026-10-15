import { FloppyDisk } from "@phosphor-icons/react";
import { Button, Field, Input, Segmented, Slider, Switch, toast } from "@pl/ui";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type InstanceDetail, type InstanceSettings } from "@/lib/api";

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="grid grid-cols-[200px_1fr] gap-6 border-b border-white/7 py-6 first:pt-1 last:border-b-0">
      <h3 className="font-display text-[15px] font-semibold text-bone-50">{title}</h3>
      <div className="grid max-w-[560px] gap-4">{children}</div>
    </section>
  );
}

const gb = (mb: number) => `${(mb / 1024).toLocaleString("tr-TR", { maximumFractionDigits: 1 })} GB`;

export function SettingsTab({ instance }: { instance: InstanceDetail }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [s, setS] = useState<InstanceSettings>(instance.settings);
  const set = <K extends keyof InstanceSettings>(key: K, value: InstanceSettings[K]) => setS((prev) => ({ ...prev, [key]: value }));

  const systemMb = instance.systemMemoryMb;
  const maxAllowed = Math.max(2048, systemMb - 1024);
  const warnAt = Math.round(systemMb * 0.75);

  const save = useMutation({
    mutationFn: () => api.updateInstanceSettings(instance.id, s),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["instance", instance.id] });
      toast.success(t("instance.settings.saved"));
    },
  });

  return (
    <div className="max-w-[880px]">
      <Group title={t("instance.settings.java")}>
        <Segmented
          value={s.javaMode}
          onValueChange={(v) => set("javaMode", v)}
          label={t("instance.settings.java")}
          options={[
            { value: "auto", label: t("instance.settings.javaAuto") },
            { value: "custom", label: t("instance.settings.javaCustom") },
          ]}
        />
        {s.javaMode === "auto" ? (
          <p className="text-[12.5px] text-ash-400">{t("instance.settings.javaAutoHelp")}</p>
        ) : (
          <Input value={s.javaPath ?? ""} onChange={(e) => set("javaPath", e.target.value)} aria-label={t("instance.settings.javaCustom")} />
        )}
      </Group>

      <Group title={t("instance.settings.memory")}>
        <Slider
          label={t("instance.settings.maxMemory")}
          value={s.maxMemoryMb}
          onValueChange={(v) => set("maxMemoryMb", Math.max(v, s.minMemoryMb))}
          min={1024}
          max={maxAllowed}
          step={512}
          format={gb}
          marks={[
            { value: 1024, label: "1 GB" },
            { value: warnAt, label: gb(warnAt), tone: "warn" },
            { value: maxAllowed, label: gb(maxAllowed) },
          ]}
        />
        <Slider
          label={t("instance.settings.minMemory")}
          value={s.minMemoryMb}
          onValueChange={(v) => set("minMemoryMb", Math.min(v, s.maxMemoryMb))}
          min={1024}
          max={Math.round(systemMb / 2)}
          step={512}
          format={gb}
        />
        {s.maxMemoryMb >= warnAt ? (
          <p role="alert" className="text-[12.5px] text-flare-400">
            {t("instance.settings.memoryWarn")}
          </p>
        ) : null}
        <Field label={t("instance.settings.jvmArgs")} help={t("instance.settings.jvmHelp")} htmlFor="jvm-args">
          <Input id="jvm-args" value={s.jvmArgs} onChange={(e) => set("jvmArgs", e.target.value)} spellCheck={false} />
        </Field>
      </Group>

      <Group title={t("instance.settings.window")}>
        <div className="grid grid-cols-2 gap-4">
          <Field label={t("instance.settings.width")} htmlFor="win-w">
            <Input id="win-w" type="number" min={640} value={s.width} onChange={(e) => set("width", Number(e.target.value))} disabled={s.fullscreen} />
          </Field>
          <Field label={t("instance.settings.height")} htmlFor="win-h">
            <Input id="win-h" type="number" min={480} value={s.height} onChange={(e) => set("height", Number(e.target.value))} disabled={s.fullscreen} />
          </Field>
        </div>
        <Switch checked={s.fullscreen} onCheckedChange={(v) => set("fullscreen", v)} label={t("instance.settings.fullscreen")} />
      </Group>

      <Group title={t("instance.settings.hooks")}>
        <Field label={t("instance.settings.preLaunch")} htmlFor="pre">
          <Input id="pre" value={s.preLaunch} onChange={(e) => set("preLaunch", e.target.value)} spellCheck={false} />
        </Field>
        <Field label={t("instance.settings.postExit")} htmlFor="post">
          <Input id="post" value={s.postExit} onChange={(e) => set("postExit", e.target.value)} spellCheck={false} />
        </Field>
        <Field label={t("instance.settings.wrapper")} help={t("instance.settings.wrapperHelp")} htmlFor="wrap">
          <Input id="wrap" value={s.wrapper} onChange={(e) => set("wrapper", e.target.value)} spellCheck={false} />
        </Field>
      </Group>

      <Group title={t("instance.settings.notes")}>
        <textarea
          aria-label={t("instance.settings.notes")}
          value={s.notes}
          onChange={(e) => set("notes", e.target.value)}
          rows={4}
          className="w-full resize-y rounded-md border border-white/10 bg-obsidian-800/80 px-3 py-2 text-[14px] text-bone-50 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-400/25 focus:outline-none"
        />
      </Group>

      <div className="sticky bottom-0 flex justify-end py-4">
        <Button variant="primary" icon={<FloppyDisk size={16} weight="fill" />} onClick={() => save.mutate()} disabled={save.isPending}>
          {t("instance.settings.save")}
        </Button>
      </div>
    </div>
  );
}
