import {
  ArrowCounterClockwise,
  ArrowSquareOut,
  ArrowsClockwise,
  Package,
} from "@phosphor-icons/react";
import {
  Button,
  Chip,
  cn,
  EmptyState,
  SearchInput,
  Segmented,
  Skeleton,
  Switch,
  toast,
} from "@pl/ui";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { newJobId, showError } from "@/features/content/jobs";
import { ProjectIcon } from "@/features/content/parts";
import { useDebounced } from "@/features/content/use-content";
import { ChangeBar } from "@/features/library/change-bar";
import {
  api,
  type JobProgress,
  type ModCategory,
  type ModCenterItem,
  type ModCenterView,
  type ModSelection,
} from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { type ModDraft, useDrafts } from "@/stores/drafts";
import { fade, glide, type MotionPrefs, pop, swap, useMotionPrefs } from "./motion";
import { JobLine } from "./parts";
import {
  bringsAlong,
  type Choices,
  categoryOrder,
  choicesOf,
  enabledSet,
  matches,
  neededBy,
  pendingSelection,
  safePageUrl,
  totals,
} from "./selection";

type Filter = ModCategory | "all";

/** Refreshes everything that shows the instance's mods after the selection changed. */
export function refreshMods(queryClient: ReturnType<typeof useQueryClient>, instanceId: string) {
  void queryClient.invalidateQueries({ queryKey: ["mod-center", instanceId] });
  void queryClient.invalidateQueries({ queryKey: ["instance", instanceId] });
  void queryClient.invalidateQueries({ queryKey: ["content-scan", instanceId, "mod"] });
}

/**
 * "Mod sayfası": the mod's official page in the browser (https only). Quiet like the summary it
 * ends, marked as external, named with the mod for screen readers.
 */
function PageLink({ url, name }: { url: string; name: string }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={() => void api.openUrl(url)}
      aria-label={t("modCenter.item.pageFor", { name })}
      className="inline-flex cursor-pointer items-center gap-1 whitespace-nowrap rounded-sm align-baseline text-[12.5px] font-medium text-ash-300 transition-colors duration-150 hover:text-gold-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
    >
      {t("modCenter.item.page")}
      <ArrowSquareOut size={12} aria-hidden className="shrink-0" />
    </button>
  );
}

/** A badge that grows in and out (from 90%, never from nothing); a direct presence child. */
function badge(prefs: MotionPrefs, key: string, label: string, tone?: "gold") {
  return (
    <motion.span key={key} {...pop(prefs.reduce)} className="inline-flex">
      <Chip tone={tone}>{label}</Chip>
    </motion.span>
  );
}

