import {
  Archive,
  CaretDown,
  ClockCounterClockwise,
  DotsThreeVertical,
  FolderOpen,
  LockSimple,
  Play,
  Stack,
  Trash,
  X,
} from "@phosphor-icons/react";
import {
  Button,
  Chip,
  ContentIcon,
  ContextMenu,
  cn,
  easeOutExpo,
  IconButton,
  Menu,
  type MenuItem,
  Progress,
  Skeleton,
  toast,
  useMotionPrefs,
  usePressGuard,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails, newJobId, showError } from "@/features/content/jobs";
import { useWorldShare, WorldShareAction, WorldShareChip } from "@/features/host/world-row";
import {
  api,
  type InstanceDetail,
  type TrashItem,
  type WorldBackup,
  type WorldDetail,
} from "@/lib/api";
import { formatBytes, formatPercent, formatRelative } from "@/lib/format";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { ConfirmDialog, InlineNotice, TabEmpty, TabSection, TrashRow } from "../parts";
import { canQuickPlay } from "../quick-play";
import { trashLabel } from "../trash-label";
import { useGameRunning, useOwnGameOf } from "../use-game-running";
import { formatWhen } from "../when";
import {
  backupJobKey,
  jobPercent,
  restoreJobKey,
  useWorldJobs,
  type WorldJob,
} from "../world-jobs";
import { DatapacksDialog } from "./datapacks-dialog";

