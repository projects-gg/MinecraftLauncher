import { ArrowsClockwise, DotsThree, Plus, Trash } from "@phosphor-icons/react";
import {
  Button,
  Chip,
  ContentIcon,
  cn,
  EmptyState,
  IconButton,
  Menu,
  Skeleton,
  toast,
} from "@pl/ui";
import { useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { newJobId, showError } from "@/features/content/jobs";
import { useDebounced } from "@/features/content/use-content";
import { ChangeBar } from "@/features/library/change-bar";
import { fade, glide, swap, useMotionPrefs } from "@/features/modcenter/motion";
import { JobLine } from "@/features/modcenter/parts";
import { ItemRow, refreshMods } from "@/features/modcenter/recommended-tab";
import { choicesOf, enabledSet, pendingSelection } from "@/features/modcenter/selection";
import {
  api,
  type InstanceDetail,
  type InstanceSet,
  type JobProgress,
  type ModCenterItem,
  type ModSelection,
  type SyncSummary,
} from "@/lib/api";
import { formatBytes, listFormat } from "@/lib/format";
import { describeSummary, setName, setSummary } from "./model";
import { PurposeMark, useWords } from "./parts";

/** Choices not applied yet, per instance and set; they survive section and tab switches. */
const useSetDrafts = create<{
  choices: Record<string, Record<string, boolean>>;
  set: (key: string, choices: Record<string, boolean> | null) => void;
}>((set) => ({
  choices: {},
  set: (key, next) =>
    set((state) => {
      const choices = { ...state.choices };
      if (next && Object.keys(next).length > 0) choices[key] = next;
      else delete choices[key];
      return { choices };
    }),
}));

/** A set change in progress, per instance (kept outside the section: it may remount). */
const useSetJob = create<{
  jobs: Record<string, { setId: string; progress: JobProgress }>;
  set: (instanceId: string, job: { setId: string; progress: JobProgress } | null) => void;
}>((set) => ({
  jobs: {},
  set: (instanceId, job) =>
    set((state) => {
      const jobs = { ...state.jobs };
      if (job) jobs[instanceId] = job;
      else delete jobs[instanceId];
      return { jobs };
    }),
}));

const draftKey = (instanceId: string, setId: string) => `${instanceId}/${setId}`;

/** What a run does, for its toast: an update, applied choices, or the set added back. */
type RunKind = "update" | "apply" | "add";
const noChoices: Record<string, boolean> = {};
const SEEN_AFTER_MS = 1500;

/** Refreshes everything that shows the instance's sets and files. */
export function refreshSets(queryClient: ReturnType<typeof useQueryClient>, instanceId: string) {
  void queryClient.invalidateQueries({ queryKey: ["mod-sets-instance", instanceId] });
  refreshMods(queryClient, instanceId);
  void queryClient.invalidateQueries({ queryKey: ["content-scan", instanceId] });
}

/** The query of an instance's sets. */
export function useInstanceSets(instanceId: string, enabled = true) {
  return useQuery({
    queryKey: ["mod-sets-instance", instanceId],
    queryFn: () => api.modSets.instance(instanceId),
    enabled,
    staleTime: 15_000,
    refetchOnMount: "always",
    retry: false,
  });
}

function addSummaries(summaries: SyncSummary[]): SyncSummary {
  const zero = () => ({ mods: 0, resourcePacks: 0, shaderPacks: 0, configs: 0, other: 0 });
  const total: SyncSummary = {
    added: zero(),
    updated: zero(),
    removed: zero(),
    kept: 0,
    unchanged: 0,
    downloadBytes: 0,
    removeBytes: 0,
  };
  for (const s of summaries) {
    for (const action of ["added", "updated", "removed"] as const) {
      for (const kind of ["mods", "resourcePacks", "shaderPacks", "configs", "other"] as const) {
        total[action][kind] += s[action][kind];
      }
    }
    total.downloadBytes += s.downloadBytes;
    total.removeBytes += s.removeBytes;
  }
  return total;
}

/** New items count as seen once the set has been on screen for a moment and the player leaves. */
function useMarkSeenOnLeave(instanceId: string, setId: string, hasNew: boolean, locked: boolean) {
  const queryClient = useQueryClient();
  const state = useRef({ hasNew, locked });
  state.current = { hasNew, locked };
  useEffect(() => {
    const since = performance.now();
    return () => {
      const { hasNew: anyNew, locked: busy } = state.current;
      if (!anyNew || busy || performance.now() - since < SEEN_AFTER_MS || document.hidden) return;
      api.modSets
        .markSeen(instanceId, setId)
        .then(() => queryClient.invalidateQueries({ queryKey: ["mod-sets-instance", instanceId] }))
        .catch(() => undefined);
    };
  }, [instanceId, setId, queryClient]);
}

function SetPanel({
  instanceId,
  mcVersion,
  set,
  locked,
  running,
  onRun,
}: {
  instanceId: string;
  mcVersion: string;
  set: InstanceSet;
  locked: boolean;
  /** A change of this set runs now. */
  running: JobProgress | null;
  onRun: (selection: ModSelection | undefined, kind: RunKind) => void;
}) {
  const { t } = useTranslation();
  const words = useWords();
  const prefs = useMotionPrefs();
  const queryClient = useQueryClient();
  const key = draftKey(instanceId, set.id);
  const picked = useSetDrafts((s) => s.choices[key] ?? noChoices);
  const setDraft = useSetDrafts((s) => s.set);
  const choices = useMemo(() => ({ ...choicesOf(set.items), ...picked }), [set.items, picked]);
  const enabled = useMemo(() => enabledSet(set.items, choices), [set.items, choices]);
  const position = prefs.layout ? ("position" as const) : false;
  useMarkSeenOnLeave(
    instanceId,
    set.id,
    set.items.some((item) => item.isNew && !item.library),
    locked,
  );

  const toggle = (item: ModCenterItem, on: boolean) => {
    const next = { ...picked };
    if (on === item.selected) delete next[item.id];
    else next[item.id] = on;
    setDraft(key, next);
  };
  const name = setName(set, words.english);
  const summary = setSummary(set, words.english);
  const choosable = set.items.filter((item) => !item.library);
  const libraries = set.items.filter((item) => item.library && enabled.has(item.id));

  const remove = async () => {
    const before = Object.fromEntries(choosable.map((item) => [item.id, item.selected]));
    const jobId = newJobId();
    useSetJob.getState().set(instanceId, {
      setId: set.id,
      progress: { stage: "removing", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 },
    });
    try {
      await api.modSets.remove(instanceId, set.id, jobId, (progress) =>
        useSetJob.getState().set(instanceId, { setId: set.id, progress }),
      );
      setDraft(key, null);
      toast.success(t("modSets.center.removed", { name }), {
        action: {
          label: t("modSets.center.undo"),
          // Brings the set back with the choices it had (its files download again).
          onClick: () => onRun({ items: before }, "add"),
        },
      });
    } catch (error) {
      showError(error);
    } finally {
      useSetJob.getState().set(instanceId, null);
      refreshSets(queryClient, instanceId);
    }
  };

  // One action per state, in the header's fixed slot.
  const action = !set.installed ? (
    set.available ? (
      <Button
        size="sm"
        variant="secondary"
        disabled={locked}
        onClick={() => onRun(undefined, "apply")}
      >
        {t("modSets.center.install", { version: mcVersion })}
      </Button>
    ) : null
  ) : set.updateAvailable && set.available ? (
    <Button
      size="sm"
      variant="secondary"
      icon={<ArrowsClockwise size={14} />}
      disabled={locked}
      onClick={() => onRun(undefined, "update")}
    >
      {t("modSets.center.updateNow")}
    </Button>
  ) : set.pending ? (
    <Button
      size="sm"
      variant="secondary"
      disabled={locked}
      onClick={() => onRun(undefined, "apply")}
    >
      {t("modSets.center.complete")}
    </Button>
  ) : null;

  return (
    <motion.section
      layout={position}
      transition={{ layout: glide }}
      {...fade(0.2)}
      aria-label={name}
      className="glass rounded-[var(--radius-panel)]"
    >
      <header className="flex items-start gap-3 px-4 pt-3.5 pb-2.5">
        <PurposeMark purpose={set.purpose} size={34} />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
            <h3 className="truncate font-display text-[16px] font-semibold text-bone-50">{name}</h3>
            {set.installed && set.updateAvailable ? (
              <Chip tone="gold">{t("modSets.center.update")}</Chip>
            ) : null}
            {set.retired ? <Chip>{t("modSets.center.retired")}</Chip> : null}
          </div>
          <p className="mt-0.5 flex flex-wrap gap-x-3 text-[12.5px] text-ash-400 tabular-nums">
            <span>{mcVersion}</span>
            {set.sizeBytes > 0 ? <span>{formatBytes(set.sizeBytes)}</span> : null}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {action}
          <Menu
            trigger={
              <IconButton
                label={t("modSets.center.actions", { name })}
                icon={<DotsThree size={18} weight="bold" />}
                size="sm"
                disabled={locked}
              />
            }
            items={[
              {
                label: t("modSets.center.remove"),
                icon: <Trash size={16} />,
                danger: true,
                onSelect: () => void remove(),
              },
            ]}
          />
        </div>
      </header>
      {summary ? (
        <p className="-mt-1 px-4 pb-2.5 pl-[62px] text-[13px] text-pretty text-ash-300">
          {summary}
        </p>
      ) : null}
      <AnimatePresence initial={false} mode="popLayout">
        {running ? (
          <motion.div key="job" {...fade(0.16)} className="px-4 pb-3">
            <JobLine progress={running} label={t("modCenter.bar.applying")} />
          </motion.div>
        ) : null}
      </AnimatePresence>
      {!set.installed ? (
        <p className="px-4 pb-3.5 pl-[62px] text-[13px] text-ash-300">
          {set.available ? null : t("modSets.center.notForVersion", { version: mcVersion })}
        </p>
      ) : (
        <div className="px-1.5 pb-2">
          {set.pending && !set.updateAvailable ? (
            <p className="px-3 pb-1 text-[12.5px] text-ash-300">{t("modSets.center.pending")}</p>
          ) : null}
          <ul className="space-y-0.5">
            {choosable.map((item) => (
              <ItemRow
                key={item.id}
                item={item}
                items={set.items}
                enabled={enabled}
                changed={(choices[item.id] ?? item.selected) !== item.selected}
                reverting={false}
                locked={locked}
                prefs={prefs}
                onToggle={(on) => toggle(item, on)}
                onRevert={() => undefined}
              />
            ))}
          </ul>
          {libraries.length > 0 || set.present.length > 0 ? (
            <div className="mt-1 space-y-0.5 border-t border-white/6 px-3 pt-2 pb-1 text-[12.5px] text-ash-400">
              {libraries.length > 0 ? (
                <p className="truncate">
                  {t("modSets.create.libraries")}
                  {": "}
                  {listFormat().format(libraries.map((item) => item.title))}
                </p>
              ) : null}
              {set.present.length > 0 ? (
                <p className="truncate">
                  {t("modSets.center.present")}
                  {": "}
                  {listFormat().format(set.present.map((item) => item.title))}
                </p>
              ) : null}
            </div>
          ) : null}
        </div>
      )}
    </motion.section>
  );
}

/**
 * "Setler" in the mod center: the ready-made sets the instance has, each with its items and
 * switches (applied together from the change bar), its update and its removal.
 */
export function InstanceSets({ instance, onAdd }: { instance: InstanceDetail; onAdd: () => void }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const view = useInstanceSets(instance.id);
  const job = useSetJob((s) => s.jobs[instance.id] ?? null);
  const drafts = useSetDrafts((s) => s.choices);
  const setDraft = useSetDrafts((s) => s.set);
  const sets = view.data?.sets ?? [];
  const locked = Boolean(view.data?.gameRunning) || job !== null;

  // Pending choices per set, against what each set has now.
  const pending = useMemo(
    () =>
      sets
        .map((set) => {
          const picked = drafts[draftKey(instance.id, set.id)];
          if (!picked) return null;
          const selection = pendingSelection(
            set.items,
            { ...choicesOf(set.items), ...picked },
            new Set(),
          );
          return selection ? { set, selection } : null;
        })
        .filter((entry): entry is { set: InstanceSet; selection: ModSelection } => entry !== null),
    [sets, drafts, instance.id],
  );
  const settled = useDebounced(pending, 200);
  const previews = useQueries({
    queries: settled.map(({ set, selection }) => ({
      queryKey: ["mod-set-preview", instance.id, set.id, selection],
      queryFn: () => api.modSets.preview(instance.id, set.id, selection),
      staleTime: 30_000,
      retry: false,
    })),
  });
  const ready = previews.length > 0 && previews.every((p) => p.data);
  const barText = ready
    ? describeSummary(
        addSummaries(previews.map((p) => p.data?.summary as SyncSummary)),
        t,
        (parts) => listFormat().format(parts),
        formatBytes,
      )
    : null;
  const stale = settled !== pending || previews.some((p) => p.isFetching);

  const run = async (setId: string, selection: ModSelection | undefined, kind: RunKind) => {
    if (useSetJob.getState().jobs[instance.id]) return;
    const set = sets.find((s) => s.id === setId);
    useSetJob.getState().set(instance.id, {
      setId,
      progress: { stage: "resolving", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 },
    });
    try {
      const result = await api.modSets.apply(
        instance.id,
        setId,
        selection,
        newJobId(),
        (progress) => useSetJob.getState().set(instance.id, { setId, progress }),
      );
      await queryClient.refetchQueries({
        queryKey: ["mod-sets-instance", instance.id],
        exact: true,
      });
      setDraft(draftKey(instance.id, setId), null);
      const name = set?.name ?? setId;
      toast.success(
        kind === "update"
          ? t("modSets.center.updated", { name })
          : kind === "add"
            ? t("modSets.center.added", { name })
            : t("modSets.center.applied", { name }),
      );
      if (result.issues > 0) toast(t("modSets.center.issues"));
    } catch (error) {
      showError(error);
    } finally {
      useSetJob.getState().set(instance.id, null);
      refreshSets(queryClient, instance.id);
    }
  };
  const applyAll = async () => {
    for (const { set, selection } of pending) await run(set.id, selection, "apply");
  };

  if (view.isPending) {
    return (
      <div aria-busy className="space-y-3">
        <Skeleton className="h-[220px] w-full rounded-[var(--radius-panel)]" />
      </div>
    );
  }
  if (view.isError || !view.data) {
    return (
      <EmptyState
        title={t("modCenter.loadFailed")}
        body={view.error instanceof Error ? view.error.message : undefined}
        action={
          <Button variant="secondary" onClick={() => void view.refetch()}>
            {t("common.retry")}
          </Button>
        }
      />
    );
  }
  if (sets.length === 0) {
    return (
      <EmptyState
        icon={<ContentIcon kind="pack" />}
        title={t("modSets.center.emptyTitle")}
        action={
          <Button variant="secondary" icon={<Plus size={16} weight="bold" />} onClick={onAdd}>
            {t("modSets.center.add")}
          </Button>
        }
      />
    );
  }
  const barMode =
    job && pending.length > 0 ? "applying" : view.data.gameRunning ? "running" : "summary";
  return (
    <div className="relative flex min-h-full flex-col gap-3 pb-8">
      {view.data.unavailable ? (
        <p className="px-1 text-[12.5px] text-ash-400">{t("modSets.center.unavailable")}</p>
      ) : null}
      <AnimatePresence initial={false}>
        {sets.map((set) => (
          <SetPanel
            key={set.id}
            instanceId={instance.id}
            mcVersion={view.data.mcVersion}
            set={set}
            locked={locked}
            running={job?.setId === set.id ? job.progress : null}
            onRun={(selection, kind) => void run(set.id, selection, kind)}
          />
        ))}
      </AnimatePresence>
      <ChangeBar
        open={pending.length > 0 || barMode === "applying"}
        label={t("modCenter.bar.label")}
      >
        <AnimatePresence initial={false} mode="popLayout">
          {barMode === "applying" ? (
            <motion.div key="applying" {...swap(0.2)} className="min-w-0 flex-1">
              <JobLine progress={job?.progress ?? null} label={t("modCenter.bar.applying")} />
            </motion.div>
          ) : barMode === "running" ? (
            <motion.p key="running" {...swap(0.2)} className="flex-1 text-[13px] text-ash-300">
              {t("modCenter.bar.gameRunning")}
            </motion.p>
          ) : (
            <motion.div
              key="summary"
              {...swap(0.2)}
              className="flex min-w-0 flex-1 items-center gap-4"
            >
              <div className="relative min-w-0 flex-1" aria-live="polite">
                {barText === null ? (
                  <Skeleton className="h-3.5 w-56 max-w-full" />
                ) : (
                  <p
                    className={cn(
                      "text-[13.5px] text-bone-100 transition-opacity duration-200",
                      stale && "opacity-55",
                    )}
                  >
                    {barText}
                  </p>
                )}
              </div>
              <Button
                variant="ghost"
                onClick={() => {
                  for (const { set } of pending) setDraft(draftKey(instance.id, set.id), null);
                }}
              >
                {t("modCenter.bar.discard")}
              </Button>
              <Button variant="primary" disabled={locked} onClick={() => void applyAll()}>
                {t("modCenter.bar.apply")}
              </Button>
            </motion.div>
          )}
        </AnimatePresence>
      </ChangeBar>
    </div>
  );
}
