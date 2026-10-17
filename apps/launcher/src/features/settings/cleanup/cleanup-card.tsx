import { Broom, CaretDown, CheckCircle, WarningCircle } from "@phosphor-icons/react";
import { Button, cn, Segmented, Skeleton } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, type Ref, useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { enter, fade, glide, stagger, useMotionPrefs } from "@/features/modcenter/motion";
import { api, type Leftover, type OldSet, type SharedItem } from "@/lib/api";
import { formatBytes, formatCount } from "@/lib/format";
import { type CleanupSetDays, usePrefs } from "@/stores/prefs";
import { AnimatedBytes } from "./animated-bytes";
import { Check } from "./check";
import { CleanupDialog } from "./cleanup-dialog";
import {
  ageLabel,
  type GroupId,
  groupBytes,
  groupState,
  leftoverRowId,
  planOf,
  rowsOf,
  selectedBytes,
  setRowId,
  sharedRowId,
  staleSets,
  type Tri,
  toggleGroup,
  toggleRow,
} from "./model";

/** The smallest threshold: the list is fetched once with it and filtered by the chosen one. */
const MIN_DAYS = 30;
const DAY_OPTIONS: CleanupSetDays[] = [30, 60, 90];

export const cleanupKeys = {
  status: ["cleanup-status"] as const,
  sets: ["cleanup-sets"] as const,
  shared: ["cleanup-shared"] as const,
  leftovers: ["cleanup-leftovers"] as const,
  trash: ["cleanup-trash"] as const,
};

/** Drive of a Windows path ("C:"), for the free-space line. */
const driveOf = (path: string | undefined) =>
  path ? /^([A-Za-z]:)[\\/]/.exec(path)?.[1]?.toUpperCase() : undefined;

const triChecked = (tri: Tri): boolean | "indeterminate" =>
  tri === "all" ? true : tri === "some" ? "indeterminate" : false;

/**
 * Settings → Veri → Temizlik: what can go, grouped (version sets not played for a while, game
 * files no instance uses, caches and old logs), everything selected at first, the selected total
 * up top and on the disk bar, one "Temizle" that shows a summary before anything is deleted.
 * Lists arrive one by one and the total counts up as they do.
 */