function ItemRow({
  item,
  items,
  enabled,
  changed,
  reverting,
  locked,
  prefs,
  onToggle,
  onRevert,
  onOpen,
}: {
  item: ModCenterItem;
  items: ModCenterItem[];
  enabled: Set<string>;
  /** The pending choice differs from the recorded one. */
  changed: boolean;
  reverting: boolean;
  /** The game is running or changes are being applied. */
  locked: boolean;
  prefs: MotionPrefs;
  onToggle: (on: boolean) => void;
  onRevert: (on: boolean) => void;
  onOpen?: () => void;
}) {
  const { t } = useTranslation();
  const on = enabled.has(item.id);
  const needed = neededBy(items, enabled, item.id);
  const forced = needed.length > 0;
  const brings = on ? [] : bringsAlong(items, item.id, enabled);
  const names = (list: string[]) => list.join(", ");
  const page = safePageUrl(item.pageUrl);

  const notes: string[] = [];
  if (item.manual === "removed") {
    notes.push(
      on && forced
        ? t("modCenter.item.removedBack", { names: names(needed) })
        : t("modCenter.item.removed"),
    );
  } else if (item.manual === "disabled") {
    notes.push(
      on && forced
        ? t("modCenter.item.disabledBack", { names: names(needed) })
        : t("modCenter.item.disabled"),
    );
  } else if (item.manual === "enabled") {
    notes.push(t("modCenter.item.enabled"));
  }
  if (forced && item.manual !== "removed" && item.manual !== "disabled") {
    notes.push(t("modCenter.item.requiredBy", { names: names(needed) }));
  }
  if (brings.length > 0) {
    notes.push(t("modCenter.item.bringsAlong", { names: names(brings) }));
  }

  return (
    <motion.li
      layout={prefs.layout ? "position" : false}
      transition={{ layout: glide }}
      className="group/row relative flex items-start gap-3 rounded-[var(--radius-panel)] px-3 py-2.5"
    >
      {/* Hover and "changed" tints fade in place (opacity only). */}
      <span
        aria-hidden
        className="pointer-events-none absolute inset-0 rounded-[inherit] bg-white/5 opacity-0 transition-opacity duration-150 group-hover/row:opacity-100"
      />
      <span
        aria-hidden
        className={cn(
          "pointer-events-none absolute inset-0 rounded-[inherit] bg-gold-500/[0.08] transition-opacity duration-200",
          changed ? "opacity-100" : "opacity-0",
        )}
      />
      <ProjectIcon
        src={item.iconUrl}
        size={36}
        className={cn("relative transition-opacity duration-200", !on && "opacity-55")}
      />
      <div className="relative min-w-0 flex-1">
        <div className="relative flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
          {onOpen ? (
            <button
              type="button"
              onClick={onOpen}
              className="min-w-0 cursor-pointer truncate text-left text-[14px] font-semibold text-bone-50 transition-colors duration-150 hover:text-gold-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              {item.title}
            </button>
          ) : (
            <span className="truncate text-[14px] font-semibold text-bone-50">{item.title}</span>
          )}
          <AnimatePresence initial={false} mode="popLayout">
            {item.isNew ? badge(prefs, "new", t("modCenter.badges.new"), "gold") : null}
            {item.updateAvailable && on
              ? badge(prefs, "update", t("modCenter.badges.update"))
              : null}
            {item.modified ? badge(prefs, "modified", t("modCenter.badges.modified")) : null}
          </AnimatePresence>
        </div>
        {item.summary || page ? (
          <p className="mt-0.5 text-[13px] leading-snug text-ash-400">
            {item.summary}
            {item.summary && page ? " " : null}
            {page ? <PageLink url={page} name={item.title} /> : null}
          </p>
        ) : null}
        <div className="relative">
          <AnimatePresence initial={false} mode="popLayout">
            {notes.map((note) => (
              <motion.p
                key={note}
                {...swap(0.2)}
                className="mt-1 text-[12.5px] leading-snug text-ash-300"
              >
                {note}
              </motion.p>
            ))}
          </AnimatePresence>
        </div>
        {item.modified ? (
          <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1">
            <p className="text-[12.5px] text-ash-300">{t("modCenter.item.modified")}</p>
            <Button
              size="sm"
              variant="ghost"
              disabled={locked}
              icon={<ArrowCounterClockwise size={14} />}
              onClick={() => onRevert(!reverting)}
            >
              {reverting ? t("modCenter.item.keepMine") : t("modCenter.item.revert")}
            </Button>
          </div>
        ) : null}
      </div>
      <span className="relative hidden w-[72px] shrink-0 pt-0.5 text-right text-[12px] text-ash-300 tabular-nums md:block">
        {formatBytes(item.sizeBytes)}
      </span>
      <Switch
        checked={on}
        onCheckedChange={onToggle}
        disabled={locked || (forced && on)}
        label={t("modCenter.item.toggle", { name: item.title })}
        hideLabel
        className="relative pt-0.5"
      />
    </motion.li>
  );
}

function LibraryRow({
  item,
  items,
  enabled,
  prefs,
}: {
  item: ModCenterItem;
  items: ModCenterItem[];
  enabled: Set<string>;
  prefs: MotionPrefs;
}) {
  const { t } = useTranslation();
  const on = enabled.has(item.id);
  const needed = neededBy(items, enabled, item.id);
  const state = on ? t("modCenter.item.libraryOn") : t("modCenter.item.libraryOff");
  const names = needed.join(", ");
  // A library the player removed or disabled comes back while a mod needs it: say why.
  const note =
    !on || needed.length === 0
      ? null
      : item.manual === "removed"
        ? t("modCenter.item.removedBack", { names })
        : item.manual === "disabled"
          ? t("modCenter.item.disabledBack", { names })
          : t("modCenter.item.libraryFor", { names });
  return (
    <motion.li
      layout={prefs.layout ? "position" : false}
      transition={{ layout: glide }}
      className="flex items-center gap-3 px-3 py-2"
    >
      <ProjectIcon
        src={item.iconUrl}
        size={30}
        className={cn("transition-opacity duration-200", !on && "opacity-50")}
      />
      <div className="relative min-w-0 flex-1">
        <p className="truncate text-[13.5px] font-medium text-bone-100">{item.title}</p>
        <AnimatePresence initial={false} mode="popLayout">
          {note ? (
            <motion.p key={note} {...swap(0.2)} className="truncate text-[12px] text-ash-400">
              {note}
            </motion.p>
          ) : null}
        </AnimatePresence>
      </div>
      <span className="relative shrink-0 text-[12px] text-ash-400">
        <AnimatePresence initial={false} mode="popLayout">
          <motion.span key={state} {...swap(0.18)} className="block">
            {state}
          </motion.span>
        </AnimatePresence>
      </span>
    </motion.li>
  );
}

