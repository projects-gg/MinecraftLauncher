// The emoji and sticker picker's panel (CHAT2, docs: chat2.md §3.1, §8.5), loaded on first use
// with its data and the embedded emoji font. 336 × 384 px, two tabs under a segmented control:
// - Emoji: search ("Emoji ara") with the skin tone beside it, "Son kullanılanlar" (24) and the
//   nine groups in an 8-column grid of 36 px cells, the group rail below (it follows the scroll;
//   a press jumps), and at the bottom the name of the emoji under the pointer or the keyboard. A
//   pick goes into the message and the panel stays open (emoji come in runs).
// - Stickers: "Son kullanılanlar" (8) and the four themes, 4 columns of 72 px; a pick sends the
//   sticker and closes the panel. A sticker moves once when the pointer comes onto it.
// The recents shown are those of the moment the panel opened, so a pick never shifts the cells
// under the pointer. Tabs cross-fade in place; the segmented plate is the one thing that moves.
import "@/assets/fonts/emoji/emoji.css";
import "./picker.css";
import {
  AirplaneTilt,
  ClockCounterClockwise,
  Flag,
  Hamburger,
  HandWaving,
  Lightbulb,
  PawPrint,
  Shapes,
  Smiley,
  SoccerBall,
} from "@phosphor-icons/react";
import { STICKER_SETS, type Sticker as StickerData } from "@pl/stickers";
import { cn, easeOutExpo, enter, SearchInput, Segmented, Skeleton, Tooltip } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  type KeyboardEvent,
  memo,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import {
  displayName,
  EMOJI_GROUPS,
  type EmojiData,
  type EmojiEntry,
  type EmojiGroup,
  type EmojiLanguage,
  loadEmojiData,
} from "./data";
import { type GridSection, PickerGrid, type PickerGridHandle } from "./picker-grid";
import { PANEL_HEIGHT, PANEL_WIDTH, type PickerTab, SKELETON_KEYS } from "./picker-layout";
import { recentEntries } from "./recents";
import { searchEmoji } from "./search";
import { Sticker } from "./sticker";
import { useEmojiPrefs } from "./store";
import { withTone } from "./tone";
import { TonePicker } from "./tone-picker";

export interface PickerPanelProps {
  /** The sticker tab is there (the service takes stickers). */
  stickers: boolean;
  initialTab?: PickerTab;
  onEmoji: (emoji: string) => void;
  onSticker: (sticker: StickerData) => void;
  /** The search field, for the layer to focus when it opens. */
  searchRef?: (node: HTMLInputElement | null) => void;
}

/** The rows of the panel, in pixels: tabs, search, rail, footer. */
const TABS = 42;
const SEARCH = 48;
const RAIL = 36;
const FOOTER = 38;
const GUTTER = 10;
const EMOJI_COLUMNS = 8;
const EMOJI_CELL = 36;
const STICKER_COLUMNS = 4;
const STICKER_CELL = 72;
const HEADER = 28;

const GROUP_ICONS: Record<EmojiGroup | "recent", typeof Smiley> = {
  recent: ClockCounterClockwise,
  smileys: Smiley,
  people: HandWaving,
  nature: PawPrint,
  food: Hamburger,
  travel: AirplaneTilt,
  activities: SoccerBall,
  objects: Lightbulb,
  symbols: Shapes,
  flags: Flag,
};

/** A tab replacing the other: the old one out fast, the new one in a beat later. */
const crossfade = {
  initial: { opacity: 0 },
  animate: { opacity: 1, transition: { ...enter(0.18), delay: 0.04 } },
  exit: { opacity: 0, transition: { duration: 0.1, ease: easeOutExpo } },
};

const languageOf = (lang: string): EmojiLanguage => (lang.startsWith("en") ? "en" : "tr");

