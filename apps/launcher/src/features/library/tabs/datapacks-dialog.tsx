import { CheckCircle, DownloadSimple, FolderSimple, Trash } from "@phosphor-icons/react";
import {
  Button,
  Dialog,
  IconButton,
  SearchInput,
  Skeleton,
  toast,
  useInteractionMotion,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { isCancelled, newJobId, type RunningJob, showError } from "@/features/content/jobs";
import { formatCompact, JobStatus, ProjectIcon } from "@/features/content/parts";
import { useDebounced } from "@/features/content/use-content";
import {
  api,
  type ContentHit,
  type Datapack,
  type InstanceDetail,
  type WorldDetail,
} from "@/lib/api";
import { formatBytes } from "@/lib/format";

const ease = [0.16, 1, 0.3, 1] as const;

function RowsSkeleton({ rows }: { rows: number }) {
  return (
    <div aria-busy className="space-y-2">
      {Array.from({ length: rows }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: static placeholders
        <div key={i} className="flex items-center gap-3 px-1 py-1.5">
          <Skeleton className="size-9 shrink-0" />
          <div className="flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-1/3" />
            <Skeleton className="h-3 w-2/3" />
          </div>
        </div>
      ))}
    </div>
  );
}

/**
 * A world's data packs: what is in `datapacks/`, and Modrinth data packs made for the
 * instance's Minecraft version to add with one click.
 */
