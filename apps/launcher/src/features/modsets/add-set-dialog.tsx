import { ArrowLeft, Package, Plus } from "@phosphor-icons/react";
import { Button, Dialog, EmptyState, IconButton, Skeleton, SwapLabel, toast } from "@pl/ui";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import offlineArt from "@/assets/art/empty-offline.webp";
import { newJobId, type RunningJob, showError } from "@/features/content/jobs";
import { JobStatus } from "@/features/content/parts";
import { useDebounced } from "@/features/content/use-content";
import { type Direction, stepVariants, useMotionPrefs } from "@/features/modcenter/motion";
import { api, type InstanceDetail, type SetCard } from "@/lib/api";
import { formatBytes, listFormat } from "@/lib/format";
import { refreshSets } from "./instance-sets";
import { changedChoices, describeSummary, fitFor, setName } from "./model";
import { PurposeMark, SetCardSkeletons, SetContents, SetPicker, useWords } from "./parts";
import { useSetCatalog } from "./set-source";

/**
 * "Set ekle" in the mod center: pick a set made for the instance's version and loader, look at
 * what it brings (items the instance already has are marked) and add it. Sets made for other
 * versions or loaders stay visible and say what they are for.
 */
export function AddSetDialog({
  instance,
  open,
  onClose,
  onAdded,
  addedIds,
}: {
  instance: InstanceDetail;
  open: boolean;
  onClose: () => void;
  onAdded?: () => void;
  /** Sets the instance already has. */
  addedIds: Set<string>;
}) {
  const { t } = useTranslation();
  const words = useWords();
  const prefs = useMotionPrefs();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const catalog = useSetCatalog(open);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [choices, setChoices] = useState<Record<string, boolean>>({});
  const [job, setJob] = useState<RunningJob | null>(null);

  useEffect(() => {
    if (!open) {
      setSelectedId(null);
      setConfirming(false);
      setChoices({});
    }
  }, [open]);

  const sets = catalog.data?.sets ?? [];
  const fitOf = (set: SetCard) => fitFor(set, instance.mcVersion, instance.loader);
  const selected = sets.find((set) => set.id === selectedId);
  const fit = selected ? fitOf(selected) : undefined;
  const variant = fit?.kind === "fits" ? fit.variant : undefined;
  const selection = useMemo(
    () => ({ items: variant ? changedChoices(variant.items, choices) : {} }),
    [variant, choices],
  );
  const settled = useDebounced(selection, 200);
  const preview = useQuery({
    queryKey: ["mod-set-preview", instance.id, selectedId, settled],
    queryFn: () => api.modSets.preview(instance.id, selectedId ?? "", settled),
    enabled: open && confirming && Boolean(variant),
    staleTime: 30_000,
    retry: false,
    placeholderData: keepPreviousData,
  });
  // Items the dry run leaves out are already in the instance (the pack, own mods, another set).
  const present = useMemo(() => {
    if (!variant || !preview.data) return new Set<string>();
    const planned = new Set(preview.data.items.map((item) => item.id));
    return new Set(
      variant.items.filter((item) => !item.library && !planned.has(item.id)).map((i) => i.id),
    );
  }, [variant, preview.data]);

  const add = async () => {
    if (!selected || !variant) return;
    const jobId = newJobId();
    setJob({ jobId });
    try {
      const result = await api.modSets.apply(
        instance.id,
        selected.id,
        selection,
        jobId,
        (progress) => setJob({ jobId, progress }),
      );
      toast.success(t("modSets.center.added", { name: setName(selected, words.english) }));
      if (result.issues > 0) toast(t("modSets.center.issues"));
      onAdded?.();
      onClose();
    } catch (error) {
      showError(error);
    } finally {
      setJob(null);
      refreshSets(queryClient, instance.id);
    }
  };
  const cancelJob = () => {
    if (job) void api.modSets.cancel(job.jobId);
  };

  let list = null;
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
    list = (
      <EmptyState
        icon={<Package size={26} weight="duotone" />}
        title={t("modSets.create.emptyTitle")}
      />
    );
  } else {
    // The sets made for this instance first; the others keep their order after them.
    const ordered = [...sets].sort(
      (a, b) => Number(fitOf(b).kind === "fits") - Number(fitOf(a).kind === "fits"),
    );
    list = (
      <SetPicker
        sets={ordered}
        value={selectedId}
        onChange={(id) => {
          setSelectedId(id);
          setChoices({});
        }}
        label={t("modSets.add.title")}
        variantOf={(set) => {
          const f = fitOf(set);
          return f.kind === "fits" ? f.variant : set.variants[0];
        }}
        fitOf={fitOf}
        addedIds={addedIds}
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
        <SetContents
          variant={variant}
          choices={choices}
          present={present}
          locked={Boolean(job)}
          onToggle={(id, on) => setChoices((current) => ({ ...current, [id]: on }))}
        />
      </div>
    ) : null;

  const step = confirming && confirm ? "confirm" : "list";
  const direction: Direction = step === "confirm" ? 1 : -1;
  const summary = preview.data?.summary;
  const summaryText = summary
    ? describeSummary(summary, t, (parts) => listFormat().format(parts), formatBytes)
    : null;

  // No set is made for this instance's version and loader: a new instance can have one.
  const noneFits =
    sets.length > 0 &&
    sets.every((set) => fitOf(set).kind !== "fits" || addedIds.has(set.id)) &&
    sets.some((set) => fitOf(set).kind !== "fits");
  const footer = job ? (
    <JobStatus job={job} onCancel={cancelJob} className="w-full" detailed />
  ) : noneFits && !selected ? (
    <>
      <p className="mr-auto min-w-0 text-[12.5px] text-ash-400">
        {t("modSets.add.noneFits", { version: instance.mcVersion })}
      </p>
      <Button variant="ghost" onClick={onClose}>
        {t("create.cancel")}
      </Button>
      <Button
        variant="secondary"
        onClick={() => {
          onClose();
          void navigate({ to: "/library", search: { create: true, source: "sets" } });
        }}
      >
        {t("modSets.add.newInstance")}
      </Button>
    </>
  ) : (
    <>
      <div className="mr-auto min-w-0 text-[12.5px] text-ash-400" aria-live="polite">
        {!selected ? (
          sets.length > 0 ? (
            t("modSets.create.pick")
          ) : null
        ) : confirming ? (
          summaryText === null ? (
            <Skeleton className="h-3.5 w-48" />
          ) : (
            <span
              className={
                preview.isFetching ? "opacity-55 transition-opacity" : "transition-opacity"
              }
            >
              {summaryText}
            </span>
          )
        ) : null}
      </div>
      <Button variant="ghost" onClick={onClose}>
        {t("create.cancel")}
      </Button>
      {confirming ? (
        <Button
          variant="primary"
          icon={<Plus size={16} weight="bold" />}
          disabled={!variant}
          onClick={() => void add()}
        >
          <SwapLabel
            labels={[t("modSets.add.submit"), t("modSets.add.adding")]}
            active={job ? 1 : 0}
          />
        </Button>
      ) : (
        <Button variant="primary" disabled={!variant} onClick={() => setConfirming(true)}>
          {t("modSets.create.next")}
        </Button>
      )}
    </>
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          cancelJob();
          onClose();
        }
      }}
      title={t("modSets.add.title")}
      closeLabel={t("common.close")}
      width={680}
      morph={prefs.layout}
      dismissOnOutsideClick={false}
      footer={footer}
      // The sets arrive just as the panel has risen in, and it then grew a second time to their
      // height (286 → 460 px at 1024 × 640): it opens at the list's height, so the skeleton turns
      // into the cards in place and the entrance is the only movement (POLISH6).
      className="min-h-[min(460px,80vh)]"
    >
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
    </Dialog>
  );
}
