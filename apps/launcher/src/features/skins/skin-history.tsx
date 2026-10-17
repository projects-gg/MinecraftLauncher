// "Önceki skinler": the skins a Microsoft account wore while the launcher knew it, ten to a page,
// to put one back on. The history lives on this computer only (pl_auth::SkinHistory).
import { CaretLeft, CaretRight, Star, X } from "@phosphor-icons/react";
import { cn, easeOutExpo, IconButton, PixelBrackets, toast, useMotionPrefs } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type KeyboardEvent, memo, useCallback, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type SkinHistory, type SkinHistoryEntry } from "@/lib/api";
import { formatRelative } from "@/lib/format";
import {
  clampPage,
  hasPreviousSkins,
  insertAt,
  moveFocus,
  PAGE_SIZE,
  pageCount,
  pageItems,
  pageOf,
  reservedRows,
  steadyOrder,
} from "./history-model";
import { HISTORY_UNDO_MS, showRemovedToast } from "./history-toast";

/** Tile height and grid gap in px: fixed, so the grid's size never depends on its content. */
const TILE_H = 148;
const GAP = 10;
/** Room around the grid for the selection brackets and focus rings (the grid clips its pages). */
const BLEED = 5;

export const historyKey = (accountId: string) => ["skin-history", accountId] as const;

/** The history of an account; pictures still missing are drawn in the background. */
export function useSkinHistory(accountId: string, enabled: boolean) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: historyKey(accountId),
    queryFn: () => api.skins.history(accountId),
    enabled,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
  });
  // Once per set of missing pictures: a texture that cannot be had now is not asked again in
  // a loop, and a new skin makes a new set.
  const missing =
    query.data?.entries
      .filter((entry) => !entry.thumbUrl)
      .map((entry) => `${entry.id}:${entry.variant}`)
      .sort()
      .join(",") ?? "";
  useQuery({
    queryKey: ["skin-history-pictures", accountId, missing],
    queryFn: async () => {
      const made = await api.skins.prepareHistory(accountId);
      if (made > 0) void queryClient.invalidateQueries({ queryKey: historyKey(accountId) });
      return made;
    },
    enabled: enabled && missing.length > 0,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
  });
  return query;
}

/** Why "Giy" is not offered now. */
export type WearBlock = "offline" | "cooldown" | "busy" | null;

interface Removal {
  entry: SkinHistoryEntry;
  index: number;
  toastId: string | number;
  at: number;
}

/** A figure-shaped placeholder while a picture is drawn. */
const FIGURE_CLIP =
  "polygon(25% 0, 75% 0, 75% 25%, 100% 25%, 100% 62.5%, 75% 62.5%, 75% 100%, 25% 100%, 25% 62.5%, 0 62.5%, 0 25%, 25% 25%)";

function Figure({ thumbUrl }: { thumbUrl: string | null }) {
  const [loaded, setLoaded] = useState(false);
  // A new picture for the same tile (another arm model) fades in again.
  const [shown, setShown] = useState(thumbUrl);
  if (shown !== thumbUrl) {
    setShown(thumbUrl);
    setLoaded(false);
  }
  return (
    <span aria-hidden className="relative flex h-[96px] w-[48px] shrink-0 justify-center">
      <span className="absolute -bottom-1.5 h-2.5 w-11 rounded-[50%] bg-[radial-gradient(closest-side,rgb(10_9_12/0.55),transparent)]" />
      {!thumbUrl || !loaded ? (
        <span
          className="absolute inset-0 overflow-hidden bg-obsidian-800/80"
          style={{ clipPath: FIGURE_CLIP }}
        >
          <span className="absolute inset-0 animate-shimmer bg-[linear-gradient(90deg,transparent,rgb(255_255_255/0.05),transparent)]" />
        </span>
      ) : null}
      {thumbUrl ? (
        <img
          src={thumbUrl}
          alt=""
          width={48}
          height={96}
          draggable={false}
          ref={(image) => {
            if (image?.complete && image.naturalWidth > 0) setLoaded(true);
          }}
          onLoad={() => setLoaded(true)}
          className={cn(
            "relative transition-[opacity,transform] duration-200 ease-out [image-rendering:pixelated]",
            "group-hover/tile:-translate-y-0.5 motion-reduce:group-hover/tile:translate-y-0",
            loaded ? "opacity-100" : "opacity-0",
          )}
        />
      ) : null}
    </span>
  );
}

