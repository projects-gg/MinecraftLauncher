import { ArrowCounterClockwise, DotsThreeVertical, Package, Trash } from "@phosphor-icons/react";
import { Button, cn, Dialog, IconButton, Menu, Skeleton, toast } from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, type Ref, useState } from "react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog, trashIcons } from "@/features/library/parts";
import { trashLabel } from "@/features/library/trash-label";
import { enter, fade, glide, swap, useMotionPrefs } from "@/features/modcenter/motion";
import { api, type TrashedFile, type TrashedInstance, type TrashedSet } from "@/lib/api";
import { formatBytes, formatCount, formatRelative } from "@/lib/format";
import { ErrorLine, failure } from "../failure";
import { cleanupQueryKeys } from "./cleanup-dialog";

const c = "dataSettings.cleanup.trash";

/** Rows shown before "Tümünü göster": the newest are what a player looks for. */
const FIRST_ROWS = 8;

/** A row of the trash list: a version set, a deleted instance, or anything else. */
type Entry =
  | { kind: "set"; item: TrashedSet }
  | { kind: "instance"; item: TrashedInstance }
  | { kind: "file"; item: TrashedFile };

const deletedTime = (entry: Entry) => Date.parse(entry.item.deletedAt ?? "") || 0;

/** What a row is called: never a file name. */
function entryName(entry: Entry): string {
  switch (entry.kind) {
    case "set":
      return entry.item.instanceName;
    case "instance":
      return entry.item.name;
    case "file":
      return trashLabel(entry.item);
  }
}

/**
 * The launcher trash: everything in it, newest first (version sets, deleted instances, worlds,
 * backups, screenshots, the folders a reinstall set aside) with "Geri al" where it can go back
 * from here and "Kalıcı olarak sil" for one item's space now, and "Boşalt" for everything. It
 * empties itself after 30 days, oldest first when it grows too big.
 */