/** What applying would do, crossfading as it changes; dimmed while it is recalculated. */
function Summary({ text, stale }: { text: string | null; stale: boolean }) {
  return (
    <div className="relative min-w-0 flex-1" aria-live="polite">
      <AnimatePresence initial={false} mode="popLayout">
        {text === null ? (
          <motion.span key="loading" {...fade(0.16)} className="block py-0.5">
            <Skeleton className="h-3.5 w-56 max-w-full" />
          </motion.span>
        ) : (
          <motion.p key={text} {...swap(0.2)} className="text-[13.5px] text-bone-100">
            {/* Dimming is CSS on an inner node: motion owns the outer node's opacity. */}
            <span className={cn("transition-opacity duration-200", stale && "opacity-55")}>
              {text}
            </span>
          </motion.p>
        )}
      </AnimatePresence>
    </div>
  );
}

/** How long the list must have been on screen before its new recommendations count as seen. */
const SEEN_AFTER_MS = 1500;

const noChoices: Record<string, boolean> = {};

/**
 * Applies in progress, per instance. Kept outside the tab: switching to "Keşfet" and back while
 * one runs remounts the tab, which still shows the progress and keeps the rows locked (no second
 * "Uygula", no edits that the running apply would drop when it ends).
 */
const useApplying = create<{
  progress: Record<string, JobProgress>;
  set: (instanceId: string, progress: JobProgress | null) => void;
}>((set) => ({
  progress: {},
  set: (instanceId, next) =>
    set((state) => {
      const progress = { ...state.progress };
      if (next) progress[instanceId] = next;
      else delete progress[instanceId];
      return { progress };
    }),
}));

/**
 * New recommendations count as seen once the list has been on screen for a moment (while the
 * window is visible) and the player leaves it: another section or tab, another page. Their
 * "Yeni" badges stay for the whole visit, so what is new can still be found. Not while the game
 * runs (the next visit marks them).
 */