export default function PickerPanel({
  stickers,
  initialTab = "emoji",
  onEmoji,
  onSticker,
  searchRef,
}: PickerPanelProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<PickerTab>(stickers ? initialTab : "emoji");
  return (
    <div className="flex h-full flex-col">
      {stickers ? (
        <div className="flex shrink-0 items-end px-2.5" style={{ height: TABS }}>
          <Segmented
            value={tab}
            onValueChange={setTab}
            label={t("chat2.picker.tabs.label")}
            options={[
              { value: "emoji", label: t("chat2.picker.tabs.emoji") },
              { value: "stickers", label: t("chat2.picker.tabs.stickers") },
            ]}
          />
        </div>
      ) : null}
      <div className="relative min-h-0 flex-1">
        <AnimatePresence initial={false}>
          <motion.div key={tab} className="absolute inset-0 flex flex-col" {...crossfade}>
            {tab === "emoji" ? (
              <EmojiTab onEmoji={onEmoji} searchRef={searchRef} top={stickers ? TABS : 0} />
            ) : (
              <StickerTab onSticker={onSticker} />
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}

/** The bottom line: the emoji or sticker under the pointer or the keyboard, with its name. */
function Footer({ glyph, name }: { glyph: ReactNode; name: string }) {
  return (
    <div
      className="flex shrink-0 items-center gap-2.5 border-t border-white/6 px-3.5"
      style={{ height: FOOTER }}
    >
      <span aria-hidden className="flex size-[26px] shrink-0 items-center justify-center">
        {glyph}
      </span>
      <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-bone-100">{name}</span>
    </div>
  );
}

function EmojiTab({
  onEmoji,
  searchRef,
  top,
}: {
  onEmoji: (emoji: string) => void;
  searchRef?: (node: HTMLInputElement | null) => void;
  /** Height above the tab (the segmented control, when there are stickers). */
  top: number;
}) {
  const { t, i18n } = useTranslation();
  const lang = languageOf(i18n.language);
  const [data, setData] = useState<EmojiData | null>(null);
  const [query, setQuery] = useState("");
  const [preview, setPreview] = useState<EmojiEntry | null>(null);
  const [section, setSection] = useState("");
  const tone = useEmojiPrefs((s) => s.tone);
  const setTone = useEmojiPrefs((s) => s.setTone);
  const usedEmoji = useEmojiPrefs((s) => s.usedEmoji);
  // The recents of the moment the panel opened (a pick never moves the cells under the pointer).
  const [recents] = useState(() => useEmojiPrefs.getState().recentEmoji);
  const grid = useRef<PickerGridHandle>(null);
  const search = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    let alive = true;
    void loadEmojiData(lang).then((loaded) => {
      if (alive) setData(loaded);
    });
    return () => {
      alive = false;
    };
  }, [lang]);

  const searching = query.trim().length > 0;
  const browse = useMemo((): GridSection<EmojiEntry>[] => {
    if (!data) return [];
    const recent = recentEntries(data, recents);
    const groups = EMOJI_GROUPS.map((group) => ({
      id: group,
      title: t(`chat2.groups.${group}`),
      items: data.entries.filter((entry) => entry.group === group),
    }));
    return recent.length
      ? [{ id: "recent", title: t("chat2.picker.recent"), items: recent }, ...groups]
      : groups;
  }, [data, recents, t]);
  const sections = useMemo(
    (): GridSection<EmojiEntry>[] =>
      data && searching
        ? [{ id: "results", title: null, items: searchEmoji(data, query) }]
        : browse,
    [data, query, searching, browse],
  );

  const toneBases = data?.toneBases;
  const glyph = useCallback(
    (entry: EmojiEntry) => (toneBases ? withTone(entry, tone, toneBases) : entry.emoji),
    [tone, toneBases],
  );

  const pick = useCallback(
    (entry: EmojiEntry, cell: HTMLElement | null) => {
      onEmoji(glyph(entry));
      usedEmoji(entry.emoji);
      // A small press in the cell: the pick landed (the panel stays open). Added on top of the
      // hover's lift, so it ends where the lift is instead of jumping to it.
      cell
        ?.querySelector(".emoji-glyph")
        ?.animate(
          [
            { transform: "scale(0.84)" },
            { transform: "scale(1.08)", offset: 0.55 },
            { transform: "scale(1)" },
          ],
          { duration: 240, easing: "cubic-bezier(0.22, 1, 0.36, 1)", composite: "add" },
        );
    },
    [glyph, onEmoji, usedEmoji],
  );

  const toSearch = useCallback(() => search.current?.focus({ preventScroll: true }), []);
  const onSearchKey = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== search.current) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      grid.current?.focus();
    } else if (event.key === "Enter" && !event.nativeEvent.isComposing) {
      // Enter in the search puts in the best result.
      event.preventDefault();
      const first = searching ? sections[0]?.items[0] : undefined;
      if (first) pick(first, document.getElementById("emoji-grid-0-0"));
    }
  };

  const height = PANEL_HEIGHT - top - SEARCH - RAIL - FOOTER;
  // The rail stays while searching (a press leaves the search for the group).
  const railIds = useMemo(() => browse.map((s) => s.id), [browse]);
  // Nothing under the pointer: the first emoji of the group in view.
  const atTop = searching ? sections[0] : (sections.find((s) => s.id === section) ?? sections[0]);
  const shown = preview ?? atTop?.items[0] ?? null;
  const results = searching ? (sections[0]?.items.length ?? 0) : null;

  return (
    <>
      {/* biome-ignore lint/a11y/noStaticElementInteractions: the keys come up from the search field in it */}
      <div
        className="flex shrink-0 items-center gap-2 px-2.5"
        style={{ height: SEARCH }}
        onKeyDown={onSearchKey}
      >
        <SearchInput
          ref={(node) => {
            search.current = node;
            searchRef?.(node);
          }}
          value={query}
          onChange={setQuery}
          placeholder={t("chat2.picker.search")}
          label={t("chat2.picker.search")}
          clearLabel={t("chat2.picker.clear")}
          className="min-w-0 flex-1"
        />
        <TonePicker tone={tone} onTone={setTone} />
      </div>
      <div className="relative min-h-0 px-2.5" style={{ height }}>
        {data ? (
          <EmojiGrid
            gridRef={grid}
            sections={sections}
            height={height}
            glyph={glyph}
            lang={lang}
            label={t("chat2.picker.grid")}
            emptyText={t("chat2.picker.empty")}
            onPick={pick}
            onPreview={setPreview}
            onSection={setSection}
            onLeaveTop={toSearch}
          />
        ) : (
          <GridSkeleton columns={EMOJI_COLUMNS} cell={EMOJI_CELL} height={height} />
        )}
        <p className="sr-only" aria-live="polite">
          {results === null ? "" : t("chat2.picker.results", { count: results })}
        </p>
      </div>
      <Rail
        ids={railIds}
        active={searching ? null : section}
        onJump={(id) => {
          if (searching) setQuery("");
          requestAnimationFrame(() => grid.current?.scrollToSection(id));
        }}
      />
      <Footer
        glyph={shown ? <span className="emoji-glyph text-[21px]">{glyph(shown)}</span> : null}
        name={shown ? displayName(shown.name, lang) : ""}
      />
    </>
  );
}