/** Progress of a backup or restore, with a cancel button. */
function JobProgress({
  job,
  label,
  onCancel,
}: {
  job: WorldJob;
  label: string;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const percent = jobPercent(job.progress);
  // The bar takes the place of "Yedekle" or "Geri yükle" and its × sits at that button's end:
  // the rest of the double click that started the job never cancels it (@pl/ui press-guard).
  const [shownAt] = useState(() => performance.now());
  const guard = usePressGuard(job.jobId, { since: shownAt });
  return (
    <div className="flex w-60 shrink-0 items-center gap-2">
      <div className="min-w-0 flex-1">
        <div className="mb-1 flex justify-between text-[12px] text-ash-300">
          <span>{label}</span>
          <span className="tabular-nums">
            {percent === undefined ? "" : formatPercent(percent)}
          </span>
        </div>
        <Progress value={percent} label={label} />
      </div>
      <IconButton
        label={t("instance.worlds.cancelJob")}
        icon={<X size={15} />}
        size="sm"
        onClick={(event) => {
          if (guard.take(event)) onCancel();
        }}
        {...guard.handlers}
      />
    </div>
  );
}

function WorldIcon({ world }: { world: WorldDetail }) {
  return (
    <span className="chamfer flex size-12 shrink-0 items-center justify-center overflow-hidden bg-obsidian-800 [--cut:6px]">
      {world.iconUrl ? (
        <img
          src={world.iconUrl}
          alt=""
          className="size-full object-cover [image-rendering:pixelated]"
          draggable={false}
        />
      ) : (
        <ContentIcon kind="world" quiet />
      )}
    </span>
  );
}

function ModeChip({ world }: { world: WorldDetail }) {
  const { t } = useTranslation();
  if (world.hardcore) return <Chip tone="flare">{t("instance.worlds.modes.hardcore")}</Chip>;
  if (world.gameMode === "unknown") return null;
  return <Chip>{t(`instance.worlds.modes.${world.gameMode}`)}</Chip>;
}

function BackupRow({
  backup,
  job,
  busy,
  locked,
  showWorld,
  onRestore,
  onDelete,
  onReveal,
  onCancel,
}: {
  backup: WorldBackup;
  job?: WorldJob;
  busy: boolean;
  locked: boolean;
  showWorld: boolean;
  onRestore: () => void;
  onDelete: () => void;
  onReveal: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const title = showWorld
    ? (backup.worldName ??
      backup.worldFolder ??
      trashLabel({ kind: "backup", name: backup.fileName, fileName: backup.fileName }))
    : formatWhen(backup.createdAt) || t("instance.worlds.backupUntitled");
  const items: Array<MenuItem | null> = [
    {
      label: t("instance.worlds.revealBackup"),
      icon: <FolderOpen size={16} />,
      onSelect: onReveal,
    },
    null,
    {
      label: t("instance.worlds.deleteBackup"),
      icon: <Trash size={16} />,
      danger: true,
      disabled: Boolean(job) || busy,
      onSelect: onDelete,
    },
  ];
  // The right-click menu also offers "Geri yükle", the row's own button.
  const contextItems: Array<MenuItem | null> = job
    ? items
    : [
        {
          label: t("instance.worlds.restore"),
          icon: <ClockCounterClockwise size={16} />,
          disabled: locked || busy || !backup.valid,
          onSelect: onRestore,
        },
        null,
        ...items,
      ];
  return (
    <ContextMenu items={contextItems}>
      <li className="flex items-center gap-3 rounded-md px-3 py-2 transition-colors hover:bg-white/4">
        <Archive size={18} className="shrink-0 text-ash-400" />
        <div className="min-w-0 flex-1">
          <p className="truncate text-[13px] font-medium text-bone-100">{title}</p>
          <p className="flex gap-3 text-[12px] text-ash-300 tabular-nums">
            {showWorld && backup.createdAt ? <span>{formatWhen(backup.createdAt)}</span> : null}
            <span>{formatBytes(backup.sizeBytes)}</span>
            {/* HOST1: the launcher took it before the world first opened to friends. */}
            {backup.reason === "beforeSharing" ? (
              <span className="min-w-0 truncate">{t("small9.backups.beforeSharing")}</span>
            ) : null}
            {backup.valid ? null : (
              <span className="text-flare-400">{t("instance.worlds.backupInvalid")}</span>
            )}
          </p>
        </div>
        <div className="flex w-60 shrink-0 justify-end">
          {job ? (
            <JobProgress job={job} label={t("instance.worlds.restoring")} onCancel={onCancel} />
          ) : (
            <Button
              size="sm"
              variant="ghost"
              icon={<ClockCounterClockwise size={15} />}
              disabled={locked || busy || !backup.valid}
              onClick={onRestore}
            >
              {t("instance.worlds.restore")}
            </Button>
          )}
        </div>
        <Menu
          trigger={
            <IconButton
              label={t("instance.worlds.more", { name: title })}
              icon={<DotsThreeVertical size={18} weight="bold" />}
              size="sm"
            />
          }
          items={items}
        />
      </li>
    </ContextMenu>
  );
}

function WorldsSkeleton() {
  return (
    <ul className="max-w-[980px] space-y-1" aria-hidden>
      {[0, 1, 2].map((i) => (
        <li key={i} className="flex items-center gap-4 px-3 py-3">
          <Skeleton className="size-12" />
          <div className="flex-1 space-y-2">
            <Skeleton className="h-4 w-48" />
            <Skeleton className="h-3 w-64" />
          </div>
          <Skeleton className="h-8 w-24" />
        </li>
      ))}
    </ul>
  );
}

/**
 * Worlds of an instance: size, last played, game mode and icon; backups (zip in `backups/`)
 * with progress, restore and delete; deleted worlds wait in the launcher trash. Nothing is
 * touched while the instance's game runs.
 */
export function WorldsTab({ instance }: { instance: InstanceDetail }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const locked = useGameRunning(instance.id);
  // A world's "Oyna" stays on while only another account plays this instance (PLAY-AGAIN): this
  // account's game starts beside it (a world open there is refused before anything loads).
  const playLocked = useOwnGameOf(instance.id);
  const id = instance.id;
  const worlds = useQuery({ queryKey: ["worlds", id], queryFn: () => api.library.worlds(id) });
  const backups = useQuery({
    queryKey: ["world-backups", id],
    queryFn: () => api.library.backups(id),
  });
  const trash = useQuery({ queryKey: ["trash", id], queryFn: () => api.library.trash(id) });
  const jobs = useWorldJobs((s) => s.jobs);
  const setJob = useWorldJobs((s) => s.setJob);
  const play = useLaunch((s) => s.play);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const quickPlay = canQuickPlay(instance.mcVersion);
  // "Arkadaşlarınla oyna" (HOST1): only where the instance may share its worlds.
  const share = useWorldShare(instance, locked);
  // Rows glide when a world or its backups come and go (not in Performance mode).
  const glides = useMotionPrefs().layout;
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [deleting, setDeleting] = useState<WorldDetail | null>(null);
  const [datapacksFor, setDatapacksFor] = useState<WorldDetail | null>(null);
  const [restoring, setRestoring] = useState<WorldBackup | null>(null);
  const [deletingBackup, setDeletingBackup] = useState<WorldBackup | null>(null);
  const [purging, setPurging] = useState<TrashItem | null>(null);

  const refresh = () => {
    for (const key of ["worlds", "world-backups", "trash", "instance"]) {
      void queryClient.invalidateQueries({ queryKey: [key, id] });
    }
  };

  // The game changes worlds while it runs: read them again once it closes.
  useEffect(() => {
    if (!locked) void queryClient.invalidateQueries({ queryKey: ["worlds", id] });
  }, [locked, id, queryClient]);

  const worldList = worlds.data ?? [];
  const folders = useMemo(() => new Set(worldList.map((w) => w.folder)), [worldList]);
  const backupsByWorld = useMemo(() => {
    const map = new Map<string, WorldBackup[]>();
    for (const b of backups.data ?? []) {
      if (!b.worldFolder || !folders.has(b.worldFolder)) continue;
      const list = map.get(b.worldFolder);
      if (list) list.push(b);
      else map.set(b.worldFolder, [b]);
    }
    return map;
  }, [backups.data, folders]);
  const otherBackups = (backups.data ?? []).filter(
    (b) => !b.worldFolder || !folders.has(b.worldFolder),
  );
  // Worlds and backups only: screenshots have their own tab; mod sets that "Yeniden yükle" set
  // aside are not shown here.
  const trashItems = (trash.data ?? []).filter(
    (item) => item.kind === "world" || item.kind === "backup",
  );
  // A backup or restore runs, or an opening's backup (HOST1): one job at a time here.
  const busy = Object.keys(jobs).some((key) => key.startsWith(`${id}:`)) || share.busy;
  const totalBytes = worldList.reduce((sum, w) => sum + w.sizeBytes, 0);

  const toggle = (folder: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(folder)) next.delete(folder);
      else next.add(folder);
      return next;
    });

  const undo = async (item: TrashItem) => {
    try {
      await api.library.restoreFromTrash(id, item.id);
      toast.success(t("instance.trash.restored", { name: trashLabel(item) }));
    } catch (error) {
      showError(error);
    } finally {
      refresh();
    }
  };

  const backup = async (world: WorldDetail) => {
    const key = backupJobKey(id, world.folder);
    const jobId = newJobId();
    setJob(key, { jobId });
    try {
      const made = await api.library.backupWorld(id, world.folder, jobId, (progress) =>
        setJob(key, { jobId, progress }),
      );
      setExpanded((prev) => new Set(prev).add(world.folder));
      toast.success(t("instance.worlds.backedUp", { name: world.name }), {
        action: {
          label: t("instance.worlds.revealBackup"),
          onClick: () => void api.library.revealFile(id, "backups", made.fileName).catch(showError),
        },
      });
    } catch (error) {
      showError(error);
    } finally {
      setJob(key, undefined);
      refresh();
    }
  };

  const restore = async (item: WorldBackup, replace: boolean) => {
    const key = restoreJobKey(id, item.fileName);
    const jobId = newJobId();
    setJob(key, { jobId });
    try {
      const restored = await api.library.restoreBackup(
        id,
        item.fileName,
        replace,
        jobId,
        (progress) => setJob(key, { jobId, progress }),
      );
      toast.success(
        restored.replaced
          ? t("instance.worlds.restoredReplaced", { name: item.worldName ?? restored.folder })
          : t("instance.worlds.restored", { folder: restored.folder }),
      );
    } catch (error) {
      showError(error);
    } finally {
      setJob(key, undefined);
      refresh();
    }
  };

  const removeWorld = async (world: WorldDetail) => {
    try {
      const item = await api.library.deleteWorld(id, world.folder);
      toast(t("instance.worlds.deleted", { name: world.name }), {
        action: { label: t("instance.trash.undo"), onClick: () => void undo(item) },
      });
    } catch (error) {
      showError(error);
    } finally {
      refresh();
    }
  };

  const removeBackup = async (item: WorldBackup) => {
    try {
      const trashed = await api.library.deleteBackup(id, item.fileName);
      toast(t("instance.worlds.backupDeleted"), {
        action: { label: t("instance.trash.undo"), onClick: () => void undo(trashed) },
      });
    } catch (error) {
      showError(error);
    } finally {
      refresh();
    }
  };

  const purge = async (item: TrashItem) => {
    try {
      await api.library.deleteFromTrash(id, item.id);
      toast(t("instance.trash.purged", { name: trashLabel(item) }));
    } catch (error) {
      showError(error);
    } finally {
      refresh();
    }
  };

  const cancel = (key: string) => {
    const job = jobs[key];
    if (job) void api.library.cancelJob(job.jobId);
  };

  /**
   * A world's actions, for its "more" menu and its right-click menu (which also offers
   * "Yedekle", the row's own button).
   */
  const worldItems = (world: WorldDetail, withBackup: boolean): Array<MenuItem | null> => {
    const shareItem = share.menuItem(world);
    return [
      ...(withBackup
        ? [
            {
              label: t("instance.worlds.backup"),
              icon: <Archive size={16} />,
              disabled: locked || busy,
              onSelect: () => void backup(world),
            },
          ]
        : []),
      ...(quickPlay
        ? [
            {
              label: t("instance.worlds.play"),
              icon: <Play size={16} />,
              disabled: playLocked || !world.readable,
              onSelect: () => {
                selectInstance(id);
                void play(id, { quickPlay: { kind: "singleplayer", world: world.folder } });
              },
            },
          ]
        : []),
      ...(shareItem ? [shareItem] : []),
      ...(withBackup || quickPlay || shareItem ? [null] : []),
      {
        label: t("datapacks.menu"),
        icon: <Stack size={16} />,
        disabled: !world.readable,
        onSelect: () => setDatapacksFor(world),
      },
      {
        label: t("instance.worlds.openWorldFolder"),
        icon: <FolderOpen size={16} />,
        onSelect: () => void api.openInstanceFolder(id, `saves/${world.folder}`).catch(showError),
      },
      null,
      {
        label: t("instance.worlds.delete"),
        icon: <Trash size={16} />,
        danger: true,
        disabled: locked || busy,
        onSelect: () => setDeleting(world),
      },
    ];
  };

  const backupRow = (item: WorldBackup, showWorld: boolean) => (
    <BackupRow
      key={item.fileName}
      backup={item}
      job={jobs[restoreJobKey(id, item.fileName)]}
      busy={busy}
      locked={locked}
      showWorld={showWorld}
      onRestore={() => setRestoring(item)}
      onDelete={() => setDeletingBackup(item)}
      onReveal={() => void api.library.revealFile(id, "backups", item.fileName).catch(showError)}
      onCancel={() => cancel(restoreJobKey(id, item.fileName))}
    />
  );

  if (worlds.isPending) return <WorldsSkeleton />;
  if (worlds.isError) {
    return (
      <TabEmpty
        icon={<ContentIcon kind="failed" />}
        title={t("instance.worlds.listFailed")}
        body={worlds.error instanceof Error ? worlds.error.message : undefined}
        action={
          <>
            <Button variant="ghost" onClick={() => void copyErrorDetails(worlds.error)}>
              {t("common.copyDetails")}
            </Button>
            <Button variant="secondary" onClick={() => void worlds.refetch()}>
              {t("common.retry")}
            </Button>
          </>
        }
      />
    );
  }

  const restoringExists = restoring?.worldFolder ? folders.has(restoring.worldFolder) : false;

  return (
    <div className="max-w-[980px] space-y-6">
      <div className="flex flex-wrap items-center gap-3">
        <p className="flex-1 text-[13px] text-ash-400 tabular-nums">
          {worldList.length > 0
            ? t("instance.worlds.summary", {
                count: worldList.length,
                size: formatBytes(totalBytes),
              })
            : null}
        </p>
        <Button
          variant="ghost"
          icon={<Archive size={16} />}
          onClick={() => void api.openInstanceFolder(id, "backups").catch(showError)}
        >
          {t("instance.worlds.openBackups")}
        </Button>
        <Button
          variant="ghost"
          icon={<FolderOpen size={16} />}
          onClick={() => void api.openInstanceFolder(id, "saves").catch(showError)}
        >
          {t("instance.worlds.openFolder")}
        </Button>
      </div>

      {locked ? (
        <InlineNotice tone="warn" icon={<LockSimple size={16} weight="bold" />}>
          {t("instance.worlds.locked")}
        </InlineNotice>
      ) : null}

      {worldList.length === 0 ? (
        <TabEmpty
          icon={<ContentIcon kind="world" />}
          title={t("instance.worlds.emptyTitle")}
          body={t("instance.worlds.emptyBody")}
          className="pt-4"
        />
      ) : (
        <ul className="relative space-y-1">
          <AnimatePresence initial={false} mode="popLayout">
            {worldList.map((world) => {
              const own = backupsByWorld.get(world.folder) ?? [];
              const open = expanded.has(world.folder);
              const job = jobs[backupJobKey(id, world.folder)];
              const shared = share.actionFor(world);
              return (
                <motion.li
                  key={world.folder}
                  layout={glides ? "position" : false}
                  transition={{ layout: { duration: 0.3, ease: easeOutExpo } }}
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1, transition: { duration: 0.2 } }}
                  exit={{ opacity: 0, transition: { duration: 0.12 } }}
                  className="rounded-[var(--radius-panel)] transition-colors hover:bg-white/[0.035]"
                >
                  <ContextMenu items={worldItems(world, !job)}>
                    <div className="flex items-center gap-4 px-3 py-3">
                      <WorldIcon world={world} />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="truncate text-[14px] font-semibold text-bone-50">
                            {world.name}
                          </span>
                          <ModeChip world={world} />
                          <WorldShareChip action={shared} />
                          {world.readable ? null : (
                            <Chip tone="flare">{t("instance.worlds.unreadable")}</Chip>
                          )}
                        </div>
                        <p className="flex flex-wrap items-center gap-x-3 text-[12.5px] text-ash-300">
                          <span>
                            {world.lastPlayedAt
                              ? t("instance.worlds.lastPlayed", {
                                  when: formatRelative(world.lastPlayedAt),
                                })
                              : t("instance.worlds.neverPlayed")}
                          </span>
                          {world.version ? (
                            <span className="tabular-nums">{world.version}</span>
                          ) : null}
                          {world.cheats ? <span>{t("instance.worlds.cheats")}</span> : null}
                          {own.length > 0 ? (
                            <button
                              type="button"
                              aria-expanded={open}
                              onClick={() => toggle(world.folder)}
                              className="inline-flex cursor-pointer items-center gap-1 rounded-sm text-ash-300 transition-colors duration-150 hover:text-bone-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
                            >
                              {t("instance.worlds.backupCount", { count: own.length })}
                              <CaretDown
                                size={12}
                                className={cn(
                                  "transition-transform duration-200",
                                  open && "rotate-180",
                                )}
                              />
                            </button>
                          ) : null}
                        </p>
                      </div>
                      <span className="w-20 shrink-0 text-right text-[12px] text-ash-300 tabular-nums">
                        {formatBytes(world.sizeBytes)}
                      </span>
                      <div className="flex w-60 shrink-0 justify-end">
                        {job ? (
                          <JobProgress
                            job={job}
                            label={t("instance.worlds.backingUp")}
                            onCancel={() => cancel(backupJobKey(id, world.folder))}
                          />
                        ) : (
                          <Button
                            size="sm"
                            variant="secondary"
                            icon={<Archive size={15} className="text-gold-300" />}
                            disabled={locked || busy}
                            onClick={() => void backup(world)}
                          >
                            {t("instance.worlds.backup")}
                          </Button>
                        )}
                      </div>
                      <WorldShareAction action={shared} instance={instance} world={world} />
                      <Menu
                        trigger={
                          <IconButton
                            label={t("instance.worlds.more", { name: world.name })}
                            icon={<DotsThreeVertical size={18} weight="bold" />}
                            size="sm"
                          />
                        }
                        items={worldItems(world, false)}
                      />
                    </div>
                  </ContextMenu>
                  {/* The backups fade in place; the worlds below glide. */}
                  <AnimatePresence initial={false}>
                    {open && own.length > 0 ? (
                      <motion.ul
                        key="backups"
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1, transition: { duration: 0.18 } }}
                        exit={{ opacity: 0, transition: { duration: 0.1 } }}
                        className="space-y-0.5 pr-1 pb-2 pl-[76px]"
                      >
                        {own.map((item) => backupRow(item, false))}
                      </motion.ul>
                    ) : null}
                  </AnimatePresence>
                </motion.li>
              );
            })}
          </AnimatePresence>
        </ul>
      )}

      {otherBackups.length > 0 ? (
        <TabSection
          title={t("instance.worlds.otherBackups")}
          caption={t("instance.worlds.otherBackupsHint")}
        >
          <ul className="space-y-0.5">{otherBackups.map((item) => backupRow(item, true))}</ul>
        </TabSection>
      ) : null}

      <AnimatePresence initial={false}>
        {trashItems.length > 0 ? (
          <motion.div
            key="trash"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.2 } }}
            exit={{ opacity: 0, transition: { duration: 0.12 } }}
          >
            <TabSection title={t("instance.trash.title")} caption={t("instance.trash.hint")}>
              <ul className="relative space-y-0.5">
                <AnimatePresence initial={false} mode="popLayout">
                  {trashItems.map((item) => (
                    <TrashRow
                      key={item.id}
                      item={item}
                      disabled={locked && item.kind === "world"}
                      onRestore={() => void undo(item)}
                      onPurge={() => setPurging(item)}
                    />
                  ))}
                </AnimatePresence>
              </ul>
            </TabSection>
          </motion.div>
        ) : null}
      </AnimatePresence>

      <ConfirmDialog
        open={Boolean(deleting)}
        title={t("instance.worlds.deleteConfirm.title", { name: deleting?.name ?? "" })}
        body={t("instance.worlds.deleteConfirm.body")}
        confirmLabel={t("instance.worlds.deleteConfirm.confirm")}
        onConfirm={() => deleting && void removeWorld(deleting)}
        onClose={() => setDeleting(null)}
      />
      <ConfirmDialog
        open={Boolean(deletingBackup)}
        title={t("instance.worlds.deleteBackupConfirm.title")}
        body={t("instance.worlds.deleteBackupConfirm.body", {
          when: formatWhen(deletingBackup?.createdAt),
        })}
        confirmLabel={t("instance.worlds.deleteBackupConfirm.confirm")}
        onConfirm={() => deletingBackup && void removeBackup(deletingBackup)}
        onClose={() => setDeletingBackup(null)}
      />
      <ConfirmDialog
        open={Boolean(purging)}
        title={t("instance.trash.purgeConfirm.title", {
          name: purging ? trashLabel(purging) : "",
        })}
        body={t("instance.trash.purgeConfirm.body")}
        confirmLabel={t("instance.trash.purgeConfirm.confirm")}
        onConfirm={() => purging && void purge(purging)}
        onClose={() => setPurging(null)}
      />
      <DatapacksDialog
        instance={instance}
        world={datapacksFor}
        onClose={() => setDatapacksFor(null)}
      />
      <RestoreDialog
        backup={restoring}
        worldExists={restoringExists}
        onClose={() => setRestoring(null)}
        onRestore={(replace) => restoring && void restore(restoring, replace)}
      />
    </div>
  );
}