function HistoryTile({
  entry,
  index,
  now,
  current,
  pending,
  tabbable,
  wearBlock,
  layout,
  register,
  onWear,
  onFavorite,
  onRemove,
  onKeyDown,
  onFocus,
}: {
  entry: SkinHistoryEntry;
  index: number;
  now: number;
  current: boolean;
  pending: boolean;
  tabbable: boolean;
  wearBlock: WearBlock;
  layout: boolean;
  register: (element: HTMLButtonElement | null) => void;
  onWear: (entry: SkinHistoryEntry) => void;
  onFavorite: (entry: SkinHistoryEntry) => void;
  onRemove: (entry: SkinHistoryEntry, index: number) => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>, index: number) => void;
  onFocus: (index: number) => void;
}) {
  const { t } = useTranslation();
  const when = formatRelative(entry.lastWorn, now);
  const worn = current || pending;
  const wearable = !worn && wearBlock === null;
  const label = pending
    ? t("skins.history.wearingLabel")
    : current
      ? t("skins.history.currentLabel")
      : t("skins.history.wearLabel", { when });
  return (
    <motion.li
      layout={layout ? "position" : false}
      initial={{ opacity: 0, scale: 0.96 }}
      animate={{ opacity: 1, scale: 1, transition: { duration: 0.2, ease: easeOutExpo } }}
      exit={{ opacity: 0, scale: 0.96, transition: { duration: 0.14, ease: easeOutExpo } }}
      className="group/item relative"
    >
      {worn ? <PixelBrackets size={8} /> : null}
      <button
        ref={register}
        type="button"
        tabIndex={tabbable ? 0 : -1}
        aria-label={label}
        aria-disabled={wearable ? undefined : true}
        onClick={() => {
          if (wearable) onWear(entry);
        }}
        onKeyDown={(event) => onKeyDown(event, index)}
        onFocus={() => onFocus(index)}
        className={cn(
          "group/tile relative flex w-full flex-col items-center rounded-[var(--radius-panel)] pt-3 select-none",
          "transition-[background-color,box-shadow,transform] duration-150 ease-out",
          "focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none",
          worn
            ? "bg-gold-500/12 ring-1 ring-gold-400/50"
            : "bg-obsidian-950/35 ring-1 ring-white/7",
          wearable
            ? "cursor-pointer hover:bg-white/5 hover:ring-white/15 active:scale-[0.97]"
            : "cursor-default",
        )}
        style={{ height: TILE_H }}
      >
        <Figure thumbUrl={entry.thumbUrl} />
        <span className="relative mt-[10px] flex h-[26px] w-full items-center justify-center px-2">
          {worn ? (
            <span className="truncate text-[12px] font-semibold text-gold-200">
              {pending ? t("skins.history.wearing") : t("skins.history.current")}
            </span>
          ) : (
            <>
              <span
                className={cn(
                  "truncate text-[12px] text-ash-400 tabular-nums transition-opacity duration-150",
                  wearBlock === null || wearBlock === "cooldown"
                    ? "group-hover/tile:opacity-0 group-focus-visible/tile:opacity-0"
                    : null,
                )}
              >
                {when}
              </span>
              {wearBlock === null ? (
                <span
                  aria-hidden
                  className={cn(
                    "chamfer absolute inset-x-2 inset-y-0 flex items-center justify-center bg-[linear-gradient(180deg,var(--color-gold-300),var(--color-gold-500)_55%,var(--color-gold-600))] text-[12.5px] font-semibold text-gold-ink [--cut:6px]",
                    "translate-y-0.5 opacity-0 transition-[opacity,transform] duration-150 ease-out motion-reduce:translate-y-0",
                    "group-hover/tile:translate-y-0 group-hover/tile:opacity-100 group-focus-visible/tile:translate-y-0 group-focus-visible/tile:opacity-100",
                  )}
                >
                  {t("skins.history.wear")}
                </span>
              ) : wearBlock === "cooldown" ? (
                <span
                  aria-hidden
                  className="absolute inset-0 flex items-center justify-center truncate px-2 text-[12px] font-medium text-ash-300 opacity-0 transition-opacity duration-150 group-hover/tile:opacity-100 group-focus-visible/tile:opacity-100"
                >
                  {t("skins.history.coolingDown")}
                </span>
              ) : null}
            </>
          )}
        </span>
      </button>
      <IconButton
        size="sm"
        tabIndex={tabbable ? 0 : -1}
        label={entry.favorite ? t("skins.history.unfavorite") : t("skins.history.favorite")}
        icon={<Star size={15} weight={entry.favorite ? "fill" : "regular"} />}
        onClick={() => onFavorite(entry)}
        className={cn(
          "absolute top-1 right-1 transition-[opacity,background-color,color,transform] duration-150",
          entry.favorite
            ? "text-gold-300 hover:text-gold-200"
            : "opacity-0 group-focus-within/item:opacity-100 group-hover/item:opacity-100",
        )}
      />
      {worn ? null : (
        <IconButton
          size="sm"
          tabIndex={tabbable ? 0 : -1}
          label={t("skins.history.remove")}
          icon={<X size={15} />}
          onClick={() => onRemove(entry, index)}
          className="absolute top-1 left-1 opacity-0 transition-[opacity,background-color,color,transform] duration-150 group-focus-within/item:opacity-100 group-hover/item:opacity-100"
        />
      )}
    </motion.li>
  );
}

