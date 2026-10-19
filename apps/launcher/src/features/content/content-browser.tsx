import { ArrowLeft, ClockCounterClockwise, DownloadSimple } from "@phosphor-icons/react";
import {
  Button,
  EmptyState,
  IconButton,
  SearchInput,
  Select,
  type SelectOption,
  Skeleton,
} from "@pl/ui";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import offlineArt from "@/assets/art/empty-offline.webp";
import {
  api,
  type ContentHit,
  type ContentKind,
  type ContentSort,
  type InstalledContent,
  type InstanceDetail,
} from "@/lib/api";
import { formatCount, formatRelative } from "@/lib/format";
import {
  isOffline,
  projectJobKey,
  type RunningJob,
  showError,
  useContentActions,
  useContentJobs,
} from "./jobs";
import {
  categoryLabel,
  formatCompact,
  InstallControl,
  installState,
  ProjectIcon,
  projectTypeOf,
} from "./parts";
import { ProjectDialog } from "./project-dialog";
import { useContentScan, useDebounced } from "./use-content";

const PAGE_SIZE = 20;
const HEADER_ORDER = ["categories", "features", "resolutions", "performance impact"];
const GROUP_KEYS: Record<string, string> = {
  features: "content.categoryGroups.features",
  resolutions: "content.categoryGroups.resolutions",
  "performance impact": "content.categoryGroups.performance",
};

function RowSkeleton() {
  return (
    <div className="flex items-center gap-4 px-3 py-3">
      <Skeleton className="size-[52px] shrink-0" />
      <div className="flex-1 space-y-2">
        <Skeleton className="h-4 w-56" />
        <Skeleton className="h-3.5 w-4/5" />
        <Skeleton className="h-3 w-40" />
      </div>
      <Skeleton className="h-8 w-24" />
    </div>
  );
}