const EmojiGrid = memo(function EmojiGrid({
  gridRef,
  sections,
  height,
  glyph,
  lang,
  label,
  emptyText,
  onPick,
  onPreview,
  onSection,
  onLeaveTop,
}: {
  gridRef: { current: PickerGridHandle | null };
  sections: readonly GridSection<EmojiEntry>[];
  height: number;
  glyph: (entry: EmojiEntry) => string;
  lang: EmojiLanguage;
  label: string;
  emptyText: string;
  onPick: (entry: EmojiEntry, cell: HTMLElement | null) => void;
  onPreview: (entry: EmojiEntry | null) => void;
  onSection: (id: string) => void;
  onLeaveTop: () => void;
}) {
  return (
    <PickerGrid<EmojiEntry>
      ref={gridRef}
      id="emoji-grid"
      label={label}
      sections={sections}
      columns={EMOJI_COLUMNS}
      cellSize={EMOJI_CELL}
      headerSize={HEADER}
      height={height}
      itemKey={(entry) => entry.emoji}
      itemLabel={(entry) => displayName(entry.name, lang)}
      renderItem={(entry) => <span className="emoji-glyph text-[22px]">{glyph(entry)}</span>}
      onPick={onPick}
      onPreview={onPreview}
      onSection={onSection}
      onLeaveTop={onLeaveTop}
      onTypeAhead={onLeaveTop}
      empty={<p className="text-[13px] text-ash-300">{emptyText}</p>}
    />
  );
});

/**
 * The groups below the grid (a toolbar: one Tab stop, the arrows move): the group at the top of
 * the view lights and a gold mark glides under it; a press jumps to the group.
 */
