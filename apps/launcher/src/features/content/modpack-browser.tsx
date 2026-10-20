import { ArrowLeft, DownloadSimple, Info } from "@phosphor-icons/react";
import {
  Button,
  cn,
  EmptyState,
  Field,
  IconButton,
  Input,
  PixelBrackets,
  SearchInput,
  Select,
  Skeleton,
} from "@pl/ui";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import offlineArt from "@/assets/art/empty-offline.webp";
import { api, type ContentHit, type ModpackImportResult } from "@/lib/api";
import { notifyWhenAway } from "@/lib/attention";
import { formatRelative } from "@/lib/format";
import { trackTask } from "@/stores/progress";
import { isOffline, newJobId, type RunningJob, showError } from "./jobs";
import { categoryLabel, formatCompact, JobStatus, ProjectIcon } from "./parts";
import { ProjectDialog } from "./project-dialog";
import { useDebounced } from "./use-content";

const PAGE_SIZE = 20;

/** Loading rows: enough to fill the dialog's full height in a window up to ~1100 px tall. */
const SKELETON_ROWS = Array.from({ length: 10 }, (_, i) => i);

function PackRow({
  hit,
  selected,
  onSelect,
}: {
  hit: ContentHit;
  selected: boolean;
  onSelect: () => void;
}) {
  const { t } = useTranslation();
  return (
    <li className="relative">
      {selected ? <PixelBrackets size={8} className="-inset-0.5" /> : null}
      <button
        type="button"
        onClick={onSelect}
        aria-pressed={selected}
        className={cn(
          "flex w-full cursor-pointer items-center gap-3.5 rounded-[var(--radius-panel)] px-3 py-2.5 text-left transition-colors hover:bg-white/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
          selected && "bg-white/7",
        )}
      >
        <ProjectIcon src={hit.iconUrl} size={44} />
        <span className="min-w-0 flex-1">
          <span className="flex items-baseline gap-2">
            <span className="truncate text-[14px] font-semibold text-bone-50">{hit.title}</span>
            <span className="shrink-0 truncate text-[12px] text-ash-400">
              {t("content.browse.by", { author: hit.author })}
            </span>
          </span>
          <span className="mt-0.5 block truncate text-[12.5px] text-ash-400">
            {hit.description}
          </span>
          <span className="mt-1 flex flex-wrap gap-x-3.5 text-[11.5px] text-ash-400">
            <span className="flex items-center gap-1 tabular-nums">
              <DownloadSimple size={12} aria-hidden />
              {formatCompact(hit.downloads)}
            </span>
            <span>{formatRelative(hit.dateModified)}</span>
            {hit.categories.slice(0, 2).map((c) => (
              <span key={c}>{categoryLabel(t, c)}</span>
            ))}
          </span>
        </span>
      </button>
    </li>
  );
}

/**
 * "Find a modpack" in the new-instance dialog: search Modrinth modpacks, pick one and a version,
 * install it as a new instance. Returns the body and the footer for the dialog.
 */