export function TrashRow() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const [showAll, setShowAll] = useState(false);
  // What "Kalıcı olarak sil" asks about; kept while the dialog fades out.
  const [purging, setPurging] = useState<Entry | null>(null);
  const [purgeOpen, setPurgeOpen] = useState(false);
  const trash = useQuery({ queryKey: ["cleanup-trash"], queryFn: () => api.cleanup.trash() });
  const refresh = (instanceId?: string) => {
    for (const queryKey of cleanupQueryKeys) {
      void queryClient.invalidateQueries({ queryKey });
    }
    // A world or screenshot that went back shows up on its instance page again.
    if (instanceId) {
      for (const key of ["trash", "worlds", "world-backups", "screenshots", "instance"]) {
        void queryClient.invalidateQueries({ queryKey: [key, instanceId] });
      }
    }
  };
  const empty = useMutation({
    mutationFn: () => api.cleanup.emptyTrash(),
    onSuccess: (result) => {
      setConfirming(false);
      toast.success(t(`${c}.emptied`, { size: formatBytes(result.freedBytes) }));
      refresh();
    },
  });
  const restore = useMutation({
    mutationFn: async (entry: Entry): Promise<string | null> => {
      if (entry.kind === "instance") {
        return (await api.restoreInstance(entry.item.trashId)).name;
      }
      if (entry.kind === "file") {
        await api.cleanup.restoreTrashed(entry.item.instanceId, entry.item.trashId);
        return null;
      }
      const back = await api.cleanup.restoreSets([entry.item.trashId]);
      if (back.restored > 0) return null;
      throw new Error(t(back.taken > 0 ? `${c}.restoreTaken` : `${c}.restoreFailed`));
    },
    onSuccess: (name, entry) => {
      if (entry.kind === "instance") {
        void queryClient.invalidateQueries({ queryKey: ["instances"] });
        toast.success(t(`${c}.instanceRestored`, { name: name ?? entry.item.name }));
      } else if (entry.kind === "file") {
        toast.success(t("instance.trash.restored", { name: entryName(entry) }));
      } else {
        toast.success(t(`${c}.restored`, { version: entry.item.minecraft }));
      }
    },
    onError: (cause, entry) =>
      toast.error(
        cause instanceof Error && !("details" in cause)
          ? cause.message
          : failure(
              cause,
              t(entry.kind === "instance" ? `${c}.instanceRestoreFailed` : `${c}.restoreFailed`),
            ).message,
      ),
    onSettled: (_data, _error, entry) =>
      refresh(entry.kind === "file" ? entry.item.instanceId : undefined),
  });
  const purge = useMutation({
    mutationFn: (entry: Entry) => api.cleanup.deleteTrashedSet(entry.item.trashId),
    onSuccess: (result, entry) => {
      const size = formatBytes(result.freedBytes);
      toast.success(
        entry.kind === "instance"
          ? t(`${c}.instancePurged`, { name: entry.item.name, size })
          : entry.kind === "file"
            ? t(`${c}.filePurged`, { name: entryName(entry), size })
            : t(`${c}.purged`, { version: entry.item.minecraft, size }),
      );
    },
    onError: (cause) => toast.error(failure(cause, t(`${c}.purgeFailed`)).message),
    onSettled: (_data, _error, entry) =>
      refresh(entry.kind === "file" ? entry.item.instanceId : undefined),
  });
  const working = empty.isPending || restore.isPending || purge.isPending;
  const data = trash.data;
  // Newest first, whatever they are.
  const entries: Entry[] = [
    ...(data?.instances ?? []).map((item) => ({ kind: "instance" as const, item })),
    ...(data?.sets ?? []).map((item) => ({ kind: "set" as const, item })),
    ...(data?.files ?? []).map((item) => ({ kind: "file" as const, item })),
  ].sort((a, b) => deletedTime(b) - deletedTime(a));
  const shown = showAll ? entries : entries.slice(0, FIRST_ROWS);
  const hidden = entries.length - shown.length;
  const summary = !data
    ? null
    : data.items === 0
      ? t(`${c}.empty`)
      : t(`${c}.summary`, { size: formatBytes(data.bytes), count: formatCount(data.items) });

  const purgeTitle = (entry: Entry | null) => {
    if (entry?.kind === "instance") return t(`${c}.instancePurgeTitle`, { name: entry.item.name });
    if (entry?.kind === "file") {
      return t("instance.trash.purgeConfirm.title", { name: entryName(entry) });
    }
    return t(`${c}.purgeTitle`, { version: entry?.item.minecraft ?? "" });
  };
  const purgeBody = (entry: Entry | null) =>
    entry?.kind === "instance"
      ? t(`${c}.instancePurgeBody`)
      : entry?.kind === "file"
        ? t("instance.trash.purgeConfirm.body")
        : t(`${c}.purgeBody`);

  return (
    <div className="border-b border-white/6 py-4">
      <div className="flex items-center justify-between gap-8">
        <div className="min-w-0">
          <p className="text-[14px] font-medium text-bone-50">{t(`${c}.title`)}</p>
          <div className="relative mt-0.5 h-[19px]">
            {trash.isPending ? (
              <Skeleton className="mt-1 h-3 w-28" />
            ) : (
              <AnimatePresence initial={false} mode="popLayout">
                <motion.p
                  key={summary ?? "none"}
                  {...swap(0.18)}
                  className="text-[12.5px] text-ash-400 tabular-nums"
                >
                  {summary}
                </motion.p>
              </AnimatePresence>
            )}
          </div>
        </div>
        <Button
          variant="secondary"
          size="sm"
          icon={<Trash size={15} />}
          disabled={!data || data.items === 0 || working}
          onClick={() => {
            empty.reset();
            setConfirming(true);
          }}
        >
          {t(`${c}.action`)}
        </Button>
      </div>

      <AnimatePresence initial={false}>
        {entries.length > 0 ? (
          <motion.ul key="entries" {...fade(0.18)} className="relative -mx-3 mt-2.5">
            <AnimatePresence initial={false} mode="popLayout">
              {shown.map((entry) => {
                const Icon = entry.kind === "file" ? trashIcons[entry.item.kind] : Package;
                return (
                  <TrashedRow
                    key={entry.item.trashId}
                    lead={
                      entry.kind === "set" ? (
                        entry.item.minecraft
                      ) : (
                        <Icon size={18} className="text-ash-300" aria-hidden />
                      )
                    }
                    name={entryName(entry)}
                    from={entry.kind === "file" ? entry.item.instanceName : undefined}
                    deletedAt={entry.item.deletedAt}
                    sizeBytes={entry.item.sizeBytes}
                    disabled={working}
                    restorable={entry.kind !== "file" || entry.item.restorable}
                    onRestore={() => restore.mutate(entry)}
                    onPurge={() => {
                      setPurging(entry);
                      setPurgeOpen(true);
                    }}
                  />
                );
              })}
            </AnimatePresence>
            {hidden > 0 || showAll ? (
              <li className="px-3 pt-1">
                <Button variant="ghost" size="sm" onClick={() => setShowAll((all) => !all)}>
                  {showAll ? t(`${c}.showLess`) : t(`${c}.showAll`, { count: entries.length })}
                </Button>
              </li>
            ) : null}
          </motion.ul>
        ) : null}
      </AnimatePresence>

      <ConfirmDialog
        open={purgeOpen}
        title={purgeTitle(purging)}
        body={purgeBody(purging)}
        confirmLabel={t(`${c}.purge`)}
        onConfirm={() => {
          if (purging) purge.mutate(purging);
        }}
        onClose={() => setPurgeOpen(false)}
      />

      <Dialog
        open={confirming}
        onOpenChange={(next) => {
          if (!next && !empty.isPending) setConfirming(false);
        }}
        title={t(`${c}.confirmTitle`)}
        dismissOnOutsideClick={false}
        description={t(`${c}.confirmBody`)}
        closeLabel={t("common.close")}
        width={460}
        dismissible={!empty.isPending}
        footer={
          <>
            <Button variant="ghost" disabled={empty.isPending} onClick={() => setConfirming(false)}>
              {t("dataSettings.cancel")}
            </Button>
            <Button
              variant="danger"
              icon={<Trash size={16} />}
              disabled={empty.isPending}
              onClick={() => empty.mutate()}
            >
              {empty.isPending ? t(`${c}.emptying`) : t(`${c}.confirm`)}
            </Button>
          </>
        }
      >
        {data ? (
          <p className="pt-1 font-display text-[22px] font-semibold text-bone-50 tabular-nums">
            {formatBytes(data.bytes)}
            <span className="ml-2 font-sans text-[13px] font-normal text-ash-400">
              {t(`${c}.items`, { count: formatCount(data.items) })}
            </span>
          </p>
        ) : null}
        {empty.error ? (
          <ErrorLine className="mt-4" error={failure(empty.error, t(`${c}.failed`))} />
        ) : null}
      </Dialog>
    </div>
  );
}

