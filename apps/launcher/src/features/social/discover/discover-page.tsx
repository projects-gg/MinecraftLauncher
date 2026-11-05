// "Ekip bul" (CREW-DISCOVER), a page of the friends panel under the crew page: listed crews in the
// day's order (the same for everyone, a new one each day), a search over names, tags and texts,
// the tags as quiet chips and the rest of the filters in one menu ("Filtreler": open to join now,
// active this week, size). The listing is fetched only when the page opens (listing.ts). Loading
// shows card-shaped skeletons; a search swaps the list in place (no glide while typing); a card
// that leaves (its crew left the listing) fades where it is and the rest glide up.
import { Funnel } from "@phosphor-icons/react";
import { Button, EmptyState, IconButton, Menu, SearchInput, Skeleton } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ToggleChip } from "@/features/library/parts";
import { DISCOVER_TAGS, type DiscoverTag } from "@/lib/api/discover-types";
import { dateTimeFormat } from "@/lib/format";
import { canAddCrew } from "../crews";
import { glide, useSocialMotion } from "../motion";
import { useSocialUi } from "../store";
import { DiscoverCard } from "./discover-card";
import { loadListing, useListing } from "./listing";
import {
  type CrewSize,
  type DiscoverFilter,
  filterCrews,
  filtering,
  menuFilters,
  NO_FILTER,
  waveEnding,
  waveWhen,
} from "./model";

function CardSkeleton({ fade }: { fade: number }) {
  return (
    <div
      aria-hidden
      className="rounded-[var(--radius-panel)] bg-obsidian-950/40 px-4 pt-3.5 pb-3 ring-1 ring-white/8"
      style={{ opacity: fade }}
    >
      <div className="flex items-center gap-3">
        <Skeleton className="size-10" />
        <div className="flex-1 space-y-1.5">
          <Skeleton className="h-3.5 w-32" />
          <Skeleton className="h-3 w-16" />
        </div>
      </div>
      <Skeleton className="mt-3 h-3 w-full" />
      <Skeleton className="mt-1.5 h-3 w-3/4" />
      <div className="mt-3 flex items-center gap-1.5">
        <Skeleton className="h-5 w-16" />
        <Skeleton className="h-5 w-20" />
        <Skeleton className="ml-auto h-8 w-16" />
      </div>
    </div>
  );
}