export function CleanupCard({ root }: { root?: string }) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  const days = usePrefs((s) => s.cleanupSetDays);
  const setPref = usePrefs((s) => s.set);
  const status = useQuery({
    queryKey: cleanupKeys.status,
    queryFn: () => api.cleanup.status(),
    refetchInterval: 4000,
  });
  const setsQuery = useQuery({
    queryKey: cleanupKeys.sets,
    queryFn: () => api.cleanup.oldSets(MIN_DAYS),
    staleTime: 0,
  });
  const sharedQuery = useQuery({
    queryKey: cleanupKeys.shared,
    queryFn: () => api.cleanup.sharedFiles(),
    staleTime: 0,
  });
  const leftoversQuery = useQuery({
    queryKey: cleanupKeys.leftovers,
    queryFn: () => api.cleanup.leftovers(),
    staleTime: 0,
  });
  const [off, setOff] = useState<ReadonlySet<string>>(new Set());
  const [open, setOpen] = useState<Record<GroupId, boolean>>({
    sets: true,
    shared: false,
    leftovers: false,
  });
  const [confirming, setConfirming] = useState(false);

  const allSets = setsQuery.data?.sets ?? [];
  const sets = useMemo(() => staleSets(allSets, days), [allSets, days]);
  const shared = sharedQuery.data;
  const leftovers = leftoversQuery.data ?? [];
  const rows = useMemo(() => rowsOf(sets, shared, leftovers), [sets, shared, leftovers]);
  const groups = shared?.groups ?? [];
  const total = selectedBytes(rows, off, groups);
  const plan = useMemo(() => planOf(sets, shared, leftovers, off), [sets, shared, leftovers, off]);

  const loading = setsQuery.isPending || sharedQuery.isPending || leftoversQuery.isPending;
  const hasSets = allSets.length > 0;
  const hasShared = (shared?.items.length ?? 0) > 0;
  const hasLeftovers = leftovers.length > 0;
  const tidy = !loading && !hasSets && !hasShared && !hasLeftovers;
  // Nothing arrived yet: a placeholder instead of "0 B" (which would read as nothing to clean).
  const waiting = loading && rows.length === 0;
  const blocked = status.data?.blocked;
  const drive = driveOf(root);

  // A group header shows what it frees with the current choice (all of it, dimmed, when none).
  const groupSize = (group: GroupId) => {
    const chosen = selectedBytes(rows, off, groups, group);
    return chosen > 0 ? chosen : groupBytes(rows, group, groups);
  };
  const toggleAll = (group: GroupId) => setOff((current) => toggleGroup(rows, group, current));
  const toggle = (id: string) => setOff((current) => toggleRow(current, id));
  const flip = (group: GroupId) => setOpen((current) => ({ ...current, [group]: !current[group] }));
  const layout = prefs.layout ? ("position" as const) : false;

  return (
    <section
      aria-labelledby="cleanup-title"
      className="mt-4 rounded-[var(--radius-panel)] bg-obsidian-950/40 ring-1 ring-white/6"
    >
      <div className="relative flex items-start justify-between gap-6 px-5 pt-4">
        <div className="min-w-0">
          <h3 id="cleanup-title" className="font-display text-[18px] font-semibold text-bone-50">
            {t("dataSettings.cleanup.title")}
          </h3>
          <p className="mt-0.5 text-[12.5px] text-ash-400 tabular-nums">
            {status.data?.freeBytes === undefined
              ? " "
              : drive
                ? t("dataSettings.cleanup.freeOn", {
                    drive,
                    size: formatBytes(status.data.freeBytes),
                  })
                : t("dataSettings.cleanup.free", { size: formatBytes(status.data.freeBytes) })}
          </p>
        </div>
        <AnimatePresence initial={false} mode="popLayout">
          {tidy ? null : waiting ? (
            <motion.div key="wait" {...fade(0.2)} className="flex flex-col items-end" aria-busy>
              <Skeleton className="h-[26px] w-24" />
              <Skeleton className="mt-2 h-3 w-10" />
            </motion.div>
          ) : (
            <motion.div key="total" {...fade(0.2)} className="text-right">
              <AnimatedBytes
                value={total}
                from={0}
                className="block font-display text-[26px] leading-none font-semibold text-bone-50 tabular-nums"
              />
              <p className="mt-1.5 text-[12px] text-ash-400">
                {t("dataSettings.cleanup.selected")}
              </p>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      <DiskMeter
        className="mx-5 mt-3.5"
        diskBytes={status.data?.diskBytes}
        freeBytes={status.data?.freeBytes}
        selectedBytes={total}
      />

      <div className="relative mt-4 border-t border-white/6">
        <AnimatePresence initial={false} mode="popLayout">
          {tidy ? (
            <motion.div
              key="tidy"
              {...fade(0.24)}
              className="flex items-center gap-3 px-5 py-5"
              aria-live="polite"
            >
              <CheckCircle size={22} weight="duotone" className="shrink-0 text-jade-400" />
              <p className="text-[14px] text-bone-100">{t("dataSettings.cleanup.tidy")}</p>
            </motion.div>
          ) : null}
        </AnimatePresence>

        <GroupSlot
          pending={setsQuery.isPending}
          failed={setsQuery.isError}
          visible={hasSets}
          onRetry={() => void setsQuery.refetch()}
          index={0}
        >
          <Group
            id="sets"
            title={t("dataSettings.cleanup.sets.title")}
            caption={
              sets.length > 0
                ? t("dataSettings.cleanup.sets.caption", { days, count: sets.length })
                : t("dataSettings.cleanup.sets.none", { days })
            }
            state={groupState(rows, "sets", off)}
            bytes={groupSize("sets")}
            open={open.sets}
            onFlip={() => flip("sets")}
            onToggleAll={() => toggleAll("sets")}
            disabled={sets.length === 0}
          >
            <div className="flex pr-5 pb-2 pl-[50px]">
              <Segmented<`${CleanupSetDays}`>
                value={`${days}`}
                onValueChange={(v) => setPref("cleanupSetDays", Number(v) as CleanupSetDays)}
                label={t("dataSettings.cleanup.sets.threshold")}
                options={DAY_OPTIONS.map((option) => ({
                  value: `${option}` as `${CleanupSetDays}`,
                  label: t("dataSettings.cleanup.sets.days", { count: option }),
                }))}
              />
            </div>
            <Rows>
              {sets.map((set, index) => (
                <SetRow
                  key={set.key}
                  set={set}
                  index={index}
                  checked={!off.has(setRowId(set))}
                  onToggle={() => toggle(setRowId(set))}
                />
              ))}
            </Rows>
          </Group>
        </GroupSlot>

        <GroupSlot
          pending={sharedQuery.isPending}
          failed={sharedQuery.isError}
          visible={hasShared}
          onRetry={() => void sharedQuery.refetch()}
          index={1}
        >
          <Group
            id="shared"
            title={t("dataSettings.cleanup.shared.title")}
            caption={t("dataSettings.cleanup.shared.caption", {
              count: shared?.items.length ?? 0,
            })}
            state={groupState(rows, "shared", off)}
            bytes={groupSize("shared")}
            open={open.shared}
            onFlip={() => flip("shared")}
            onToggleAll={() => toggleAll("shared")}
          >
            <Rows>
              {(shared?.items ?? []).map((item, index) => (
                <SharedRow
                  key={item.id}
                  item={item}
                  index={index}
                  checked={!off.has(sharedRowId(item))}
                  onToggle={() => toggle(sharedRowId(item))}
                />
              ))}
            </Rows>
            <p className="pr-5 pb-3 pl-[50px] text-[12px] text-ash-400">
              {t("dataSettings.cleanup.shared.redownload")}
            </p>
          </Group>
        </GroupSlot>

        <GroupSlot
          pending={leftoversQuery.isPending}
          failed={leftoversQuery.isError}
          visible={hasLeftovers}
          onRetry={() => void leftoversQuery.refetch()}
          index={2}
        >
          <Group
            id="leftovers"
            title={t("dataSettings.cleanup.leftovers.title")}
            caption={t("dataSettings.cleanup.leftovers.caption", {
              count: formatCount(leftovers.reduce((sum, item) => sum + item.files, 0)),
            })}
            state={groupState(rows, "leftovers", off)}
            bytes={groupSize("leftovers")}
            open={open.leftovers}
            onFlip={() => flip("leftovers")}
            onToggleAll={() => toggleAll("leftovers")}
          >
            <Rows>
              {leftovers.map((item, index) => (
                <LeftoverRow
                  key={item.kind}
                  item={item}
                  index={index}
                  checked={!off.has(leftoverRowId(item.kind))}
                  onToggle={() => toggle(leftoverRowId(item.kind))}
                />
              ))}
            </Rows>
          </Group>
        </GroupSlot>
      </div>

      <AnimatePresence initial={false}>
        {tidy ? null : (
          <motion.div
            key="footer"
            layout={layout}
            transition={{ layout: glide }}
            {...fade(0.2)}
            className="flex min-h-[56px] items-center justify-between gap-4 border-t border-white/6 px-5 py-3"
          >
            <p className="min-w-0 text-[12.5px] text-ash-400" aria-live="polite">
              {blocked ? t(`dataSettings.cleanup.blocked.${blocked}`) : null}
            </p>
            <Button
              variant="primary"
              size="sm"
              icon={<Broom size={15} weight="bold" />}
              disabled={loading || total === 0 || Boolean(blocked)}
              onClick={() => setConfirming(true)}
            >
              {t("dataSettings.cleanup.clean")}
            </Button>
          </motion.div>
        )}
      </AnimatePresence>

      <CleanupDialog
        open={confirming}
        plan={plan}
        olderThanDays={days}
        onClose={() => setConfirming(false)}
      />
    </section>
  );
}

/**
 * The disk as one thin bar: used space in ash, the selection's share of it in gold (it slides
 * and stretches as the selection changes), free space empty. Transform-only.
 */
function DiskMeter({
  diskBytes,
  freeBytes,
  selectedBytes: selected,
  className,
}: {
  diskBytes?: number;
  freeBytes?: number;
  selectedBytes: number;
  className?: string;
}) {
  const { t } = useTranslation();
  if (!diskBytes || freeBytes === undefined || diskBytes <= 0) {
    return <div className={cn("h-1.5", className)} />;
  }
  const used = Math.min(1, Math.max(0, (diskBytes - freeBytes) / diskBytes));
  // A few gigabytes are a sliver of a big disk: never thinner than a few pixels.
  const gain = selected > 0 ? Math.min(used, Math.max(selected / diskBytes, 0.008)) : 0;
  return (
    <div
      role="img"
      aria-label={t("dataSettings.cleanup.meter", {
        used: formatBytes(diskBytes - freeBytes),
        disk: formatBytes(diskBytes),
        gain: formatBytes(selected),
      })}
      className={cn("relative h-1.5 overflow-hidden rounded-full bg-white/8", className)}
    >
      <div
        className="absolute inset-y-0 left-0 w-full origin-left bg-ash-500/45 transition-transform duration-500 ease-[cubic-bezier(0.16,1,0.3,1)]"
        style={{ transform: `scaleX(${used})` }}
      />
      <div
        className="absolute inset-y-0 left-0 w-full origin-left bg-[linear-gradient(90deg,var(--color-gold-600),var(--color-gold-400))] transition-transform duration-300 ease-[cubic-bezier(0.16,1,0.3,1)]"
        style={{ transform: `translateX(${(used - gain) * 100}%) scaleX(${gain})` }}
      />
    </div>
  );
}

/** A group's place in the card: a skeleton while its list loads, nothing when it is empty. */
function GroupSlot({
  pending,
  failed,
  visible,
  onRetry,
  index,
  children,
}: {
  pending: boolean;
  failed: boolean;
  visible: boolean;
  onRetry: () => void;
  index: number;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  const layout = prefs.layout ? ("position" as const) : false;
  const state = pending ? "pending" : failed ? "failed" : visible ? "shown" : "empty";
  return (
    <AnimatePresence initial={false} mode="popLayout">
      {state === "empty" ? null : (
        <motion.div
          key={state}
          layout={layout}
          transition={{ layout: glide }}
          initial={{ opacity: 0, transform: prefs.reduce ? "none" : "translateY(4px)" }}
          animate={{
            opacity: 1,
            transform: "translateY(0px)",
            transition: { ...enter(0.26), delay: state === "shown" ? stagger(index, 0.05) : 0 },
          }}
          exit={{ opacity: 0, transition: { duration: 0.12 } }}
          className="relative border-t border-white/6 first:border-t-0"
        >
          {state === "pending" ? (
            <GroupSkeleton />
          ) : state === "failed" ? (
            <div className="flex items-center gap-2.5 px-5 py-4" role="alert">
              <WarningCircle size={17} weight="fill" className="shrink-0 text-flare-400" />
              <p className="min-w-0 flex-1 text-[13px] text-ash-300">
                {t("dataSettings.cleanup.failed")}
              </p>
              <Button size="sm" variant="ghost" className="-my-1.5" onClick={onRetry}>
                {t("common.retry")}
              </Button>
            </div>
          ) : (
            children
          )}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/** Shaped like a group header: box, title and caption, size. */
function GroupSkeleton() {
  return (
    <div aria-busy className="flex items-center gap-3 px-5 py-[15px]">
      <Skeleton className="size-[18px] shrink-0" />
      <div className="flex-1 space-y-1.5">
        <Skeleton className="h-3.5 w-40" />
        <Skeleton className="h-3 w-28" />
      </div>
      <Skeleton className="h-4 w-14" />
      <span className="w-4" />
    </div>
  );
}

function Group({
  id,
  title,
  caption,
  state,
  bytes,
  open,
  onFlip,
  onToggleAll,
  disabled,
  children,
}: {
  id: GroupId;
  title: string;
  caption: string;
  state: Tri;
  bytes: number;
  open: boolean;
  onFlip: () => void;
  onToggleAll: () => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const bodyId = useId();
  return (
    <div>
      <div className="flex items-center gap-3 pl-5">
        <Check
          checked={disabled ? false : triChecked(state)}
          onCheckedChange={onToggleAll}
          label={t("dataSettings.cleanup.selectGroup", { group: title })}
          disabled={disabled}
        />
        <button
          type="button"
          aria-expanded={open}
          aria-controls={bodyId}
          onClick={onFlip}
          data-group={id}
          className="group/head flex min-w-0 flex-1 cursor-pointer items-center gap-3 py-3 pr-5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-inset"
        >
          <span className="min-w-0 flex-1">
            <span className="block text-[14px] font-medium text-bone-50">{title}</span>
            <span className="block truncate text-[12.5px] text-ash-400">{caption}</span>
          </span>
          {bytes > 0 ? (
            <span
              className={cn(
                "shrink-0 font-display text-[15px] font-semibold tabular-nums transition-colors duration-150",
                state === "none" ? "text-ash-400" : "text-bone-100",
              )}
            >
              {formatBytes(bytes)}
            </span>
          ) : null}
          <CaretDown
            size={15}
            className={cn(
              "shrink-0 text-ash-400 transition-transform duration-200 ease-[cubic-bezier(0.23,1,0.32,1)] group-hover/head:text-bone-100",
              open && "rotate-180",
            )}
          />
        </button>
      </div>
      <AnimatePresence initial={false} mode="popLayout">
        {open ? (
          <motion.div key="body" id={bodyId} {...fade(0.18)}>
            {children}
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

/**
 * Rows that come and go fade in place; the rows below glide. Each row forwards its `ref` to its
 * `motion.li` so a leaving row can be popped out of the flow while it fades.
 */
function Rows({ children }: { children: ReactNode }) {
  return (
    <ul className="relative pb-2">
      <AnimatePresence initial={false} mode="popLayout">
        {children}
      </AnimatePresence>
    </ul>
  );
}

function Row({
  ref,
  index,
  checked,
  onToggle,
  children,
}: {
  ref?: Ref<HTMLLIElement>;
  index: number;
  checked: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  const prefs = useMotionPrefs();
  const id = useId();
  return (
    <motion.li
      ref={ref}
      layout={prefs.layout ? "position" : false}
      transition={{ layout: glide }}
      initial={{ opacity: 0, transform: prefs.reduce ? "none" : "translateY(4px)" }}
      animate={{
        opacity: 1,
        transform: "translateY(0px)",
        transition: { ...enter(0.24), delay: stagger(index, 0.03) },
      }}
      exit={{ opacity: 0, transition: { duration: 0.12 } }}
    >
      <label
        htmlFor={id}
        className="flex cursor-pointer items-center gap-3 py-2 pr-5 pl-[50px] transition-colors duration-150 hover:bg-white/4"
      >
        <Check id={id} checked={checked} onCheckedChange={onToggle} />
        {children}
      </label>
    </motion.li>
  );
}

function Size({ bytes, on }: { bytes: number; on: boolean }) {
  return (
    <span
      className={cn(
        "w-[76px] shrink-0 text-right text-[13px] tabular-nums transition-colors duration-150",
        on ? "text-bone-100" : "text-ash-400",
      )}
    >
      {formatBytes(bytes)}
    </span>
  );
}

function SetRow({
  ref,
  set,
  index,
  checked,
  onToggle,
}: {
  ref?: Ref<HTMLLIElement>;
  set: OldSet;
  index: number;
  checked: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Row ref={ref} index={index} checked={checked} onToggle={onToggle}>
      <span className="w-[68px] shrink-0 font-display text-[15px] font-semibold text-bone-50 tabular-nums">
        {set.minecraft}
      </span>
      <span className="min-w-0 flex-1 truncate text-[13px] text-bone-100">{set.instanceName}</span>
      {set.lastUsedAt ? (
        <span className="shrink-0 text-[12px] text-ash-400">
          {t("dataSettings.cleanup.sets.played", { when: ageLabel(set.lastUsedAt) })}
        </span>
      ) : null}
      <Size bytes={set.sizeBytes} on={checked} />
    </Row>
  );
}

function SharedRow({
  ref,
  item,
  index,
  checked,
  onToggle,
}: {
  ref?: Ref<HTMLLIElement>;
  item: SharedItem;
  index: number;
  checked: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const c = "dataSettings.cleanup.shared";
  let title: string;
  let meta: string | null = null;
  switch (item.kind) {
    case "gameVersion":
      title = t(`${c}.gameVersion`, { version: item.minecraft ?? item.name });
      meta = t(`${c}.gameVersionMeta`);
      break;
    case "loaderProfile": {
      const loader = item.loader ? t(`loaders.${item.loader}`) : t(`${c}.loader`);
      title = item.loaderVersion ? `${loader} ${item.loaderVersion}` : loader;
      meta = item.minecraft ? t(`${c}.forMinecraft`, { version: item.minecraft }) : null;
      break;
    }
    case "javaRuntime":
      title = item.javaMajor ? t(`${c}.java`, { major: item.javaMajor }) : t(`${c}.javaUnknown`);
      break;
    case "libraries":
      title = t(`${c}.libraries`);
      meta = t("dataSettings.cleanup.files", { count: formatCount(item.files) });
      break;
    case "assets":
      title = t(`${c}.assets`);
      meta = t("dataSettings.cleanup.files", { count: formatCount(item.files) });
      break;
  }
  return (
    <Row ref={ref} index={index} checked={checked} onToggle={onToggle}>
      <span className="min-w-0 flex-1 truncate text-[13.5px] text-bone-100">
        {title}
        {meta ? <span className="ml-2 text-[12px] text-ash-400">{meta}</span> : null}
      </span>
      <Size bytes={item.bytes} on={checked} />
    </Row>
  );
}

function LeftoverRow({
  ref,
  item,
  index,
  checked,
  onToggle,
}: {
  ref?: Ref<HTMLLIElement>;
  item: Leftover;
  index: number;
  checked: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Row ref={ref} index={index} checked={checked} onToggle={onToggle}>
      <span className="min-w-0 flex-1 truncate text-[13.5px] text-bone-100">
        {t(`dataSettings.cleanup.leftovers.${item.kind}`)}
        <span className="ml-2 text-[12px] text-ash-400">
          {t("dataSettings.cleanup.files", { count: formatCount(item.files) })}
        </span>
      </span>
      <Size bytes={item.bytes} on={checked} />
    </Row>
  );
}