/**
 * One item in the trash, laid out like the old-set rows of the card above. Forwards `ref` so a
 * leaving row pops out of the flow while it fades.
 */
function TrashedRow({
  ref,
  lead,
  name,
  from,
  deletedAt,
  sizeBytes,
  disabled,
  restorable,
  onRestore,
  onPurge,
}: {
  ref?: Ref<HTMLLIElement>;
  /** The set's Minecraft version, or the item's mark. */
  lead: ReactNode;
  name: string;
  /** The instance a world, backup or screenshot came from. */
  from?: string;
  deletedAt?: string;
  sizeBytes: number;
  disabled: boolean;
  /** "Geri al" works from here (else its place stays empty, so the columns line up). */
  restorable: boolean;
  onRestore: () => void;
  onPurge: () => void;
}) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  return (
    <motion.li
      ref={ref}
      layout={prefs.layout ? "position" : false}
      transition={{ layout: glide }}
      initial={{ opacity: 0, transform: prefs.reduce ? "none" : "translateY(4px)" }}
      animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.24) }}
      exit={{ opacity: 0, transition: { duration: 0.12 } }}
      className="flex items-center gap-3 rounded-md py-1 pr-1 pl-3 transition-colors duration-150 hover:bg-white/4"
    >
      <span className="flex w-[68px] shrink-0 items-center font-display text-[15px] font-semibold text-bone-50 tabular-nums">
        {lead}
      </span>
      <span className="flex min-w-0 flex-1 items-baseline gap-2">
        <span className="min-w-0 truncate text-[13px] text-bone-100">{name}</span>
        {from ? <span className="shrink-0 text-[12px] text-ash-400">{from}</span> : null}
      </span>
      {deletedAt ? (
        <span className="shrink-0 text-[12px] text-ash-400">
          {Date.now() - Date.parse(deletedAt) < 60_000
            ? t(`${c}.deletedJustNow`)
            : t(`${c}.deletedAt`, { when: formatRelative(deletedAt) })}
        </span>
      ) : null}
      <span className="w-[76px] shrink-0 text-right text-[13px] text-bone-100 tabular-nums">
        {formatBytes(sizeBytes)}
      </span>
      <Button
        size="sm"
        variant="ghost"
        icon={<ArrowCounterClockwise size={15} />}
        disabled={disabled || !restorable}
        aria-hidden={restorable ? undefined : true}
        tabIndex={restorable ? undefined : -1}
        className={cn(!restorable && "invisible")}
        onClick={onRestore}
      >
        {t(`${c}.restore`)}
      </Button>
      <Menu
        trigger={
          <IconButton
            label={t(`${c}.more`, { name })}
            icon={<DotsThreeVertical size={18} weight="bold" />}
            size="sm"
            disabled={disabled}
          />
        }
        items={[
          {
            label: t(`${c}.purge`),
            icon: <Trash size={16} />,
            danger: true,
            onSelect: onPurge,
          },
        ]}
      />
    </motion.li>
  );
}
