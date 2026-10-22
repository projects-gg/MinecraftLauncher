import { Gauge, Package, Sparkle, Sword } from "@phosphor-icons/react";
import { Chip, cn, PixelBrackets, Skeleton, Switch, Tooltip } from "@pl/ui";
import { type KeyboardEvent, type ReactNode, type Ref, useRef } from "react";
import { useTranslation } from "react-i18next";
import { ProjectIcon } from "@/features/content/parts";
import type { LoaderKind, SetBadge, SetCard, SetItem, SetPurpose, SetVariant } from "@/lib/api";
import { formatBytes, listFormat } from "@/lib/format";
import { ContentIcon, type ContentIconKind, spriteSize } from "@/shell/content-icon";
import {
  countWords,
  enabledItems,
  fitWords,
  iconStrip,
  neededBy,
  type SetFit,
  setName,
  setSummary,
  splitItems,
  type Words,
} from "./model";

/** A set's purpose on a chip (Phosphor, 12 px: a label, too small for a sprite). */
const marks: Record<SetPurpose, typeof Sword> = {
  pvp: Sword,
  performance: Gauge,
  visual: Sparkle,
  other: Package,
};

/** A set's purpose in its tile: our pixel sprites. */
const sprites: Record<SetPurpose, ContentIconKind> = {
  pvp: "pvp",
  performance: "performance",
  visual: "sparkle",
  other: "pack",
};

/** A set's purpose as a chamfered tile: its pixel mark, a gold step quieter until chosen. */
export function PurposeMark({
  purpose,
  active = false,
  size = 36,
  className,
}: {
  purpose: SetPurpose;
  active?: boolean;
  size?: number;
  className?: string;
}) {
  const kind = sprites[purpose];
  const px = spriteSize(size);
  return (
    <span
      aria-hidden
      className={cn(
        "chamfer relative flex shrink-0 items-center justify-center bg-white/7 [--cut:7px]",
        className,
      )}
      style={{ width: size, height: size }}
    >
      {/* Both tones stay mounted: the chosen one fades in, nothing reflows. */}
      <ContentIcon
        kind={kind}
        size={px}
        quiet
        className={cn("absolute transition-opacity duration-150", active && "opacity-0")}
      />
      <ContentIcon
        kind={kind}
        size={px}
        className={cn("absolute transition-opacity duration-150", !active && "opacity-0")}
      />
    </span>
  );
}

/** The words of a `Words` list, joined the language's way ("12 mod ve 3 kaynak paketi"). */
export function useWords() {
  const { t, i18n } = useTranslation();
  return {
    one: (w: Words) => t(w.key, w.values),
    list: (words: Words[]) => listFormat().format(words.map((w) => t(w.key, w.values))),
    english: i18n.language.startsWith("en"),
  };
}

const skeletonKeys = ["first", "second", "third", "fourth"];

/** Placeholder cards while the sets load (same size as a card, so nothing moves). */
export function SetCardSkeletons({ count = 2 }: { count?: number }) {
  return (
    <div aria-busy className="grid grid-cols-2 gap-3">
      {skeletonKeys.slice(0, count).map((key) => (
        <Skeleton key={key} className="h-[164px] w-full rounded-[var(--radius-panel)]" />
      ))}
    </div>
  );
}

/**
 * One ready-made set: what it is for (mark, name, summary), what it changes (item icons, counts)
 * and its size. As a choice it is a radio of `SetPicker`; a set that does not fit says why
 * instead of its counts.
 */