function useMarkSeenOnLeave(instanceId: string, hasNew: boolean, locked: boolean) {
  const queryClient = useQueryClient();
  const state = useRef({ hasNew, locked });
  state.current = { hasNew, locked };
  useEffect(() => {
    let seenMs = 0;
    let since: number | null = document.hidden ? null : performance.now();
    const onVisibility = () => {
      if (document.hidden && since !== null) {
        seenMs += performance.now() - since;
        since = null;
      } else if (!document.hidden && since === null) {
        since = performance.now();
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      if (since !== null) seenMs += performance.now() - since;
      const { hasNew: anyNew, locked: busy } = state.current;
      if (!anyNew || busy || seenMs < SEEN_AFTER_MS) return;
      api.modCenter
        .markSeen(instanceId)
        .then(() => refreshMods(queryClient, instanceId))
        .catch(() => undefined);
    };
  }, [instanceId, queryClient]);
}

/** "Önerilenler": the official pack's recommended mods with the player's choices. */
export function RecommendedTab({
  view,
  onOpenProject,
}: {
  view: ModCenterView;
  onOpenProject: (item: ModCenterItem) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const prefs = useMotionPrefs();
  const instanceId = view.instanceId;
  // Choices not applied yet live in the draft store: they survive tab and section switches and
  // background refreshes of the view, until they are applied or discarded.
  const draft = useDrafts((s) => s.mods[instanceId]);
  const setDraft = useDrafts((s) => s.setMods);
  const picked = draft?.choices ?? noChoices;
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const applying = useApplying((s) => s.progress[instanceId] ?? null);
  const setApplying = (progress: JobProgress | null) =>
    useApplying.getState().set(instanceId, progress);

  const choices = useMemo<Choices>(
    () => ({ ...choicesOf(view.items), ...picked }),
    [view.items, picked],
  );
  // Only an item whose file is still changed can go back to the pack's copy.
  const revert = useMemo(
    () =>
      new Set(
        (draft?.revert ?? []).filter((id) => view.items.some((i) => i.id === id && i.modified)),
      ),
    [draft?.revert, view.items],
  );
  const enabled = useMemo(() => enabledSet(view.items, choices), [view.items, choices]);
  const selection = useMemo(
    () => pendingSelection(view.items, choices, revert),
    [view.items, choices, revert],
  );
  // A draft the view caught up with (a sync recorded the same choice) changes nothing: it goes,
  // and the tab's marker with it.
  useEffect(() => {
    if (draft && !selection && !applying) setDraft(instanceId, null);
  }, [draft, selection, applying, instanceId, setDraft]);
  const editDraft = (change: (current: ModDraft) => ModDraft) => {
    const next = change({ choices: picked, revert: [...revert] });
    const empty = Object.keys(next.choices).length === 0 && next.revert.length === 0;
    setDraft(instanceId, empty ? null : next);
  };
  const toggle = (item: ModCenterItem, on: boolean) =>
    editDraft((current) => {
      const next = { ...current.choices };
      if (on === item.selected) delete next[item.id];
      else next[item.id] = on;
      return { ...current, choices: next };
    });
  const setReverting = (item: ModCenterItem, on: boolean) =>
    editDraft((current) => {
      const others = current.revert.filter((id) => id !== item.id);
      return { ...current, revert: on ? [...others, item.id] : others };
    });
  const settled = useDebounced(selection, 200);
  const preview = useQuery({
    queryKey: ["mod-center-preview", instanceId, settled],
    queryFn: () => api.modCenter.preview(instanceId, settled as ModSelection),
    enabled: settled !== null && !applying,
    staleTime: 30_000,
    retry: false,
    placeholderData: keepPreviousData,
  });
  const locked = view.gameRunning || applying !== null;
  const position = prefs.layout ? ("position" as const) : false;

  const apply = async (chosen: ModSelection) => {
    if (useApplying.getState().progress[instanceId]) return;
    setApplying({ stage: "resolving", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
    try {
      const result = await api.modCenter.apply(instanceId, chosen, newJobId(), (p) =>
        setApplying(p),
      );
      // The fresh view first, then the bar leaves: it never flashes back to the old summary.
      await queryClient.refetchQueries({ queryKey: ["mod-center", instanceId], exact: true });
      setDraft(instanceId, null);
      toast.success(t("modCenter.applied.title"));
      if (result.issues > 0) toast(t("modCenter.applied.issues"));
    } catch (error) {
      showError(error);
    } finally {
      setApplying(null);
      refreshMods(queryClient, instanceId);
    }
  };

  useMarkSeenOnLeave(
    instanceId,
    view.items.some((i) => i.isNew && !i.library),
    locked,
  );

  const choosable = view.items.filter((i) => !i.library);
  const libraries = view.items.filter((i) => i.library);
  const present = categoryOrder.filter(
    (c) => c !== "library" && choosable.some((i) => i.category === c),
  );
  const visible = choosable.filter(
    (i) => (filter === "all" || i.category === filter) && matches(i, query),
  );
  const updates = view.items.filter((i) => i.updateAvailable && enabled.has(i.id)).length;
  const grouped = filter === "all" && !query.trim();

  const row = (item: ModCenterItem) => (
    <ItemRow
      key={item.id}
      item={item}
      items={view.items}
      enabled={enabled}
      changed={(choices[item.id] ?? item.selected) !== item.selected || revert.has(item.id)}
      reverting={revert.has(item.id)}
      locked={locked}
      prefs={prefs}
      onToggle={(on) => toggle(item, on)}
      onRevert={(on) => setReverting(item, on)}
      onOpen={item.projectId ? () => onOpenProject(item) : undefined}
    />
  );

  const summaryText = (): string | null => {
    if (!preview.data) return null;
    const { added, updated, removed, otherFiles } = totals(preview.data.summary);
    const parts: string[] = [];
    if (added > 0) parts.push(t("modCenter.bar.added", { count: added }));
    if (updated > 0) parts.push(t("modCenter.bar.updated", { count: updated }));
    if (removed > 0) parts.push(t("modCenter.bar.removed", { count: removed }));
    if (parts.length === 0 && otherFiles) parts.push(t("modCenter.bar.settings"));
    if (parts.length === 0) return t("modCenter.bar.noFiles");
    const size =
      preview.data.summary.downloadBytes > 0
        ? ` ${t("modCenter.bar.download", { size: formatBytes(preview.data.summary.downloadBytes) })}`
        : "";
    return `${parts.join(", ")}.${size}`;
  };
  const stale = preview.isPlaceholderData || preview.isFetching || settled !== selection;

  const showUpdates = updates > 0 && !selection;
  const barMode = applying ? "applying" : view.gameRunning ? "running" : "summary";

  return (
    <div className="relative flex min-h-full flex-col">
      {/* New recommendations need no notice of their own: the section says "(N yeni)" and the
          rows carry "Yeni" (seen on leaving, see useMarkSeenOnLeave). */}
      <AnimatePresence initial={false} mode="popLayout">
        {showUpdates ? (
          <motion.div
            key="updates"
            layout={prefs.layout}
            transition={{ layout: glide }}
            {...fade(0.2)}
            style={{ borderRadius: 10 }}
            className="glass relative mb-3 flex min-h-11 items-center gap-3 px-4 py-1.5"
          >
            <ArrowsClockwise size={18} className="shrink-0 text-gold-300" />
            <p className="flex-1 text-[13.5px] text-bone-100">
              {t("modCenter.updates", { count: updates })}
            </p>
            <Button
              size="sm"
              variant="secondary"
              disabled={locked}
              onClick={() => void apply({ items: {}, revert: [] })}
            >
              {t("modCenter.updateNow")}
            </Button>
          </motion.div>
        ) : null}
      </AnimatePresence>

      <motion.div
        layout={position}
        transition={{ layout: glide }}
        className="mb-3 flex flex-wrap items-center gap-2.5"
      >
        <SearchInput
          value={query}
          onChange={setQuery}
          placeholder={t("modCenter.search")}
          label={t("modCenter.search")}
          clearLabel={t("library.clearSearch")}
          className="w-64"
        />
        {present.length > 1 ? (
          <Segmented
            value={filter}
            onValueChange={setFilter}
            label={t("modCenter.categoriesLabel")}
            options={[
              { value: "all" as Filter, label: t("modCenter.categories.all") },
              ...present.map((c) => ({
                value: c as Filter,
                label: t(`modCenter.categories.${c}`),
              })),
            ]}
          />
        ) : null}
      </motion.div>

      {/* Filtering swaps the list in place (no glide while typing or picking a category). */}
      <div key={`${filter}|${query.trim()}`}>
        {choosable.length === 0 ? (
          <EmptyState
            icon={<Package size={26} weight="duotone" />}
            title={t("modCenter.empty.title")}
          />
        ) : visible.length === 0 ? (
          <p className="px-3 py-8 text-center text-[13px] text-ash-400">{t("modCenter.noMatch")}</p>
        ) : grouped ? (
          <div className="space-y-4">
            {present.map((category) => (
              <motion.section
                key={category}
                layout={position}
                transition={{ layout: glide }}
                aria-label={t(`modCenter.categories.${category}`)}
              >
                <h3 className="mb-1 px-3 font-display text-[15px] font-semibold text-bone-50">
                  {t(`modCenter.categories.${category}`)}
                </h3>
                <ul className="space-y-0.5">
                  {visible.filter((i) => i.category === category).map(row)}
                </ul>
              </motion.section>
            ))}
          </div>
        ) : (
          <ul className="space-y-0.5">{visible.map(row)}</ul>
        )}

        {libraries.length > 0 && grouped ? (
          <motion.section
            layout={position}
            transition={{ layout: glide }}
            aria-label={t("modCenter.categories.library")}
            className="mt-6"
          >
            <h3 className="mb-1 px-3 font-display text-[15px] font-semibold text-bone-50">
              {t("modCenter.categories.library")}
            </h3>
            <ul>
              {libraries.map((item) => (
                <LibraryRow
                  key={item.id}
                  item={item}
                  items={view.items}
                  enabled={enabled}
                  prefs={prefs}
                />
              ))}
            </ul>
          </motion.section>
        ) : null}
      </div>

      <ChangeBar open={Boolean(selection || applying)} label={t("modCenter.bar.label")}>
        <AnimatePresence initial={false} mode="popLayout">
          {barMode === "applying" ? (
            <motion.div key="applying" {...swap(0.2)} className="min-w-0 flex-1">
              <JobLine progress={applying} label={t("modCenter.bar.applying")} />
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
              <Summary text={summaryText()} stale={stale} />
              <Button variant="ghost" onClick={() => setDraft(instanceId, null)}>
                {t("modCenter.bar.discard")}
              </Button>
              <Button variant="primary" onClick={() => selection && void apply(selection)}>
                {t("modCenter.bar.apply")}
              </Button>
            </motion.div>
          )}
        </AnimatePresence>
      </ChangeBar>
    </div>
  );
}