/**
 * The "Önceki skinler" section of the appearance panel. Hidden while the account has worn no
 * other skin; once shown during a visit it stays, so removing the last old skin does not make
 * the panel jump. The order holds still while the panel is open (a worn or favourite skin does
 * not run off to another page); the next visit sorts favourites first, then the most recently
 * worn. Memoized: the panel re-renders while the big preview changes skins, the grid need not.
 */
export const SkinHistorySection = memo(function SkinHistorySection({
  accountId,
  history,
  currentId,
  pendingId,
  wearBlock,
  onWear,
}: {
  accountId: string;
  history: SkinHistory;
  /** The skin worn now as far as the panel knows (one being put on counts). */
  currentId: string | null;
  /** The skin being put on right now. */
  pendingId: string | null;
  wearBlock: WearBlock;
  /** Stable across renders (the grid is memoized). */
  onWear: (entry: SkinHistoryEntry) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const prefs = useMotionPrefs();
  const ids = useId();
  const sectionRef = useRef<HTMLElement>(null);

  // The order of this visit (see `steadyOrder`), derived again whenever the entries change.
  const entryIds = history.entries.map((entry) => entry.id);
  const idsKey = entryIds.join(",");
  const [order, setOrder] = useState<{ key: string; ids: string[] }>(() => ({
    key: idsKey,
    ids: entryIds,
  }));
  if (order.key !== idsKey) setOrder({ key: idsKey, ids: steadyOrder(order.ids, entryIds) });
  const byId = new Map(history.entries.map((entry) => [entry.id, entry]));
  const list = order.ids.flatMap((id) => byId.get(id) ?? []);

  const [stuck, setStuck] = useState(false);
  const worthShowing = hasPreviousSkins(entryIds, currentId);
  useEffect(() => {
    if (worthShowing) setStuck(true);
  }, [worthShowing]);

  const [page, setPage] = useState(0);
  const [direction, setDirection] = useState(1);
  const [focusIndex, setFocusIndex] = useState(0);
  const pages = pageCount(list.length);
  const shownPage = clampPage(page, list.length);
  const tabStop = pageOf(focusIndex) === shownPage ? focusIndex : shownPage * PAGE_SIZE;

  const tiles = useRef(new Map<string, HTMLButtonElement>());
  const focusNext = useRef<string | null>(null);
  useEffect(() => {
    const key = focusNext.current;
    if (!key) return;
    const tile = tiles.current.get(key);
    if (tile) {
      focusNext.current = null;
      tile.focus({ preventScroll: true });
    }
  });

  const goToPage = useCallback(
    (next: number, focus?: number) => {
      const target = clampPage(next, list.length);
      setDirection(target >= shownPage ? 1 : -1);
      setPage(target);
      const index = focus ?? target * PAGE_SIZE;
      setFocusIndex(index);
      const entry = list[index];
      if (focus !== undefined && entry) focusNext.current = `${target}:${entry.id}`;
    },
    [list, shownPage],
  );

  const setEntries = useCallback(
    (change: (entries: SkinHistoryEntry[]) => SkinHistoryEntry[]) =>
      queryClient.setQueryData<SkinHistory>(historyKey(accountId), (old) =>
        old ? { ...old, entries: change(old.entries) } : old,
      ),
    [accountId, queryClient],
  );

  const favorite = (entry: SkinHistoryEntry) => {
    const next = !entry.favorite;
    const set = (value: boolean) =>
      setEntries((entries) =>
        entries.map((other) => (other.id === entry.id ? { ...other, favorite: value } : other)),
      );
    set(next);
    api.skins.setHistoryFavorite(accountId, entry.id, next).catch(() => {
      set(!next);
      toast.error(t("skins.history.favoriteFailed"));
    });
  };

  const lastRemoval = useRef<Removal | null>(null);

  const undo = useCallback(
    (removal: Removal) => {
      if (lastRemoval.current === removal) lastRemoval.current = null;
      toast.dismiss(removal.toastId);
      setOrder((current) => ({
        ...current,
        ids: insertAt(current.ids, removal.entry.id, removal.index),
      }));
      setEntries((entries) =>
        entries.some((other) => other.id === removal.entry.id)
          ? entries
          : [...entries, removal.entry],
      );
      const back = pageOf(removal.index);
      setDirection(back >= shownPage ? 1 : -1);
      setPage(back);
      setFocusIndex(removal.index);
      focusNext.current = `${back}:${removal.entry.id}`;
      api.skins.restoreToHistory(accountId, removal.entry.id).catch(() => {
        toast.error(t("skins.history.undoFailed"));
        void queryClient.invalidateQueries({ queryKey: historyKey(accountId) });
      });
    },
    [accountId, queryClient, setEntries, shownPage, t],
  );

  const remove = (entry: SkinHistoryEntry, index: number) => {
    const rest = list.filter((other) => other.id !== entry.id);
    setOrder((current) => ({ ...current, ids: current.ids.filter((id) => id !== entry.id) }));
    setEntries((entries) => entries.filter((other) => other.id !== entry.id));
    // The focus stays in the grid: on the skin that took this one's place, else the one before.
    const nextIndex = Math.min(index, rest.length - 1);
    const nextPage = clampPage(pageOf(Math.max(0, nextIndex)), rest.length);
    if (nextPage !== shownPage) {
      setDirection(-1);
      setPage(nextPage);
    }
    setFocusIndex(Math.max(0, nextIndex));
    const neighbour = rest[nextIndex];
    focusNext.current = neighbour ? `${nextPage}:${neighbour.id}` : null;

    const removal: Removal = { entry, index, toastId: "", at: Date.now() };
    removal.toastId = showRemovedToast(entry.thumbUrl, () => undo(removal));
    lastRemoval.current = removal;
    api.skins.removeFromHistory(accountId, entry.id).catch(() => {
      toast.dismiss(removal.toastId);
      if (lastRemoval.current === removal) lastRemoval.current = null;
      setOrder((current) => ({ ...current, ids: insertAt(current.ids, entry.id, index) }));
      setEntries((entries) => [...entries, entry]);
      toast.error(t("skins.history.removeFailed"));
    });
  };

  // Ctrl+Z puts back the last skin removed, while its toast still offers it (as in the account
  // center), when the focus is in this panel.
  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "z") return;
      const removal = lastRemoval.current;
      if (!removal || Date.now() - removal.at > HISTORY_UNDO_MS) return;
      const panel = sectionRef.current?.closest("aside");
      const target = event.target as HTMLElement | null;
      if (!panel?.contains(document.activeElement) || target?.closest("input, textarea")) return;
      event.preventDefault();
      undo(removal);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo]);

  const onTileKey = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const entry = list[index];
    if (!entry) return;
    if (event.key === "Delete" || event.key === "Backspace") {
      if (entry.id !== currentId && entry.id !== pendingId) {
        event.preventDefault();
        remove(entry, index);
      }
      return;
    }
    if (
      ![
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "ArrowDown",
        "Home",
        "End",
        "PageUp",
        "PageDown",
      ].includes(event.key)
    )
      return;
    event.preventDefault();
    const next = moveFocus(index, event.key, list.length);
    if (next === null) return;
    const nextPage = pageOf(next);
    if (nextPage !== shownPage) {
      goToPage(nextPage, next);
      return;
    }
    setFocusIndex(next);
    const target = list[next];
    if (target) tiles.current.get(`${shownPage}:${target.id}`)?.focus({ preventScroll: true });
  };

  if (!worthShowing && !stuck) return null;

  const rows = reservedRows(list.length);
  const height = rows * TILE_H + (rows - 1) * GAP;
  const distance = prefs.layout ? 32 : 0;
  const now = Date.now();

  return (
    <section ref={sectionRef} aria-labelledby={`${ids}-title`} className="mt-8">
      <div className="mb-3 flex min-h-8 items-center justify-between gap-4">
        <h3
          id={`${ids}-title`}
          className="font-display text-[17px] leading-tight font-semibold text-bone-50"
        >
          {t("skins.history.title")}
        </h3>
        {pages > 1 ? (
          <div className="flex shrink-0 items-center gap-1">
            <IconButton
              size="sm"
              label={t("skins.history.previousPage")}
              icon={<CaretLeft size={15} weight="bold" />}
              aria-disabled={shownPage === 0 || undefined}
              onClick={() => {
                if (shownPage > 0) goToPage(shownPage - 1);
              }}
              className="aria-disabled:cursor-default aria-disabled:opacity-35 aria-disabled:hover:bg-transparent"
            />
            <span
              aria-hidden
              className="min-w-[44px] text-center text-[12.5px] text-ash-300 tabular-nums"
            >
              {shownPage + 1} / {pages}
            </span>
            <span className="sr-only" aria-live="polite">
              {t("skins.history.pageOf", { page: shownPage + 1, pages })}
            </span>
            <IconButton
              size="sm"
              label={t("skins.history.nextPage")}
              icon={<CaretRight size={15} weight="bold" />}
              aria-disabled={shownPage === pages - 1 || undefined}
              onClick={() => {
                if (shownPage < pages - 1) goToPage(shownPage + 1);
              }}
              className="aria-disabled:cursor-default aria-disabled:opacity-35 aria-disabled:hover:bg-transparent"
            />
          </div>
        ) : (
          <span className="shrink-0 text-[12.5px] text-ash-400">
            {t("skins.history.count", { count: list.length })}
          </span>
        )}
      </div>
      <div
        className="relative overflow-hidden"
        style={{ height: height + 2 * BLEED, margin: -BLEED, padding: BLEED }}
      >
        <AnimatePresence initial={false} custom={direction}>
          <motion.ul
            key={shownPage}
            custom={direction}
            variants={{
              enter: (dir: number) => ({
                opacity: 0,
                transform: `translateX(${dir * distance}px)`,
              }),
              center: {
                opacity: 1,
                transform: "translateX(0px)",
                transition: { duration: 0.24, ease: easeOutExpo },
              },
              exit: (dir: number) => ({
                opacity: 0,
                transform: `translateX(${-dir * distance}px)`,
                transition: { duration: 0.16, ease: easeOutExpo },
              }),
            }}
            initial="enter"
            animate="center"
            exit="exit"
            aria-label={t("skins.history.title")}
            className="absolute grid grid-cols-5 content-start"
            style={{ inset: BLEED, gap: GAP }}
          >
            <AnimatePresence initial={false} mode="popLayout">
              {pageItems(list, shownPage).map((entry, slot) => {
                const index = shownPage * PAGE_SIZE + slot;
                return (
                  <HistoryTile
                    key={entry.id}
                    entry={entry}
                    index={index}
                    now={now}
                    current={entry.id === currentId}
                    pending={entry.id === pendingId}
                    tabbable={index === tabStop}
                    wearBlock={wearBlock}
                    layout={prefs.layout}
                    register={(element) => {
                      const key = `${shownPage}:${entry.id}`;
                      if (element) tiles.current.set(key, element);
                      else tiles.current.delete(key);
                    }}
                    onWear={onWear}
                    onFavorite={favorite}
                    onRemove={remove}
                    onKeyDown={onTileKey}
                    onFocus={setFocusIndex}
                  />
                );
              })}
            </AnimatePresence>
          </motion.ul>
        </AnimatePresence>
      </div>
    </section>
  );
});