export function SetCardView({
  ref,
  set,
  variant,
  fit,
  selected = false,
  added = false,
  tabIndex,
  onSelect,
  onKeyDown,
}: {
  ref?: Ref<HTMLDivElement>;
  set: SetCard;
  /** The variant whose contents and size the card shows. */
  variant: SetVariant | undefined;
  fit?: SetFit;
  selected?: boolean;
  /** Already added to the instance (the add dialog). */
  added?: boolean;
  tabIndex?: number;
  onSelect?: () => void;
  onKeyDown?: (event: KeyboardEvent<HTMLDivElement>) => void;
}) {
  const { t } = useTranslation();
  const words = useWords();
  const loaderName = (loader: LoaderKind) => t(`loaders.${loader}`);
  const reason = fit ? fitWords(fit, loaderName) : null;
  const disabled = Boolean(reason) || added;
  const strip = variant ? iconStrip(variant) : { shown: [], more: 0 };
  const name = setName(set, words.english);
  const summary = setSummary(set, words.english);
  const versions = set.variants.map((v) => v.minecraft).join(", ");
  return (
    // biome-ignore lint/a11y/useSemanticElements: a card with rich content; the group handles arrows (APG radio group).
    <div
      ref={ref}
      role="radio"
      data-set-card={set.id}
      aria-checked={selected}
      aria-disabled={disabled || undefined}
      tabIndex={tabIndex}
      onClick={disabled ? undefined : onSelect}
      onKeyDown={onKeyDown}
      className={cn(
        "glass group/card relative flex min-h-[164px] flex-col rounded-[var(--radius-panel)] px-4 pt-3.5 pb-3 text-left outline-none",
        "transition-[scale,box-shadow] duration-150 focus-visible:ring-2 focus-visible:ring-gold-300",
        disabled ? "cursor-default" : "cursor-pointer active:scale-[0.985]",
      )}
    >
      {/* The chosen set: the brand's pixel brackets and a faint gold wash, faded in place. */}
      <span
        aria-hidden
        className={cn(
          "pointer-events-none absolute inset-0 rounded-[inherit] bg-gold-500/[0.07] transition-opacity duration-200",
          selected ? "opacity-100" : "opacity-0",
        )}
      />
      <span
        aria-hidden
        className={cn(
          "pointer-events-none absolute inset-0 rounded-[inherit] bg-white/[0.04] opacity-0 transition-opacity duration-150",
          !disabled && !selected && "group-hover/card:opacity-100",
        )}
      />
      <span
        className={cn("transition-opacity duration-150", selected ? "opacity-100" : "opacity-0")}
      >
        <PixelBrackets size={10} className="-inset-[3px]" />
      </span>

      <div className={cn("relative flex flex-1 flex-col", disabled && !added && "opacity-55")}>
        {/* What it is for and what it changes, then the size: read in that order. */}
        <div className="flex items-start gap-3">
          <PurposeMark purpose={set.purpose} active={selected} />
          <div className="min-w-0 flex-1">
            <p className="truncate font-display text-[17px] leading-tight font-semibold text-bone-50">
              {name}
            </p>
            <p className="mt-0.5 truncate text-[12.5px] text-ash-300">
              {reason ? words.one(reason) : variant ? words.list(countWords(variant)) : null}
            </p>
          </div>
          {added ? (
            <Chip tone="jade">{t("modSets.card.added")}</Chip>
          ) : variant && !reason ? (
            <span className="pt-0.5 font-display text-[13px] text-bone-100 tabular-nums">
              {formatBytes(variant.downloadBytes)}
            </span>
          ) : null}
        </div>
        {summary ? (
          <p className="mt-2.5 line-clamp-2 text-[13px] leading-[1.45] text-pretty text-ash-300">
            {summary}
          </p>
        ) : null}
        <div className="mt-auto flex min-h-[22px] items-center justify-between gap-3 pt-3">
          {strip.shown.length > 0 ? (
            <span
              className="flex min-w-0 items-center gap-1"
              role="img"
              aria-label={t("modSets.card.contents", {
                names: listFormat().format(strip.shown.map((item) => item.title)),
              })}
            >
              {strip.shown.map((item) => (
                <ProjectIcon key={item.id} src={item.iconUrl} size={22} className="[--cut:4px]" />
              ))}
              {strip.more > 0 ? (
                <span className="pl-0.5 text-[12px] text-ash-400 tabular-nums">
                  {t("modSets.card.more", { count: strip.more })}
                </span>
              ) : null}
            </span>
          ) : (
            <span />
          )}
          {/* The versions it is made for (said once: a card for other versions already says it). */}
          {fit?.kind === "otherVersion" ? null : (
            <span className="shrink-0 text-[12px] text-ash-400 tabular-nums">{versions}</span>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * The sets as a radio group: arrows (and Home/End) move the choice among the sets that can be
 * chosen, Tab leaves the group (APG radio group). Two columns.
 */
export function SetPicker({
  sets,
  value,
  onChange,
  label,
  variantOf,
  fitOf,
  addedIds,
}: {
  sets: SetCard[];
  value: string | null;
  onChange: (id: string) => void;
  label: string;
  /** The variant a card shows (its contents and size). */
  variantOf: (set: SetCard) => SetVariant | undefined;
  fitOf?: (set: SetCard) => SetFit;
  addedIds?: Set<string>;
}) {
  const refs = useRef(new Map<string, HTMLDivElement>());
  const choosable = sets.filter(
    (set) => (!fitOf || fitOf(set).kind === "fits") && !addedIds?.has(set.id),
  );
  const focusId = value && choosable.some((s) => s.id === value) ? value : choosable[0]?.id;
  const move = (from: string, step: number | "first" | "last") => {
    if (choosable.length === 0) return;
    const index = choosable.findIndex((s) => s.id === from);
    const next =
      step === "first"
        ? 0
        : step === "last"
          ? choosable.length - 1
          : (index + step + choosable.length) % choosable.length;
    const target = choosable[next];
    if (!target) return;
    onChange(target.id);
    refs.current.get(target.id)?.focus();
  };
  const onKey = (id: string) => (event: KeyboardEvent<HTMLDivElement>) => {
    const steps: Record<string, number | "first" | "last"> = {
      ArrowRight: 1,
      ArrowDown: 1,
      ArrowLeft: -1,
      ArrowUp: -1,
      Home: "first",
      End: "last",
    };
    const step = steps[event.key];
    if (step !== undefined) {
      event.preventDefault();
      move(id, step);
    } else if (event.key === " " || event.key === "Enter") {
      if (choosable.some((s) => s.id === id)) {
        event.preventDefault();
        onChange(id);
      }
    }
  };
  return (
    <div role="radiogroup" aria-label={label} className="grid grid-cols-2 gap-3">
      {sets.map((set) => (
        <SetCardView
          key={set.id}
          ref={(node) => {
            if (node) refs.current.set(set.id, node);
            else refs.current.delete(set.id);
          }}
          set={set}
          variant={variantOf(set)}
          fit={fitOf?.(set)}
          added={addedIds?.has(set.id)}
          selected={value === set.id}
          tabIndex={set.id === focusId ? 0 : -1}
          onSelect={() => onChange(set.id)}
          onKeyDown={onKey(set.id)}
        />
      ))}
    </div>
  );
}

/** One choosable item of a set with its switch. */
function ContentRow({
  item,
  on,
  needed,
  present,
  locked,
  onToggle,
}: {
  item: SetItem;
  on: boolean;
  /** Titles of the chosen items that need it (it stays on). */
  needed: string[];
  present: boolean;
  locked: boolean;
  onToggle: (on: boolean) => void;
}) {
  const { t } = useTranslation();
  const forced = needed.length > 0;
  return (
    <li className="group/row relative flex items-center gap-3 rounded-[8px] px-2.5 py-2">
      <span
        aria-hidden
        className="pointer-events-none absolute inset-0 rounded-[inherit] bg-white/5 opacity-0 transition-opacity duration-150 group-hover/row:opacity-100"
      />
      <ProjectIcon
        src={item.iconUrl}
        size={30}
        className={cn("relative [--cut:6px] transition-opacity duration-200", !on && "opacity-50")}
      />
      <div className="relative min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-[13.5px] font-semibold text-bone-50">{item.title}</span>
          {item.kind !== "mod" ? (
            <Chip className="shrink-0">{t(`modSets.kind.${item.kind}`)}</Chip>
          ) : null}
        </div>
        <p className="truncate text-[12.5px] text-ash-400">
          {present
            ? t("modSets.item.present")
            : forced && on
              ? t("modCenter.item.requiredBy", { names: needed.join(", ") })
              : item.summary}
        </p>
      </div>
      <span className="relative w-[64px] shrink-0 text-right text-[12px] text-ash-300 tabular-nums">
        {present ? null : formatBytes(item.sizeBytes)}
      </span>
      <Switch
        checked={on}
        onCheckedChange={onToggle}
        disabled={locked || present || (forced && on)}
        label={t("modCenter.item.toggle", { name: item.title })}
        hideLabel
        className="relative"
      />
    </li>
  );
}

/**
 * What a set brings, with a switch per item (the player's choices before adding it) and its
 * libraries on one quiet line. Items the instance already has are marked and stay as they are.
 */
export function SetContents({
  variant,
  choices,
  onToggle,
  present,
  locked,
  footer,
}: {
  variant: SetVariant;
  choices: Record<string, boolean>;
  onToggle: (id: string, on: boolean) => void;
  /** Item ids the instance already has from elsewhere. */
  present?: Set<string>;
  locked: boolean;
  footer?: ReactNode;
}) {
  const { t } = useTranslation();
  const { choices: rows, libraries } = splitItems(variant.items);
  const enabled = enabledItems(variant.items, choices);
  const comes = libraries.filter((item) => enabled.has(item.id));
  return (
    <div className="glass rounded-[var(--radius-panel)] py-1.5">
      {/* A long set scrolls inside its panel, under a soft edge; the dialog keeps its size. */}
      <div className="max-h-[min(300px,calc(100vh-470px))] overflow-y-auto px-1.5 [mask-image:linear-gradient(to_bottom,black_calc(100%-18px),transparent)] pb-2">
        <ul className="space-y-px">
          {rows.map((item) => (
            <ContentRow
              key={item.id}
              item={item}
              on={present?.has(item.id) ? true : enabled.has(item.id)}
              needed={neededBy(variant.items, enabled, item.id)}
              present={Boolean(present?.has(item.id))}
              locked={locked}
              onToggle={(on) => onToggle(item.id, on)}
            />
          ))}
        </ul>
      </div>
      {comes.length > 0 || footer ? (
        <div className="flex items-center gap-3 border-t border-white/6 px-4 pt-2.5 pb-1.5">
          {comes.length > 0 ? (
            <p
              className="min-w-0 flex-1 truncate text-[12.5px] text-ash-400"
              title={listFormat().format(comes.map((item) => item.title))}
            >
              {t("modSets.create.libraries")}
              {": "}
              {listFormat().format(comes.map((item) => item.title))}
            </p>
          ) : (
            <span className="flex-1" />
          )}
          {footer}
        </div>
      ) : null}
    </div>
  );
}

/**
 * The chip on a file a set installed: the set's mark and name. `compact` (narrow cards) keeps
 * the mark alone; its name is the tooltip and the accessible name.
 */
export function SetBadgeChip({
  badge,
  compact = false,
  className,
}: {
  badge: SetBadge;
  compact?: boolean;
  className?: string;
}) {
  const { t } = useTranslation();
  const Mark = marks[badge.purpose];
  const label = t("modSets.badge", { name: badge.name });
  if (compact) {
    return (
      <Tooltip content={label}>
        <Chip role="img" aria-label={label} className={cn("shrink-0 px-[5px]", className)}>
          <Mark size={12} weight="bold" aria-hidden className="text-gold-300" />
        </Chip>
      </Tooltip>
    );
  }
  return (
    <Chip className={cn("shrink-0", className)}>
      <Mark size={12} weight="bold" aria-hidden className="text-gold-300" />
      {label}
    </Chip>
  );
}
