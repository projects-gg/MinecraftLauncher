import { ArrowLeft, DownloadSimple } from "@phosphor-icons/react";
import { Button, EmptyState, Field, IconButton, Input, Segmented } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import offlineArt from "@/assets/art/empty-offline.webp";
import { newJobId, type RunningJob, showError } from "@/features/content/jobs";
import { JobStatus } from "@/features/content/parts";
import { defaultInstanceName } from "@/features/library/default-name";
import { type Direction, stepVariants, useMotionPrefs } from "@/features/modcenter/motion";
import { api, type InstanceSummary, type SetCard } from "@/lib/api";
import { notifyWhenAway } from "@/lib/attention";
import { formatBytes } from "@/lib/format";
import { ContentIcon } from "@/shell/content-icon";
import { trackTask } from "@/stores/progress";
import { bytesOf, changedChoices, defaultVariant, enabledItems, iconFor, setName } from "./model";
import { PurposeMark, SetCardSkeletons, SetContents, SetPicker, useWords } from "./parts";

/** The catalog query (shared by the new-instance dialog and the mod center). */
export function useSetCatalog(enabled: boolean) {
  return useQuery({
    queryKey: ["mod-sets"],
    queryFn: () => api.modSets.catalog(),
    enabled,
    staleTime: 5 * 60_000,
    retry: false,
  });
}

/**
 * "Hazır set" in the new-instance dialog: pick a set (on the recommended Minecraft version when
 * the set has it, else its newest, unless the player picks another), look at what it brings and
 * switch items off, then create an instance with the set installed. Returns the body and the
 * footer for the dialog.
 */