function Rail({
  ids,
  active,
  onJump,
}: {
  ids: readonly string[];
  active: string | null;
  onJump: (id: string) => void;
}) {
  const { t } = useTranslation();
  const bar = useRef<HTMLDivElement>(null);
  const [focus, setFocus] = useState(0);
  const index = active ? ids.indexOf(active) : -1;
  const width = ids.length ? (PANEL_WIDTH - GUTTER * 2) / ids.length : 0;
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const delta = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? ids.length - 1
          : delta
            ? (focus + delta + ids.length) % ids.length
            : null;
    if (next === null) return;
    event.preventDefault();
    setFocus(next);
    bar.current?.querySelectorAll<HTMLButtonElement>("button")[next]?.focus();
  };
  return (
    <div
      ref={bar}
      role="toolbar"
      aria-label={t("chat2.picker.groups")}
      onKeyDown={onKeyDown}
      className="relative flex shrink-0 items-center px-2.5"
      style={{ height: RAIL }}
    >
      {ids.map((id, at) => {
        const Icon = GROUP_ICONS[id as EmojiGroup | "recent"];
        const name = id === "recent" ? t("chat2.picker.recent") : t(`chat2.groups.${id}`);
        const on = id === active;
        return (
          <Tooltip key={id} content={name} side="top">
            <button
              type="button"
              aria-label={name}
              aria-current={on ? "true" : undefined}
              tabIndex={at === focus ? 0 : -1}
              onFocus={() => setFocus(at)}
              onClick={() => onJump(id)}
              className={cn(
                "relative flex h-[30px] flex-1 cursor-pointer items-center justify-center rounded-md outline-none",
                "transition-[color,background-color,scale] duration-150 hover:bg-white/6 hover:text-bone-100 active:scale-[0.92]",
                "focus-visible:ring-2 focus-visible:ring-gold-300",
                on ? "text-gold-300" : "text-ash-400",
              )}
            >
              {Icon ? <Icon size={17} weight={on ? "duotone" : "regular"} aria-hidden /> : null}
            </button>
          </Tooltip>
        );
      })}
      <span
        aria-hidden
        className="rail-marker pointer-events-none absolute bottom-0 h-[2px] rounded-full bg-gold-400"
        style={{
          left: GUTTER,
          width: Math.max(0, width - 18),
          transform: `translateX(${Math.max(0, index) * width + 9}px)`,
          opacity: index >= 0 ? 1 : 0,
        }}
      />
    </div>
  );
}

/** The grid's place while the data loads (a few milliseconds after a first open). */
function GridSkeleton({
  columns,
  cell,
  height,
}: {
  columns: number;
  cell: number;
  height: number;
}) {
  const count = Math.floor((height - HEADER) / cell) * columns;
  return (
    <div aria-hidden className="overflow-hidden" style={{ height }}>
      <div className="flex items-end px-1.5 pb-1.5" style={{ height: HEADER }}>
        <Skeleton className="h-2.5 w-28" />
      </div>
      <div className="grid" style={{ gridTemplateColumns: `repeat(${columns}, ${cell}px)` }}>
        {SKELETON_KEYS.slice(0, count).map((key) => (
          <div key={key} className="flex items-center justify-center" style={{ height: cell }}>
            <Skeleton className="size-[22px] rounded-full" />
          </div>
        ))}
      </div>
    </div>
  );
}

function StickerTab({ onSticker }: { onSticker: (sticker: StickerData) => void }) {
  const { t, i18n } = useTranslation();
  const lang = languageOf(i18n.language);
  const [preview, setPreview] = useState<StickerData | null>(null);
  const usedSticker = useEmojiPrefs((s) => s.usedSticker);
  const [recents] = useState(() => useEmojiPrefs.getState().recentStickers);

  const sections = useMemo((): GridSection<StickerData>[] => {
    const all = STICKER_SETS.flatMap((set) => set.stickers);
    const byRef = new Map(all.map((sticker) => [sticker.ref, sticker]));
    const recent = recents
      .map((ref) => byRef.get(ref))
      .filter((sticker): sticker is StickerData => Boolean(sticker));
    const themes = STICKER_SETS.flatMap((set) =>
      set.themes.map((theme) => ({
        id: `${set.set}-${theme.id}`,
        title: theme.name[lang],
        items: set.stickers.filter((sticker) => sticker.theme === theme.id),
      })),
    );
    return recent.length
      ? [{ id: "recent", title: t("chat2.stickers.recent"), items: recent }, ...themes]
      : themes;
  }, [recents, lang, t]);

  const onPick = useCallback(
    (sticker: StickerData) => {
      usedSticker(sticker.ref);
      onSticker(sticker);
    },
    [onSticker, usedSticker],
  );
  const shown = preview ?? sections[0]?.items[0] ?? null;
  const height = PANEL_HEIGHT - TABS - FOOTER - 6;

  return (
    <>
      <div className="relative min-h-0 px-2.5 pt-1.5" style={{ height: height + 6 }}>
        <PickerGrid<StickerData>
          id="sticker-grid"
          label={t("chat2.stickers.grid")}
          sections={sections}
          columns={STICKER_COLUMNS}
          cellSize={STICKER_CELL}
          headerSize={HEADER}
          height={height}
          itemKey={(sticker) => sticker.ref}
          itemLabel={(sticker) => t("chat2.stickers.label", { name: sticker.name[lang] })}
          renderItem={(sticker) => <Sticker sticker={sticker} size={64} />}
          onPick={onPick}
          onPreview={setPreview}
        />
      </div>
      <Footer
        glyph={shown ? <Sticker sticker={shown} size={26} moveOnHover={false} /> : null}
        name={shown ? shown.name[lang] : ""}
      />
    </>
  );
}