export function DatapacksDialog({
  instance,
  world,
  onClose,
}: {
  instance: InstanceDetail;
  world: WorldDetail | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const move = useInteractionMotion();
  // Rows glide when one leaves, except in Performance mode and under reduced motion.
  const { layout } = useMotionPrefs();
  const folder = world?.folder ?? "";
  const open = Boolean(world);
  const [search, setSearch] = useState("");
  const query = useDebounced(search.trim(), 350);
  const [jobs, setJobs] = useState<Record<string, RunningJob>>({});
  // Projects added from this dialog (installed files do not say which project they came from).
  const [added, setAdded] = useState<Set<string>>(() => new Set());
  useEffect(() => {
    if (!folder) return;
    setAdded(new Set());
    setSearch("");
  }, [folder]);

  const listKey = ["datapacks", instance.id, folder];
  const installed = useQuery({
    queryKey: listKey,
    queryFn: () => api.content.datapacks(instance.id, folder),
    enabled: open,
  });
  const results = useQuery({
    queryKey: ["content-search", "datapack", instance.id, query],
    queryFn: () =>
      api.content.search({
        projectType: "datapack",
        query,
        sort: query ? "relevance" : "downloads",
        offset: 0,
        limit: 12,
        instanceId: instance.id,
      }),
    enabled: open,
    staleTime: 5 * 60_000,
    retry: false,
  });

  const install = async (hit: ContentHit) => {
    const jobId = newJobId();
    setJobs((prev) => ({ ...prev, [hit.projectId]: { jobId } }));
    try {
      await api.content.installDatapack(
        { instanceId: instance.id, world: folder, projectId: hit.projectId, jobId },
        (progress) => setJobs((prev) => ({ ...prev, [hit.projectId]: { jobId, progress } })),
      );
      toast.success(t("datapacks.added", { name: hit.title }));
      setAdded((prev) => new Set(prev).add(hit.projectId));
      await queryClient.invalidateQueries({ queryKey: listKey });
    } catch (error) {
      if (!isCancelled(error)) showError(error);
    } finally {
      setJobs((prev) => {
        const { [hit.projectId]: _done, ...rest } = prev;
        return rest;
      });
    }
  };

  const remove = async (pack: Datapack) => {
    queryClient.setQueryData<Datapack[]>(listKey, (prev) =>
      prev?.filter((p) => p.fileName !== pack.fileName),
    );
    try {
      await api.content.removeDatapack(instance.id, folder, pack.fileName);
      toast(t("datapacks.removed", { name: pack.name }));
    } catch (error) {
      showError(error);
    } finally {
      void queryClient.invalidateQueries({ queryKey: listKey });
    }
  };

  const enter = (index: number) => ({
    initial: { opacity: 0, transform: move ? "translateY(4px)" : "none" },
    animate: {
      opacity: 1,
      transform: "translateY(0px)",
      transition: { duration: 0.22, ease, delay: Math.min(index, 8) * 0.03 },
    },
    exit: { opacity: 0, transition: { duration: 0.14 } },
  });

  const packs = installed.data ?? [];
  const hits = results.data?.hits ?? [];

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && onClose()}
      title={t("datapacks.title")}
      dismissOnOutsideClick={false}
      description={world?.name}
      closeLabel={t("common.close")}
      width={600}
    >
      <section aria-label={t("datapacks.installed")}>
        <h3 className="mb-2 text-[12.5px] font-medium text-ash-300">{t("datapacks.installed")}</h3>
        {installed.isPending ? (
          <RowsSkeleton rows={2} />
        ) : installed.isError ? (
          <p className="text-[13px] text-flare-400">{t("datapacks.listFailed")}</p>
        ) : packs.length === 0 ? (
          <p className="text-[13px] text-ash-400">{t("datapacks.empty")}</p>
        ) : (
          <ul className="space-y-0.5">
            <AnimatePresence initial={false}>
              {packs.map((pack, index) => (
                <motion.li
                  key={pack.fileName}
                  layout={layout ? "position" : false}
                  {...enter(index)}
                  className="flex items-center gap-3 rounded-md px-1 py-1.5"
                >
                  {pack.folder && !pack.icon ? (
                    <span className="chamfer flex size-9 shrink-0 items-center justify-center bg-obsidian-800 [--cut:6px]">
                      <FolderSimple size={17} className="text-gold-400/80" />
                    </span>
                  ) : (
                    <ProjectIcon src={pack.icon} size={36} />
                  )}
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-[13.5px] font-medium text-bone-50">{pack.name}</p>
                    <p className="flex gap-3 text-[12px] text-ash-400">
                      {pack.description ? (
                        <span className="min-w-0 truncate">{pack.description}</span>
                      ) : null}
                      <span className="shrink-0 tabular-nums">
                        {pack.folder ? t("datapacks.folder") : formatBytes(pack.sizeBytes)}
                      </span>
                    </p>
                  </div>
                  <IconButton
                    label={t("datapacks.remove")}
                    icon={<Trash size={16} />}
                    size="sm"
                    onClick={() => void remove(pack)}
                  />
                </motion.li>
              ))}
            </AnimatePresence>
          </ul>
        )}
      </section>

      <section aria-label={t("datapacks.add")} className="mt-6">
        <h3 className="mb-2 text-[12.5px] font-medium text-ash-300">{t("datapacks.add")}</h3>
        <SearchInput
          value={search}
          onChange={setSearch}
          placeholder={t("datapacks.search")}
          label={t("datapacks.search")}
        />
        <div className="mt-3 max-h-[300px] overflow-y-auto pr-1">
          {results.isPending ? (
            <RowsSkeleton rows={4} />
          ) : results.isError ? (
            <div className="flex items-center justify-between gap-3 py-2">
              <p className="text-[13px] text-bone-100">
                {results.error instanceof Error ? results.error.message : ""}
              </p>
              <Button size="sm" variant="secondary" onClick={() => void results.refetch()}>
                {t("common.retry")}
              </Button>
            </div>
          ) : hits.length === 0 ? (
            <p className="py-2 text-[13px] text-ash-400">{t("datapacks.noResults")}</p>
          ) : (
            <ul className="space-y-0.5">
              {hits.map((hit, index) => {
                const job = jobs[hit.projectId];
                return (
                  <motion.li
                    key={hit.projectId}
                    {...enter(index)}
                    className="flex items-center gap-3 rounded-md px-1 py-1.5"
                  >
                    <ProjectIcon src={hit.iconUrl} size={36} />
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-[13.5px] font-medium text-bone-50">{hit.title}</p>
                      <p className="flex gap-3 text-[12px] text-ash-400">
                        <span className="min-w-0 truncate">{hit.description}</span>
                        <span className="shrink-0 tabular-nums">
                          {formatCompact(hit.downloads)}
                        </span>
                      </p>
                    </div>
                    {job ? (
                      <JobStatus
                        job={job}
                        className="w-36"
                        onCancel={() => void api.content.cancel(job.jobId)}
                      />
                    ) : added.has(hit.projectId) ? (
                      <motion.span
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1, transition: { duration: 0.2 } }}
                        className="flex items-center gap-1.5 px-2 text-[12.5px] font-medium text-jade-400"
                      >
                        <CheckCircle size={15} weight="fill" />
                        {t("datapacks.done")}
                      </motion.span>
                    ) : (
                      <Button
                        size="sm"
                        variant="secondary"
                        icon={<DownloadSimple size={15} className="text-gold-300" />}
                        onClick={() => void install(hit)}
                      >
                        {t("datapacks.install")}
                      </Button>
                    )}
                  </motion.li>
                );
              })}
            </ul>
          )}
        </div>
      </section>
    </Dialog>
  );
}