export function useModpackBrowser({
  active,
  onCancel,
  onDone,
}: {
  active: boolean;
  onCancel: () => void;
  onDone: (result: ModpackImportResult) => void;
}): { body: ReactNode; footer: ReactNode; cancelJob: () => void } {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const debounced = useDebounced(query.trim(), 350);
  const [selected, setSelected] = useState<ContentHit | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [details, setDetails] = useState(false);
  const [versionId, setVersionId] = useState("latest");
  const [name, setName] = useState("");
  const [job, setJob] = useState<RunningJob | null>(null);

  useEffect(() => {
    if (!active) {
      setSelected(null);
      setConfirming(false);
      setQuery("");
    }
  }, [active]);

  const search = useInfiniteQuery({
    queryKey: ["content-search", "modpack", null, debounced, "relevance", "all"],
    queryFn: ({ pageParam }) =>
      api.content.search({
        projectType: "modpack",
        query: debounced,
        sort: "relevance",
        offset: pageParam,
        limit: PAGE_SIZE,
      }),
    initialPageParam: 0,
    getNextPageParam: (last) => {
      const next = last.offset + last.hits.length;
      return last.hits.length > 0 && next < last.total ? next : undefined;
    },
    enabled: active,
    staleTime: 2 * 60_000,
    retry: false,
  });
  const versions = useQuery({
    queryKey: ["content-versions", selected?.projectId, null, null],
    queryFn: () => api.content.versions(selected?.projectId ?? ""),
    enabled: Boolean(selected) && confirming,
    staleTime: 5 * 60_000,
  });

  const { hasNextPage, isFetchingNextPage, fetchNextPage } = search;
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const node = sentinel.current;
    if (!node || confirming) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting) && hasNextPage && !isFetchingNextPage) {
          fetchNextPage().catch(showError);
        }
      },
      { rootMargin: "240px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage, confirming]);

  const start = async () => {
    if (!selected) return;
    const jobId = newJobId();
    setJob({ jobId });
    // The taskbar button follows the install; a player who went elsewhere hears when it ends.
    const task = trackTask(`modpack:${jobId}`);
    try {
      const result = await api.content.installModpack(
        {
          projectId: selected.projectId,
          versionId: versionId === "latest" ? undefined : versionId,
          name: name.trim() || undefined,
          icon: "compass",
          jobId,
        },
        (progress) => {
          setJob({ jobId, progress });
          task.report(progress.percent);
        },
      );
      setSelected(null);
      setConfirming(false);
      void notifyWhenAway(
        task.done(),
        t("attention.pack.title"),
        t("attention.pack.body", { name: result.instance.name }),
      );
      onDone(result);
    } catch (error) {
      showError(error);
    } finally {
      task.done();
      setJob(null);
    }
  };

  const cancelJob = () => {
    if (job) void api.content.cancel(job.jobId);
  };

  const hits = search.data?.pages.flatMap((p) => p.hits) ?? [];

  // One `minmax(0, 1fr)` column (`grid-cols-1`): an auto track would grow to the longest
  // description and push the list and the search field past the dialog's edge.
  const list = (
    <div className="grid grid-cols-1 gap-3">
      <SearchInput
        value={query}
        onChange={setQuery}
        placeholder={t("content.browse.search.modpack")}
        label={t("content.browse.search.modpack")}
        clearLabel={t("library.clearSearch")}
      />
      {search.isPending ? (
        // As many rows as a tall window shows: the dialog glides to its full height once, when the
        // player picks this tab, and the results then fill it in place. With four rows it glided to
        // 503 px, stopped, and grew again to 563 px when they came (1024 × 640; POLISH6).
        <div className="space-y-2">
          {SKELETON_ROWS.map((i) => (
            <Skeleton key={i} className="h-[66px] w-full" />
          ))}
        </div>
      ) : search.isError ? (
        isOffline(search.error) ? (
          <EmptyState
            illustration={offlineArt}
            title={t("content.browse.offlineTitle")}
            body={t("content.browse.offlineBody")}
            action={
              <Button variant="secondary" onClick={() => void search.refetch()}>
                {t("common.retry")}
              </Button>
            }
            className="py-4"
          />
        ) : (
          <EmptyState
            title={t("content.browse.errorTitle")}
            body={search.error instanceof Error ? search.error.message : undefined}
            action={
              <Button variant="secondary" onClick={() => void search.refetch()}>
                {t("common.retry")}
              </Button>
            }
          />
        )
      ) : hits.length === 0 ? (
        <EmptyState title={t("content.browse.emptyTitle")} body={t("content.browse.emptyBody")} />
      ) : (
        <>
          <ul className="space-y-1">
            {hits.map((hit) => (
              <PackRow
                key={hit.projectId}
                hit={hit}
                selected={selected?.projectId === hit.projectId}
                onSelect={() => {
                  setSelected(hit);
                  setName(hit.title);
                  setVersionId("latest");
                }}
              />
            ))}
          </ul>
          <div ref={sentinel} aria-hidden className="h-px" />
          {isFetchingNextPage ? <Skeleton className="h-[66px] w-full" /> : null}
        </>
      )}
    </div>
  );

  const confirm = selected ? (
    <div className="grid grid-cols-1 gap-4">
      <div className="flex items-center gap-2">
        <IconButton
          label={t("modpack.browse.back")}
          icon={<ArrowLeft size={18} />}
          size="sm"
          disabled={Boolean(job)}
          onClick={() => setConfirming(false)}
        />
        <span className="text-[13px] text-ash-400">{t("modpack.browse.selected")}</span>
      </div>
      <div className="glass flex items-start gap-4 rounded-[var(--radius-panel)] p-4">
        <ProjectIcon src={selected.iconUrl} size={64} />
        <div className="min-w-0 flex-1">
          <p className="truncate font-display text-[17px] font-semibold text-bone-50">
            {selected.title}
          </p>
          <p className="text-[12.5px] text-ash-400">
            {t("content.browse.by", { author: selected.author })}
          </p>
          <p className="mt-1.5 line-clamp-3 text-[13px] text-ash-300">{selected.description}</p>
        </div>
        <IconButton
          label={t("content.actions.details")}
          icon={<Info size={18} />}
          size="sm"
          onClick={() => setDetails(true)}
        />
      </div>
      <div className="grid grid-cols-2 gap-4">
        <Field label={t("create.name")} htmlFor="modpack-name">
          <Input
            id="modpack-name"
            value={name}
            maxLength={48}
            disabled={Boolean(job)}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        <Field label={t("modpack.browse.version")}>
          <Select
            value={versionId}
            onValueChange={setVersionId}
            label={t("modpack.browse.version")}
            disabled={Boolean(job) || !versions.data?.length}
            options={[
              { value: "latest", label: t("modpack.browse.latest") },
              ...(versions.data ?? []).slice(0, 40).map((v) => ({
                value: v.id,
                label: v.versionNumber,
                hint: [
                  v.gameVersions.slice(-2).join(", "),
                  v.versionType === "release"
                    ? ""
                    : t(`content.detail.versionTypes.${v.versionType}`),
                ]
                  .filter(Boolean)
                  .join(", "),
              })),
            ]}
          />
        </Field>
      </div>
    </div>
  ) : null;

  const body = (
    <>
      {confirming && confirm ? confirm : list}
      <ProjectDialog
        projectId={details && selected ? selected.projectId : null}
        hint={selected ?? undefined}
        onClose={() => setDetails(false)}
        footer={null}
      />
    </>
  );

  const footer = job ? (
    <JobStatus job={job} onCancel={cancelJob} className="w-full" detailed />
  ) : (
    <>
      {!selected ? (
        <p className="mr-auto text-[12.5px] text-ash-400">{t("modpack.browse.pick")}</p>
      ) : null}
      <Button variant="ghost" onClick={onCancel}>
        {t("create.cancel")}
      </Button>
      {confirming ? (
        <Button
          variant="primary"
          icon={<DownloadSimple size={16} weight="bold" />}
          onClick={() => void start()}
        >
          {t("modpack.browse.submit")}
        </Button>
      ) : (
        <Button variant="primary" disabled={!selected} onClick={() => setConfirming(true)}>
          {t("modpack.browse.next")}
        </Button>
      )}
    </>
  );

  return { body, footer, cancelJob };
}