export function useSetSource({
  active,
  onCancel,
  onDone,
  instances,
}: {
  active: boolean;
  onCancel: () => void;
  onDone: (created: InstanceSummary) => void;
  /** Names already taken (a new name is numbered like the dialog's default name). */
  instances: string[];
}): { body: ReactNode; footer: ReactNode; cancelJob: () => void } {
  const { t } = useTranslation();
  const words = useWords();
  const prefs = useMotionPrefs();
  const catalog = useSetCatalog(active);
  // The recommended Minecraft version: the "Sıfırdan" list's query, usually loaded already.
  const versions = useQuery({
    queryKey: ["mc-versions"],
    queryFn: () => api.minecraftVersions(),
    enabled: active,
    staleTime: 5 * 60_000,
  });
  const recommended = versions.data?.find((v) => v.recommended)?.id;
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [version, setVersion] = useState<string>("");
  const [name, setName_] = useState("");
  const [choices, setChoices] = useState<Record<string, boolean>>({});
  const [job, setJob] = useState<RunningJob | null>(null);

  useEffect(() => {
    if (!active) {
      setSelectedId(null);
      setConfirming(false);
      setChoices({});
      setName_("");
    }
  }, [active]);

  const sets = catalog.data?.sets ?? [];
  const selected: SetCard | undefined = sets.find((set) => set.id === selectedId);
  const variant =
    selected?.variants.find((v) => v.minecraft === version) ??
    (selected ? defaultVariant(selected, recommended) : undefined);
  const enabled = variant ? enabledItems(variant.items, choices) : new Set<string>();
  const defaultName = selected
    ? defaultInstanceName(
        `${setName(selected, words.english)} ${variant?.minecraft ?? ""}`.trim(),
        instances,
      )
    : "";

  const pick = (id: string) => {
    setSelectedId(id);
    setChoices({});
    const set = sets.find((s) => s.id === id);
    setVersion(set ? (defaultVariant(set, recommended)?.minecraft ?? "") : "");
  };

  const start = async () => {
    if (!selected || !variant) return;
    const jobId = newJobId();
    setJob({ jobId });
    const task = trackTask(`modset:${jobId}`);
    try {
      const created = await api.modSets.createInstance(
        {
          setId: selected.id,
          minecraft: variant.minecraft,
          name: name.trim() || defaultName,
          icon: iconFor(selected.purpose),
          selection: { items: changedChoices(variant.items, choices) },
        },
        jobId,
        (progress) => {
          setJob({ jobId, progress });
          task.report(progress.percent);
        },
      );
      void notifyWhenAway(
        task.done(),
        t("attention.pack.title"),
        t("attention.pack.body", { name: created.name }),
      );
      setSelectedId(null);
      setConfirming(false);
      onDone(created);
    } catch (error) {
      showError(error);
    } finally {
      task.done();
      setJob(null);
    }
  };
  const cancelJob = () => {
    if (job) void api.modSets.cancel(job.jobId);
  };

  let list: ReactNode;
  if (catalog.isPending) {
    list = <SetCardSkeletons />;
  } else if (catalog.isError || catalog.data?.unavailable) {
    list = (
      <EmptyState
        illustration={offlineArt}
        title={t("modSets.create.offlineTitle")}
        body={t("modSets.create.offlineBody")}
        action={
          <Button variant="secondary" onClick={() => void catalog.refetch()}>
            {t("common.retry")}
          </Button>
        }
        className="py-4"
      />
    );
  } else if (sets.length === 0) {
    list = <EmptyState icon={<ContentIcon kind="pack" />} title={t("modSets.create.emptyTitle")} />;
  } else {
    list = (
      <SetPicker
        sets={sets}
        value={selectedId}
        onChange={pick}
        label={t("modSets.create.source")}
        variantOf={(set) => defaultVariant(set, recommended)}
      />
    );
  }

  const confirm =
    selected && variant ? (
      <div className="grid grid-cols-1 gap-4">
        <div className="flex items-center gap-3">
          <IconButton
            label={t("modSets.create.back")}
            icon={<ArrowLeft size={18} />}
            size="sm"
            disabled={Boolean(job)}
            onClick={() => setConfirming(false)}
          />
          <PurposeMark purpose={selected.purpose} active size={30} />
          <p className="min-w-0 flex-1 truncate font-display text-[17px] font-semibold text-bone-50">
            {setName(selected, words.english)}
          </p>
        </div>
        <div className="grid grid-cols-[1fr_auto] items-end gap-4">
          <Field label={t("create.name")} htmlFor="set-instance-name">
            <Input
              id="set-instance-name"
              value={name}
              placeholder={defaultName}
              className="placeholder:text-ash-400"
              maxLength={48}
              disabled={Boolean(job)}
              onChange={(e) => setName_(e.target.value)}
            />
          </Field>
          {selected.variants.length > 1 ? (
            <Field label={t("modSets.create.version")}>
              <Segmented
                value={variant.minecraft}
                onValueChange={(next) => {
                  setVersion(next);
                  setChoices({});
                }}
                label={t("modSets.create.version")}
                options={selected.variants.map((v) => ({ value: v.minecraft, label: v.minecraft }))}
              />
            </Field>
          ) : (
            <Field label={t("modSets.create.version")}>
              <p className="flex h-[34px] items-center text-[13.5px] text-bone-100 tabular-nums">
                {variant.minecraft}
              </p>
            </Field>
          )}
        </div>
        <section aria-label={t("modSets.create.contents")}>
          <SetContents
            key={variant.minecraft}
            variant={variant}
            choices={choices}
            locked={Boolean(job)}
            onToggle={(id, on) => setChoices((current) => ({ ...current, [id]: on }))}
          />
        </section>
      </div>
    ) : null;

  // Steps slide in from the side they come from while the dialog glides to its new size.
  const step = confirming && confirm ? "confirm" : "list";
  const direction: Direction = step === "confirm" ? 1 : -1;
  const body = (
    <AnimatePresence initial={false} mode="popLayout" custom={direction}>
      <motion.div
        key={step}
        custom={direction}
        variants={stepVariants(prefs.reduce)}
        initial="enter"
        animate="center"
        exit="exit"
      >
        {step === "confirm" ? confirm : list}
      </motion.div>
    </AnimatePresence>
  );

  const size = variant ? bytesOf(variant.items, enabled) : 0;
  const footer = job ? (
    <JobStatus job={job} onCancel={cancelJob} className="w-full" detailed />
  ) : (
    <>
      <p className="mr-auto text-[12.5px] text-ash-400 tabular-nums">
        {!selected
          ? sets.length > 0
            ? t("modSets.create.pick")
            : null
          : confirming
            ? t("modSets.create.download", { size: formatBytes(size) })
            : null}
      </p>
      <Button variant="ghost" onClick={onCancel}>
        {t("create.cancel")}
      </Button>
      {confirming ? (
        <Button
          variant="primary"
          icon={<DownloadSimple size={16} weight="bold" />}
          onClick={() => void start()}
        >
          {t("create.submit")}
        </Button>
      ) : (
        <Button variant="primary" disabled={!selected} onClick={() => setConfirming(true)}>
          {t("modSets.create.next")}
        </Button>
      )}
    </>
  );

  return { body, footer, cancelJob };
}
