import {
  ArrowsClockwise,
  ArrowsLeftRight,
  FilePlus,
  MagnifyingGlassPlus,
  Package,
  Plus,
  Trash,
} from "@phosphor-icons/react";
import { Button, Chip, cn, IconButton, SearchInput, toast } from "@pl/ui";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  bulkJobKey,
  projectJobKey,
  type RunningJob,
  showError,
  useContentActions,
  useContentJobs,
  useUndoableRemoval,
} from "@/features/content/jobs";
import { JobStatus } from "@/features/content/parts";
import { ProjectDialog } from "@/features/content/project-dialog";
import { useContentScan } from "@/features/content/use-content";
import { PageDropZone } from "@/features/drop/page-zone";
import { api, type InstalledContent, type InstanceDetail, type ModEntry } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { matchesQuery } from "@/lib/search";
import { TabEmpty } from "../parts";

/**
 * A vanilla instance's Modlar tab (and mod center): it cannot load mods; a new instance with a
 * loader can.
 */
export function VanillaModsEmpty() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  return (
    <TabEmpty
      icon={<Package size={26} weight="duotone" />}
      title={t("instance.mods.vanillaTitle")}
      body={t("instance.mods.vanillaBody")}
      action={
        <Button
          variant="secondary"
          icon={<Plus size={16} weight="bold" className="text-gold-300" />}
          onClick={() => void navigate({ to: "/library", search: { create: true } })}
        >
          {t("library.empty.action")}
        </Button>
      }
    />
  );
}

function ModToggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative h-[22px] w-[38px] shrink-0 cursor-pointer rounded-full ring-1 transition-colors duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
        checked ? "bg-gold-500 ring-gold-400/60" : "bg-obsidian-700 ring-white/10",
      )}
    >
      <span
        className={cn(
          "absolute top-[3px] left-[3px] size-4 rounded-full shadow transition-transform duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]",
          checked ? "translate-x-4 bg-gold-ink" : "bg-bone-100",
        )}
      />
    </button>
  );
}

function ModRow({
  mod,
  entry,
  job,
  onToggle,
  onOpen,
  onUpdate,
  onRemove,
  onCancel,
  onPackItem,
  busy,
}: {
  mod: ModEntry;
  entry?: InstalledContent;
  job?: RunningJob;
  onToggle: (enabled: boolean) => void;
  onOpen?: () => void;
  onUpdate: () => void;
  onRemove: () => void;
  onCancel: () => void;
  /** A recommended mod of the pack: its chip opens the mod center. */
  onPackItem?: () => void;
  /** A bulk update is running: single updates wait. */
  busy: boolean;
}) {
  const { t } = useTranslation();
  const fromPack = mod.fromPack || Boolean(entry?.fromPack);
  return (
    <li
      className={cn(
        "flex items-center gap-3.5 rounded-[var(--radius-panel)] px-3 py-2.5 transition-colors hover:bg-white/5",
        !mod.enabled && "opacity-55",
      )}
    >
      <span className="chamfer flex size-10 shrink-0 items-center justify-center overflow-hidden bg-obsidian-800 [--cut:6px]">
        {mod.iconUrl ? (
          <img
            src={mod.iconUrl}
            alt=""
            className="size-full object-cover [image-rendering:pixelated]"
          />
        ) : (
          <Package size={20} weight="duotone" className="text-gold-400/80" />
        )}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          {onOpen ? (
            <button
              type="button"
              onClick={onOpen}
              className="min-w-0 cursor-pointer truncate text-left text-[14px] font-semibold text-bone-50 hover:text-gold-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              {mod.name}
            </button>
          ) : (
            <span className="truncate text-[14px] font-semibold text-bone-50">{mod.name}</span>
          )}
          <span className="shrink-0 text-[12px] text-ash-300 tabular-nums">{mod.version}</span>
          {fromPack && onPackItem ? (
            <button
              type="button"
              onClick={onPackItem}
              aria-label={t("modCenter.fromMods.open")}
              className="group/pack shrink-0 cursor-pointer transition-transform duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97]"
            >
              <Chip className="transition-colors duration-150 group-hover/pack:bg-white/15">
                {t("modCenter.fromMods.chip")}
              </Chip>
            </button>
          ) : fromPack ? (
            <Chip className="shrink-0">{t("instance.mods.fromPack")}</Chip>
          ) : null}
          {entry?.update && !fromPack ? (
            <Chip tone="gold" className="shrink-0">
              {t("content.updates.newVersion", { version: entry.update.versionNumber })}
            </Chip>
          ) : null}
        </div>
        <p className="truncate text-[12.5px] text-ash-400">{mod.description}</p>
      </div>
      {job ? (
        <JobStatus job={job} onCancel={onCancel} />
      ) : entry?.update && !fromPack ? (
        <Button
          size="sm"
          variant="ghost"
          icon={<ArrowsClockwise size={15} className="text-gold-300" />}
          disabled={busy}
          onClick={onUpdate}
        >
          {t("content.actions.update")}
        </Button>
      ) : null}
      <span className="hidden w-20 shrink-0 text-right text-[12px] text-ash-300 tabular-nums lg:block">
        {formatBytes(mod.sizeBytes)}
      </span>
      {fromPack ? (
        <span aria-hidden className="size-8 shrink-0" />
      ) : (
        <IconButton
          label={t("content.actions.remove")}
          icon={<Trash size={16} />}
          size="sm"
          onClick={onRemove}
        />
      )}
      <ModToggle
        checked={mod.enabled}
        onChange={onToggle}
        label={t("instance.mods.toggle", { name: mod.name })}
      />
    </li>
  );
}