function HitRow({
  hit,
  entry,
  job,
  onOpen,
  onInstall,
  onUpdate,
  onRemove,
  onCancel,
}: {
  hit: ContentHit;
  entry?: InstalledContent;
  job?: RunningJob;
  onOpen: () => void;
  onInstall: () => void;
  onUpdate: () => void;
  onRemove: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const updated = formatRelative(hit.dateModified);
  return (
    <li className="relative flex items-center gap-4 rounded-[var(--radius-panel)] px-3 py-3 transition-colors hover:bg-white/5">
      <ProjectIcon src={hit.iconUrl} size={52} />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-baseline gap-2">
          <button
            type="button"
            onClick={onOpen}
            className="min-w-0 cursor-pointer truncate text-left text-[15px] font-semibold text-bone-50 outline-none transition-colors duration-150 after:absolute after:inset-0 after:rounded-[var(--radius-panel)] hover:text-gold-200 focus-visible:after:ring-2 focus-visible:after:ring-gold-300"
          >
            {hit.title}
          </button>
          <span className="shrink-0 truncate text-[12.5px] text-ash-400">
            {t("content.browse.by", { author: hit.author })}
          </span>
        </div>
        <p className="mt-0.5 line-clamp-2 text-[13px] leading-snug text-ash-400">
          {hit.description}
        </p>
        <div className="mt-1.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-[12px] text-ash-400">
          <span
            className="flex items-center gap-1 tabular-nums"
            title={t("content.browse.downloads", { count: formatCount(hit.downloads) })}
          >
            <DownloadSimple size={13} aria-hidden />
            {formatCompact(hit.downloads)}
          </span>
          <span
            className="flex items-center gap-1"
            title={t("content.browse.updated", { when: updated })}
          >
            <ClockCounterClockwise size={13} aria-hidden />
            {updated}
          </span>
          {hit.categories.slice(0, 3).map((c) => (
            <span key={c}>{categoryLabel(t, c)}</span>
          ))}
        </div>
      </div>
      <div className="relative z-10 shrink-0">
        <InstallControl
          state={installState(entry)}
          job={job}
          onInstall={onInstall}
          onUpdate={onUpdate}
          onRemove={onRemove}
          onCancel={onCancel}
        />
      </div>
    </li>
  );
}

/**
 * Modrinth browser inside an instance: search, sort, categories and infinite scroll, limited to
 * what runs on the instance. Rows show installed and update state; a row opens the project page.
 */
export function ContentBrowser({
  instance,
  kind,
  onBack,
  embedded = false,
}: {
  instance: InstanceDetail;
  kind: ContentKind;
  onBack?: () => void;
  /** Inside another page (the mod center's "Keşfet"): no back header. */
  embedded?: boolean;
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<ContentSort>("relevance");
  const [category, setCategory] = useState("all");
  const [selected, setSelected] = useState<ContentHit | null>(null);
  const debounced = useDebounced(query.trim(), 350);
  const projectType = projectTypeOf(kind);

  const search = useInfiniteQuery({
    queryKey: ["content-search", projectType, instance.id, debounced, sort, category],
    queryFn: ({ pageParam }) =>
      api.content.search({
        projectType,
        query: debounced,
        sort,
        category: category === "all" ? undefined : category,
        offset: pageParam,
        limit: PAGE_SIZE,
        instanceId: instance.id,
      }),
    initialPageParam: 0,
    getNextPageParam: (last) => {
      const next = last.offset + last.hits.length;
      return last.hits.length > 0 && next < last.total ? next : undefined;
    },
    staleTime: 2 * 60_000,
    retry: false,
  });
  const categories = useQuery({
    queryKey: ["content-categories"],
    queryFn: () => api.content.categories(),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
  const scan = useContentScan(instance.id, kind);
  const installed = useMemo(
    () =>
      new Map(
        (scan.data?.entries ?? []).flatMap((e) => (e.projectId ? [[e.projectId, e] as const] : [])),
      ),
    [scan.data],
  );
  const actions = useContentActions(instance.id, kind);
  const jobs = useContentJobs((s) => s.jobs);

  const { hasNextPage, isFetchingNextPage, fetchNextPage } = search;
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const node = sentinel.current;
    if (!node) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting) && hasNextPage && !isFetchingNextPage) {
          fetchNextPage().catch(showError);
        }
      },
      { rootMargin: "320px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  const categoryOptions = useMemo((): SelectOption<string>[] => {
    const list = (categories.data ?? [])
      .filter((c) => c.projectType === projectType)
      .map((c) => ({
        value: c.name,
        label: categoryLabel(t, c.name),
        hint: GROUP_KEYS[c.header] ? t(GROUP_KEYS[c.header] ?? "") : undefined,
        order: HEADER_ORDER.indexOf(c.header),
      }))
      .sort((a, b) => a.order - b.order || a.label.localeCompare(b.label, "tr"))
      .map(({ order: _order, ...option }) => option);
    return [{ value: "all", label: t("content.browse.allCategories") }, ...list];
  }, [categories.data, projectType, t]);

  const hits = search.data?.pages.flatMap((p) => p.hits) ?? [];
  const total = search.data?.pages[0]?.total ?? 0;
  const loaderName = instance.loader === "vanilla" ? "Minecraft" : t(`loaders.${instance.loader}`);
  const target =
    kind === "mod" ? `${loaderName} ${instance.mcVersion}` : `Minecraft ${instance.mcVersion}`;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {embedded ? (
        <p className="text-[12.5px] text-ash-400">{t("content.browse.compatible", { target })}</p>
      ) : (
        <div className="mt-5 flex items-center gap-3">
          <IconButton
            label={t(`content.browse.back.${kind}`)}
            icon={<ArrowLeft size={18} />}
            onClick={onBack}
            tooltipSide="right"
          />
          <div className="min-w-0">
            <h2 className="font-display text-[20px] leading-tight font-semibold text-bone-50">
              {t(`content.browse.title.${kind}`)}
            </h2>
            <p className="text-[12.5px] text-ash-400">
              {t("content.browse.compatible", { target })}
            </p>
          </div>
        </div>
      )}

      <div
        className={
          embedded
            ? "mt-3 flex flex-wrap items-center gap-2.5"
            : "mt-4 flex flex-wrap items-center gap-2.5"
        }
      >
        <SearchInput
          value={query}
          onChange={setQuery}
          placeholder={t(`content.browse.search.${kind}`)}
          label={t(`content.browse.search.${kind}`)}
          clearLabel={t("library.clearSearch")}
          className="min-w-[260px] flex-1"
        />
        <Select
          value={sort}
          onValueChange={setSort}
          label={t("content.browse.sort")}
          className="w-48"
          options={(["relevance", "downloads", "updated"] as const).map((value) => ({
            value,
            label: t(`content.browse.sorts.${value}`),
          }))}
        />
        <Select
          value={category}
          onValueChange={setCategory}
          label={t("content.browse.category")}
          className="w-52"
          options={categoryOptions}
          disabled={categoryOptions.length <= 1}
        />
      </div>

      <div className="-mx-2 mt-4 min-h-0 flex-1 overflow-y-auto px-2 pb-8">
        {search.isPending ? (
          <div className="space-y-1">
            {[0, 1, 2, 3, 4].map((i) => (
              <RowSkeleton key={i} />
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
            <p className="mb-1.5 px-3 text-[12px] text-ash-400 tabular-nums">
              {t("content.browse.results", { count: formatCount(total) })}
            </p>
            <ul className="space-y-0.5">
              {hits.map((hit) => {
                const entry = installed.get(hit.projectId);
                const key = projectJobKey(instance.id, hit.projectId);
                return (
                  <HitRow
                    key={hit.projectId}
                    hit={hit}
                    entry={entry}
                    job={jobs[key]}
                    onOpen={() => setSelected(hit)}
                    onInstall={() => void actions.install(hit.projectId, hit.title)}
                    onUpdate={() => entry && void actions.update([entry])}
                    onRemove={() => entry && void actions.remove(entry.fileName, hit.title)}
                    onCancel={() => actions.cancel(key)}
                  />
                );
              })}
            </ul>
            <div ref={sentinel} aria-hidden className="h-px" />
            {isFetchingNextPage ? (
              <div className="space-y-1">
                <RowSkeleton />
                <RowSkeleton />
              </div>
            ) : !hasNextPage ? (
              <p className="py-6 text-center text-[12.5px] text-ash-400">
                {t("content.browse.end")}
              </p>
            ) : null}
          </>
        )}
      </div>

      <ProjectDialog
        projectId={selected?.projectId ?? null}
        hint={selected ?? undefined}
        target={{ instanceId: instance.id, kind }}
        onClose={() => setSelected(null)}
      />
    </div>
  );
}