export function DiscoverPage() {
  const { t } = useTranslation();
  const { layout } = useSocialMotion();
  const kept = useListing((s) => s.kept);
  const state = useListing((s) => s.state);
  const crews = useSocialUi((s) => s.view.crews);
  const limit = useSocialUi((s) => s.view.crewLimit);
  const multi = useSocialUi((s) => s.view.crew.multi);
  const canAdd = canAddCrew(crews, limit, multi);
  const [filter, setFilter] = useState<DiscoverFilter>(NO_FILTER);

  // Asked for when the page opens (kept until the next wave; a quick check after 15 minutes).
  useEffect(() => {
    void loadListing();
  }, []);

  const tagWords = (tag: DiscoverTag) => t(`discover.tags.${tag}`);
  const shown = useMemo(
    () => (kept ? filterCrews(kept.listing.crews, filter, (tag) => t(`discover.tags.${tag}`)) : []),
    [kept, filter, t],
  );
  // Only tags some listed crew carries are offered (no chip that empties the list).
  const offered = useMemo(
    () =>
      DISCOVER_TAGS.filter((tag) => kept?.listing.crews.some((crew) => crew.tags.includes(tag))),
    [kept],
  );
  const set = (change: Partial<DiscoverFilter>) => setFilter((now) => ({ ...now, ...change }));
  const toggleTag = (tag: DiscoverTag, on: boolean) =>
    set({ tags: on ? [...filter.tags, tag] : filter.tags.filter((t) => t !== tag) });
  const size = (value: CrewSize) => ({
    label: t(
      value === "any"
        ? "discover.filters.sizeAny"
        : value === "small"
          ? "discover.filters.small"
          : value === "mid"
            ? "discover.filters.mid"
            : "discover.filters.big",
    ),
    checked: filter.size === value,
    radio: true,
    onSelect: () => set({ size: value }),
  });
  const active = menuFilters(filter);
  const wave = kept ? waveWhen(kept.listing.next, Date.now()) : null;
  const clock = dateTimeFormat({ hour: "2-digit", minute: "2-digit" });

  if (state === "off") {
    return <EmptyState title={t("discover.off")} className="pt-10" />;
  }
  if (state === "error" && !kept) {
    return (
      <EmptyState
        title={t("discover.failed.title")}
        body={t("discover.failed.body")}
        action={
          <Button size="sm" variant="secondary" onClick={() => void loadListing(true)}>
            {t("discover.failed.retry")}
          </Button>
        }
        className="pt-10"
      />
    );
  }

  return (
    <div className="pt-1">
      <div className="flex items-center gap-2">
        <SearchInput
          value={filter.query}
          onChange={(query) => set({ query })}
          placeholder={t("discover.search")}
          label={t("discover.searchLabel")}
          clearLabel={t("discover.clearSearch")}
          className="min-w-0 flex-1"
        />
        <Menu
          title={t("discover.filters.button")}
          items={[
            {
              label: t("discover.filters.open"),
              checked: filter.open,
              onSelect: () => set({ open: !filter.open }),
            },
            {
              label: t("discover.filters.active"),
              checked: filter.active,
              onSelect: () => set({ active: !filter.active }),
            },
            null,
            size("any"),
            size("small"),
            size("mid"),
            size("big"),
            ...(active > 0
              ? [
                  null,
                  {
                    label: t("discover.filters.clear"),
                    onSelect: () => set({ open: false, active: false, size: "any" }),
                  },
                ]
              : []),
          ]}
          trigger={
            <span className="relative inline-flex">
              <IconButton
                label={t("discover.filters.button")}
                icon={<Funnel size={17} weight={active > 0 ? "fill" : "regular"} />}
              />
              {active > 0 ? (
                <span
                  aria-hidden
                  className="pointer-events-none absolute -top-0.5 -right-0.5 flex size-4 items-center justify-center rounded-full bg-bone-100 text-[10.5px] font-semibold text-obsidian-950 tabular-nums"
                >
                  {active}
                </span>
              ) : null}
            </span>
          }
        />
      </div>

      {offered.length > 0 ? (
        <fieldset className="-mx-4 mt-2.5 flex min-w-0 gap-1.5 overflow-x-auto border-0 px-4 pt-0 pb-1 [scrollbar-width:none]">
          <legend className="sr-only">{t("discover.tagsLabel")}</legend>
          {offered.map((tag) => (
            <ToggleChip
              key={tag}
              tone="chosen"
              pressed={filter.tags.includes(tag)}
              onPressedChange={(on) => toggleTag(tag, on)}
            >
              {tagWords(tag)}
            </ToggleChip>
          ))}
        </fieldset>
      ) : null}

      <div className="mt-3">
        {state === "loading" && !kept ? (
          <div className="flex flex-col gap-2.5" aria-busy>
            {[1, 0.75, 0.5].map((fade) => (
              <CardSkeleton key={fade} fade={fade} />
            ))}
          </div>
        ) : kept && kept.listing.crews.length === 0 ? (
          <EmptyState
            title={t("discover.empty.title")}
            body={t("discover.empty.body")}
            className="pt-8"
          />
        ) : shown.length === 0 && kept ? (
          <EmptyState
            title={t("discover.noMatch.title")}
            body={t("discover.noMatch.body")}
            action={
              filtering(filter) ? (
                <Button size="sm" variant="secondary" onClick={() => setFilter(NO_FILTER)}>
                  {t("discover.filters.clear")}
                </Button>
              ) : undefined
            }
            className="pt-8"
          />
        ) : (
          // Keyed by the filter: a search swaps the list in place; a card that leaves fades and the
          // others glide.
          <motion.ul key={JSON.stringify(filter)} className="flex flex-col gap-2.5">
            <AnimatePresence initial={false}>
              {shown.map((crew) => (
                <motion.li
                  key={crew.id}
                  layout={layout ? "position" : false}
                  transition={{ layout: glide }}
                  exit={{ opacity: 0, transition: { duration: 0.16 } }}
                >
                  <DiscoverCard crew={crew} canAdd={canAdd} />
                </motion.li>
              ))}
            </AnimatePresence>
          </motion.ul>
        )}
      </div>

      {kept && wave && shown.length > 0 ? (
        <p className="mt-4 px-1 text-[12px] leading-snug text-ash-400">
          {t("discover.renews", {
            time: clock.format(new Date(kept.listing.next * 1000)),
            ending: waveEnding(wave.hour, wave.minute),
          })}
        </p>
      ) : null}
    </div>
  );
}