export function ModsTab({
  instance,
  onBrowse,
  ownOnly = false,
  onMigrate,
  onOpenModCenter,
}: {
  instance: InstanceDetail;
  onBrowse: () => void;
  /** Only the player's own mods ("Kendi modların" in the mod center). */
  ownOnly?: boolean;
  /** "Başka sürüme taşı". */
  onMigrate?: () => void;
  /** Opens the mod center (recommended mods of the pack). */
  onOpenModCenter?: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [query, setQuery] = useState("");
  const [opened, setOpened] = useState<{ projectId: string; mod: ModEntry } | null>(null);
  const modded = instance.loader !== "vanilla";

  // Recommended mods of the pack switch through the mod center selection (the backend routes
  // them); other mods are renamed to `.disabled`.
  const toggle = useMutation({
    mutationFn: ({ mod, enabled }: { mod: ModEntry; enabled: boolean }) =>
      api.setModEnabled(instance.id, mod.fileName, enabled),
    onMutate: async ({ mod, enabled }) => {
      queryClient.setQueryData<InstanceDetail>(["instance", instance.id], (prev) =>
        prev
          ? {
              ...prev,
              mods: prev.mods.map((m) => (m.fileName === mod.fileName ? { ...m, enabled } : m)),
            }
          : prev,
      );
    },
    onError: showError,
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: ["instance", instance.id] });
      void queryClient.invalidateQueries({ queryKey: ["mod-center", instance.id] });
    },
  });

  // Update info comes from an explicit check (or from the browser, which shares the query).
  const scan = useContentScan(instance.id, "mod", false);
  const actions = useContentActions(instance.id, "mod");
  // A removed mod hides at once and goes only when its "Geri al" toast ends (as packs do).
  const removal = useUndoableRemoval(instance.id, "mod");
  const jobs = useContentJobs((s) => s.jobs);
  // Files dropped onto the window are sorted by the window-wide drop layer (features/drop); an
  // empty list shows its "Buraya bırak" zone in place.

  const entries = useMemo(() => {
    const map = new Map<string, InstalledContent>();
    for (const entry of scan.data?.entries ?? []) {
      map.set(entry.fileName.replace(/\.disabled$/, ""), entry);
    }
    return map;
  }, [scan.data]);
  const entryOf = (mod: ModEntry) => entries.get(mod.fileName.replace(/\.disabled$/, ""));
  const bulkKey = bulkJobKey(instance.id, "mod");
  const bulkJob = jobs[bulkKey];

  const checkUpdates = async () => {
    const result = await scan.refetch();
    if (result.error) {
      showError(result.error);
    } else if (result.data?.offline) {
      toast(t("content.toasts.updatesOffline"));
    } else if (!result.data?.entries.some((e) => e.update && !e.fromPack)) {
      toast(t("content.toasts.noUpdates"));
    }
  };

  const { isHidden } = removal;
  // Every mod of the list (own ones only in the mod center), then the ones the search keeps.
  const listed = useMemo(
    () =>
      instance.mods
        .filter(
          (m) =>
            !ownOnly ||
            !(m.fromPack || entries.get(m.fileName.replace(/\.disabled$/, ""))?.fromPack),
        )
        .filter((m) => !isHidden(m.fileName))
        .sort((a, b) => a.name.localeCompare(b.name, "tr")),
    [instance.mods, ownOnly, entries, isHidden],
  );
  const mods = useMemo(
    () => (query.trim() ? listed.filter((m) => matchesQuery(m.name, query)) : listed),
    [listed, query],
  );
  // Updates of the mods still listed: the update check is not run again after a removal, and a
  // removed mod (also one waiting for its "Geri al" toast) has nothing left to update.
  const updates = useMemo(() => {
    const present = new Set(listed.map((m) => m.fileName.replace(/\.disabled$/, "")));
    return (scan.data?.entries ?? []).filter(
      (e) => e.update && !e.fromPack && present.has(e.fileName.replace(/\.disabled$/, "")),
    );
  }, [scan.data, listed]);
  // The list that takes the empty zone's place (mods were just dropped or added) rises in.
  const wasEmpty = useRef(listed.length === 0);
  const arriving = wasEmpty.current && listed.length > 0;
  useEffect(() => {
    wasEmpty.current = listed.length === 0;
  });

  if (!modded) return <VanillaModsEmpty />;

  const browseButton = (
    <Button
      variant="secondary"
      icon={<MagnifyingGlassPlus size={17} className="text-gold-300" />}
      onClick={onBrowse}
    >
      {t("instance.mods.browse")}
    </Button>
  );

  return (
    <div className="relative max-w-[980px]">
      <div className="mb-4 flex items-center gap-2.5">
        <SearchInput
          value={query}
          onChange={setQuery}
          placeholder={t("instance.mods.search")}
          label={t("instance.mods.search")}
          clearLabel={t("library.clearSearch")}
          className="w-72"
        />
        <span className="flex-1" />
        <Button
          variant="ghost"
          icon={<FilePlus size={17} />}
          onClick={() => void actions.addFiles()}
        >
          {t("instance.mods.addFile")}
        </Button>
        <Button
          variant="ghost"
          icon={<ArrowsClockwise size={17} />}
          disabled={scan.isFetching || instance.mods.length === 0}
          onClick={() => void checkUpdates()}
        >
          {scan.isFetching ? t("content.updates.checking") : t("content.updates.check")}
        </Button>
        {onMigrate ? (
          <Button
            variant="ghost"
            icon={<ArrowsLeftRight size={17} />}
            disabled={listed.length === 0}
            onClick={onMigrate}
          >
            {t("modCenter.own.migrate")}
          </Button>
        ) : null}
        {browseButton}
      </div>

      {updates.length > 0 || bulkJob ? (
        <div className="glass mb-3 flex items-center gap-3 rounded-[var(--radius-panel)] px-4 py-2.5">
          <ArrowsClockwise size={18} className="shrink-0 text-gold-300" />
          <p className="flex-1 text-[13.5px] text-bone-100">
            {t("content.updates.available", { count: updates.length })}
          </p>
          {bulkJob ? (
            <JobStatus job={bulkJob} onCancel={() => actions.cancel(bulkKey)} />
          ) : (
            <Button size="sm" variant="secondary" onClick={() => void actions.update(updates)}>
              {t("content.updates.updateAll")}
            </Button>
          )}
        </div>
      ) : null}

      {listed.length === 0 ? (
        <PageDropZone
          kind="mod"
          label={ownOnly ? t("modCenter.own.emptyTitle") : t("instance.mods.emptyTitle")}
          actions={browseButton}
          className="-mt-1"
        />
      ) : mods.length === 0 ? (
        // The search hides every mod: say so (the mods are still there) and offer the way back.
        <div className="flex flex-col items-center gap-3 py-10 text-center">
          <p className="text-[13.5px] text-ash-300">{t("modCenter.noMatch")}</p>
          <Button variant="ghost" size="sm" onClick={() => setQuery("")}>
            {t("library.clearSearch")}
          </Button>
        </div>
      ) : (
        <ul className={cn("space-y-0.5", arriving && "panel-in")}>
          {mods.map((mod) => {
            const entry = entryOf(mod);
            const projectId = mod.projectId ?? entry?.projectId;
            const key = projectJobKey(instance.id, projectId ?? mod.fileName);
            return (
              <ModRow
                key={mod.fileName}
                mod={mod}
                entry={entry}
                job={jobs[key]}
                onToggle={(enabled) => toggle.mutate({ mod, enabled })}
                onOpen={projectId ? () => setOpened({ projectId, mod }) : undefined}
                onUpdate={() => entry && void actions.update([entry])}
                onRemove={() => removal.remove(mod.fileName, mod.name)}
                onCancel={() => actions.cancel(key)}
                onPackItem={mod.itemId && onOpenModCenter ? onOpenModCenter : undefined}
                busy={Boolean(bulkJob)}
              />
            );
          })}
        </ul>
      )}

      <ProjectDialog
        projectId={opened?.projectId ?? null}
        hint={
          opened
            ? {
                title: opened.mod.name,
                author: opened.mod.authors[0] ?? "",
                iconUrl: undefined,
                description: opened.mod.description,
              }
            : undefined
        }
        target={{ instanceId: instance.id, kind: "mod" }}
        onClose={() => setOpened(null)}
      />
    </div>
  );
}
