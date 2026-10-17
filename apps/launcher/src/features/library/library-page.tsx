import { CaretDown, DownloadSimple, Plus } from "@phosphor-icons/react";
import { Button, cn, Dialog, EmptyState, SearchInput, Select, Skeleton, toast } from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { AnimatePresence } from "motion/react";
import { useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import emptyArt from "@/assets/art/empty-library.webp";
import { ExportDialog } from "@/features/content/export-dialog";
import { showError } from "@/features/content/jobs";
import { CardsDropGuide } from "@/features/drop/card-zone";
import { api, type InstanceSummary } from "@/lib/api";
import { matchesQuery } from "@/lib/search";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { CreateInstanceDialog } from "./create-instance-dialog";
import { GroupDialog } from "./group-dialog";
import { type InstanceActions, InstanceCard } from "./instance-card";
import { type LibrarySort, sortInstances, useLibraryView } from "./library-view";
import { createDesktopShortcut } from "./shortcut";

function Section({
  title,
  items,
  selectedId,
  actions,
  offset,
  collapsed,
  onToggle,
  meta,
}: {
  title: string;
  items: InstanceSummary[];
  selectedId: string | undefined;
  actions: InstanceActions;
  offset: number;
  collapsed: boolean;
  onToggle: () => void;
  meta: "playTime" | "lastPlayed";
}) {
  const panelId = useId();
  // A section the player opens rises in (like a tab panel); on arrival it is just there.
  const opened = useRef(false);
  if (!items.length) return null;
  return (
    <section className="mb-5">
      <h2 className="mb-2.5">
        <button
          type="button"
          aria-expanded={!collapsed}
          aria-controls={collapsed ? undefined : panelId}
          onClick={() => {
            opened.current = collapsed;
            onToggle();
          }}
          className="flex cursor-pointer items-center gap-2 rounded-md px-1 py-0.5 font-display text-[15px] font-semibold text-ash-300 transition-colors hover:text-bone-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          <CaretDown
            size={14}
            weight="bold"
            className={cn("transition-transform duration-200", collapsed && "-rotate-90")}
          />
          {title}
          <span className="font-sans text-[12px] font-medium text-ash-400 tabular-nums">
            {items.length}
          </span>
        </button>
      </h2>
      {collapsed ? null : (
        <div
          id={panelId}
          className={cn(
            "grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3",
            opened.current && "panel-in",
          )}
        >
          {/* A deleted card fades out in place while the others glide over (popLayout). */}
          <AnimatePresence initial={false} mode="popLayout">
            {items.map((instance, i) => (
              <InstanceCard
                key={instance.id}
                instance={instance}
                selected={instance.id === selectedId}
                actions={actions}
                index={offset + i}
                meta={meta}
              />
            ))}
          </AnimatePresence>
        </div>
      )}
    </section>
  );
}

const sortOptions: LibrarySort[] = ["lastPlayed", "name", "playTime", "created"];

export function LibraryPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const search = useSearch({ strict: false }) as { create?: boolean; source?: "import" };
  const [query, setQuery] = useState("");
  const [pendingDelete, setPendingDelete] = useState<InstanceSummary | null>(null);
  const [exporting, setExporting] = useState<InstanceSummary | null>(null);
  const [grouping, setGrouping] = useState<InstanceSummary | null>(null);
  const {
    data: instances,
    isPending,
    isError,
    error,
    refetch,
  } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const selectedId = usePrefs((s) => s.selectedInstanceId) ?? instances?.[0]?.id;
  const selectInstance = usePrefs((s) => s.selectInstance);
  const play = useLaunch((s) => s.play);
  const sort = useLibraryView((s) => s.sort);
  const setSort = useLibraryView((s) => s.setSort);
  const collapsed = useLibraryView((s) => s.collapsed);
  const toggleSection = useLibraryView((s) => s.toggleSection);

  const createOpen = Boolean(search.create);
  // "İçe aktar" opens the same dialog on its file import.
  const setCreateOpen = (open: boolean, source?: "import") =>
    void navigate({ to: "/library", search: open ? { create: true, source } : {} });

  const filtered = useMemo(() => {
    const matches = (instances ?? []).filter((i) =>
      // The name, the version, the group, and the loader's name ("fabric" finds Fabric ones).
      matchesQuery([i.name, i.mcVersion, t(`loaders.${i.loader}`), i.group ?? ""].join(" "), query),
    );
    return sortInstances(matches, sort);
  }, [instances, query, sort, t]);

  // Existing group names (case-insensitively unique), alphabetical.
  const groupNames = useMemo(() => {
    const seen = new Map<string, string>();
    for (const i of instances ?? []) {
      if (!i.managed && i.group) seen.set(i.group.toLocaleLowerCase("tr"), i.group);
    }
    return [...seen.values()].sort((a, b) => a.localeCompare(b, "tr"));
  }, [instances]);

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ["instances"] });
  const duplicate = useMutation({
    mutationFn: (i: InstanceSummary) => api.duplicateInstance(i.id),
    onSuccess: (copy) => {
      invalidate();
      toast.success(t("library.duplicated", { name: copy.name }));
    },
    onError: (error) => showError(error),
  });
  // A deleted instance waits in the launcher trash: the toast puts it back in one click.
  const undoDelete = async (trashId: string) => {
    try {
      const back = await api.restoreInstance(trashId);
      invalidate();
      toast.success(t("instance.trash.restored", { name: back.name }));
    } catch (error) {
      showError(error);
    }
  };
  const remove = useMutation({
    mutationFn: (i: InstanceSummary) => api.deleteInstance(i.id),
    onSuccess: (trashId, i) => {
      invalidate();
      void queryClient.invalidateQueries({ queryKey: ["cleanup-trash"] });
      toast(t("library.deleted", { name: i.name }), {
        duration: 9000,
        action: { label: t("instance.trash.undo"), onClick: () => void undoDelete(trashId) },
      });
    },
    onError: (error) => showError(error),
  });
  const moveToGroup = useMutation({
    mutationFn: ({ instance, group }: { instance: InstanceSummary; group: string | null }) =>
      api.library.setGroup(instance.id, group),
    onMutate: ({ instance, group }) => {
      queryClient.setQueryData<InstanceSummary[]>(["instances"], (prev) =>
        prev?.map((i) => (i.id === instance.id ? { ...i, group: group ?? undefined } : i)),
      );
    },
    onSuccess: (_, { instance, group }) => {
      toast.success(
        group
          ? t("library.groups.moved", { name: instance.name, group })
          : t("library.groups.removed", { name: instance.name }),
      );
    },
    onError: (err) => showError(err),
    onSettled: invalidate,
  });

  const actions: InstanceActions = {
    play: (i) => {
      selectInstance(i.id);
      void play(i.id);
    },
    openFolder: (i) => void api.openInstanceFolder(i.id),
    duplicate: (i) => duplicate.mutate(i),
    exportPack: (i) => setExporting(i),
    group: (i) => setGrouping(i),
    shortcut: (i) => void createDesktopShortcut(i),
    remove: (i) => setPendingDelete(i),
  };

  // Official packs first, then the player's groups (A to Z), then everything without a group.
  const searching = query.trim().length > 0;
  const official = filtered.filter((i) => i.managed);
  const mine = filtered.filter((i) => !i.managed);
  const groups = groupNames
    .map((name) => ({
      key: `group:${name.toLocaleLowerCase("tr")}`,
      title: name,
      items: mine.filter((i) => i.group?.toLocaleLowerCase("tr") === name.toLocaleLowerCase("tr")),
    }))
    .filter((g) => g.items.length > 0);
  const ungrouped = mine.filter((i) => !i.group);
  const sections = [
    { key: "official", title: t("library.official"), items: official },
    ...groups,
    {
      key: "ungrouped",
      title: groupNames.length > 0 ? t("library.groups.none") : t("library.mine"),
      items: ungrouped,
    },
  ];
  let offset = 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col px-page pt-3">
      <header className="flex flex-wrap items-end justify-between gap-3 pb-4">
        <div>
          <h1 className="page-title">{t("library.title")}</h1>
          {/* While files are dragged over the cards it says how to drop them. */}
          <CardsDropGuide
            rest={instances ? t("library.count", { count: instances.length }) : t("common.loading")}
            className="mt-0.5 text-[13px] text-ash-300"
          />
        </div>
        <div className="flex flex-wrap items-center gap-2.5">
          <SearchInput
            value={query}
            onChange={setQuery}
            placeholder={t("library.search")}
            label={t("library.search")}
            clearLabel={t("library.clearSearch")}
            className="w-60"
          />
          <Select
            value={sort}
            onValueChange={setSort}
            label={t("library.sort.label")}
            options={sortOptions.map((value) => ({ value, label: t(`library.sort.${value}`) }))}
            className="w-48"
          />
          <Button
            variant="ghost"
            icon={<DownloadSimple size={17} />}
            onClick={() => setCreateOpen(true, "import")}
          >
            {t("library.import")}
          </Button>
          <Button
            variant="secondary"
            icon={<Plus size={17} weight="bold" className="text-gold-300" />}
            onClick={() => setCreateOpen(true)}
          >
            {t("library.create")}
          </Button>
        </div>
      </header>

      {/* Cards scrolled out of this area are not under the cursor while files are dragged. */}
      <div data-drop-clip className="-mx-2 min-h-0 flex-1 overflow-y-auto px-2 pb-5">
        {isPending ? (
          // A section title and cards, as the library will look.
          <div aria-busy>
            <Skeleton className="mb-2.5 h-[24px] w-36" />
            <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3">
              {[0, 1, 2, 3].map((i) => (
                <Skeleton key={i} className="h-[168px] rounded-[var(--radius-panel)]" />
              ))}
            </div>
          </div>
        ) : isError ? (
          <EmptyState
            illustration={emptyArt}
            title={t("library.loadFailed")}
            body={error instanceof Error ? error.message : undefined}
            action={
              <Button variant="secondary" onClick={() => void refetch()}>
                {t("common.retry")}
              </Button>
            }
          />
        ) : instances && instances.length === 0 ? (
          <EmptyState
            illustration={emptyArt}
            title={t("library.empty.title")}
            body={t("library.empty.body")}
            action={
              <Button
                variant="primary"
                icon={<Plus size={16} weight="bold" />}
                onClick={() => setCreateOpen(true)}
              >
                {t("library.empty.action")}
              </Button>
            }
          />
        ) : filtered.length === 0 ? (
          <div className="flex flex-col items-center gap-3 pt-10 text-center">
            <p className="text-ash-400">{t("library.noResults")}</p>
            <Button variant="ghost" size="sm" onClick={() => setQuery("")}>
              {t("library.clearSearch")}
            </Button>
          </div>
        ) : (
          sections.map((section) => {
            const start = offset;
            offset += section.items.length;
            return (
              <Section
                key={section.key}
                title={section.title}
                items={section.items}
                selectedId={selectedId}
                actions={actions}
                offset={start}
                collapsed={!searching && collapsed.includes(section.key)}
                onToggle={() => toggleSection(section.key)}
                meta={sort === "playTime" ? "playTime" : "lastPlayed"}
              />
            );
          })
        )}
      </div>

      <CreateInstanceDialog
        open={createOpen}
        initialSource={search.source}
        onOpenChange={(open) => setCreateOpen(open)}
      />
      <ExportDialog instance={exporting} onClose={() => setExporting(null)} />
      <GroupDialog
        instance={grouping}
        groups={groupNames}
        onClose={() => setGrouping(null)}
        onSave={(instance, group) => moveToGroup.mutate({ instance, group })}
      />
      <Dialog
        open={Boolean(pendingDelete)}
        onOpenChange={(open) => !open && setPendingDelete(null)}
        title={t("library.deleteConfirm.title", { name: pendingDelete?.name ?? "" })}
        description={t("library.deleteConfirm.body")}
        closeLabel={t("common.close")}
        width={440}
        footer={
          <>
            <Button variant="ghost" onClick={() => setPendingDelete(null)}>
              {t("create.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                if (pendingDelete) remove.mutate(pendingDelete);
                setPendingDelete(null);
              }}
            >
              {t("library.deleteConfirm.confirm")}
            </Button>
          </>
        }
      />
    </div>
  );
}