function RestoreDialog({
  backup,
  worldExists,
  onClose,
  onRestore,
}: {
  backup: WorldBackup | null;
  worldExists: boolean;
  onClose: () => void;
  onRestore: (replace: boolean) => void;
}) {
  const { t, i18n } = useTranslation();
  const [replace, setReplace] = useState(false);
  useEffect(() => {
    if (backup) setReplace(false);
  }, [backup]);
  // "Yedek: dün 03:33": the row it opened from names the world, the line says which backup.
  const taken = formatWhen(backup?.createdAt);
  const when = taken ? taken.charAt(0).toLocaleLowerCase(i18n.language) + taken.slice(1) : taken;
  const options = [
    {
      value: false,
      title: t("instance.worlds.restoreNew"),
      body: t("instance.worlds.restoreNewHint"),
    },
    {
      value: true,
      title: t("instance.worlds.restoreReplace"),
      body: t("instance.worlds.restoreReplaceHint"),
    },
  ];
  return (
    <ConfirmDialog
      open={Boolean(backup)}
      title={t("instance.worlds.restoreTitle")}
      body={t("instance.worlds.restoreBody", { when })}
      confirmLabel={t("instance.worlds.restore")}
      danger={false}
      onConfirm={() => onRestore(worldExists && replace)}
      onClose={onClose}
    >
      {worldExists ? (
        <div role="radiogroup" aria-label={t("instance.worlds.restoreTitle")} className="space-y-2">
          {options.map((option) => {
            const checked = replace === option.value;
            return (
              // biome-ignore lint/a11y/useSemanticElements: a styled card is clearer than a bare radio
              <button
                key={String(option.value)}
                type="button"
                role="radio"
                aria-checked={checked}
                onClick={() => setReplace(option.value)}
                className={cn(
                  "flex w-full cursor-pointer items-start gap-3 rounded-[var(--radius-panel)] px-3.5 py-3 text-left ring-1 transition-colors ring-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
                  // The chosen row is the quiet plate with a bone dot, as every radio row (design.md
                  // §6): the dialog's gold is its own button.
                  checked ? "bg-white/[0.06] ring-white/25" : "ring-white/10 hover:bg-white/4",
                )}
              >
                <span
                  aria-hidden
                  className={cn(
                    "mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full ring-2",
                    checked ? "ring-bone-100" : "ring-ash-500",
                  )}
                >
                  {checked ? <span className="size-2 rounded-full bg-bone-50" /> : null}
                </span>
                <span>
                  <span className="block text-[13.5px] font-semibold text-bone-50">
                    {option.title}
                  </span>
                  <span className="mt-0.5 block text-[12.5px] text-ash-400">{option.body}</span>
                </span>
              </button>
            );
          })}
        </div>
      ) : (
        <p className="text-[13px] text-ash-300">{t("instance.worlds.restoreAsNewOnly")}</p>
      )}
    </ConfirmDialog>
  );
}
