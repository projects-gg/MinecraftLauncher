import { Button, Chip, cn, EmptyState, Segmented, Skeleton, useMotionPrefs } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { type Ref, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type NewsCategory, type NewsItem } from "@/lib/api";
import { formatRelative } from "@/lib/format";

type Filter = "all" | NewsCategory;

function Featured({ item }: { item: NewsItem }) {
  const { t } = useTranslation();
  return (
    <Link
      to="/news"
      search={{ news: item.id }}
      className="group relative block overflow-hidden rounded-[14px] outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
    >
      <div className="h-[clamp(200px,38vh,320px)] w-full overflow-hidden bg-obsidian-800">
        {item.coverUrl ? (
          <img
            src={item.coverUrl}
            alt=""
            className="size-full object-cover transition-transform duration-700 ease-out group-hover:scale-[1.03]"
          />
        ) : null}
      </div>
      <div className="absolute inset-0 bg-[linear-gradient(to_top,rgb(10_9_12/0.92),rgb(10_9_12/0.25)_55%,transparent)]" />
      <div className="absolute inset-x-0 bottom-0 p-5">
        <div className="flex items-center gap-2.5">
          <Chip tone="onArt">{t(`categories.${item.category}`)}</Chip>
          <span className="text-shadow-art text-[12.5px] text-bone-100/90">
            {formatRelative(item.publishedAt)}
          </span>
        </div>
        <h2 className="text-shadow-art mt-2 max-w-[26ch] font-display text-[26px] leading-[1.05] font-semibold text-bone-50">
          {item.title}
        </h2>
        <p className="mt-1.5 max-w-[60ch] text-[13.5px] text-bone-100/90">{item.summary}</p>
      </div>
    </Link>
  );
}

/**
 * A news card. The page arrives with the page cross-fade; cards that a filter change brings in
 * fade in place and the others glide to their new spots (no rise on every visit).
 */
function Card({ item, ref }: { item: NewsItem; ref?: Ref<HTMLLIElement> }) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  return (
    <motion.li
      ref={ref}
      layout={layout ? "position" : false}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { duration: 0.2 } }}
      exit={{ opacity: 0, transition: { duration: 0.12 } }}
      transition={{ layout: { duration: 0.3, ease: [0.16, 1, 0.3, 1] } }}
    >
      <Link
        to="/news"
        search={{ news: item.id }}
        className="group glass block h-full overflow-hidden rounded-[var(--radius-panel)] outline-none transition-transform duration-200 hover:-translate-y-0.5 focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <div className="aspect-video overflow-hidden bg-obsidian-800">
          {item.coverUrl ? (
            <img
              src={item.coverUrl}
              alt=""
              loading="lazy"
              className="size-full object-cover transition-transform duration-500 ease-out group-hover:scale-[1.05]"
            />
          ) : null}
        </div>
        <div className="p-3.5">
          <div className="flex items-center gap-2">
            <Chip>{t(`categories.${item.category}`)}</Chip>
            <span className="truncate text-[12px] text-ash-300">
              {formatRelative(item.publishedAt)}
            </span>
          </div>
          <h3 className="mt-1.5 line-clamp-2 font-display text-[15px] leading-snug font-semibold text-bone-50">
            {item.title}
          </h3>
          <p className="mt-1 line-clamp-2 text-[12.5px] text-ash-300">{item.summary}</p>
        </div>
      </Link>
    </motion.li>
  );
}

export function NewsPage() {
  const { t } = useTranslation();
  const [filter, setFilter] = useState<Filter>("all");
  const { data, isPending } = useQuery({
    queryKey: ["home-content"],
    queryFn: () => api.homeContent(),
  });

  const items = useMemo(
    () => (data?.news ?? []).filter((n) => filter === "all" || n.category === filter),
    [data, filter],
  );
  const [featured, ...rest] = items;

  return (
    <div className="flex min-h-0 flex-1 flex-col px-page pt-3">
      <header className="flex flex-wrap items-end justify-between gap-3 pb-4">
        <h1 className="page-title">{t("nav.news")}</h1>
        <Segmented<Filter>
          value={filter}
          onValueChange={setFilter}
          label={t("nav.news")}
          options={[
            { value: "all", label: t("home.allNews") },
            { value: "event", label: t("categories.event") },
            { value: "update", label: t("categories.update") },
            { value: "patchNotes", label: t("categories.patchNotes") },
            { value: "announcement", label: t("categories.announcement") },
            { value: "community", label: t("categories.community") },
          ]}
        />
      </header>
      <div className={cn("-mx-2 min-h-0 flex-1 overflow-y-auto px-2 pb-6")}>
        {isPending ? (
          <>
            <Skeleton className="h-[clamp(200px,38vh,320px)] w-full rounded-[14px]" />
            <div className="mt-4 grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-3">
              {[0, 1, 2, 3].map((i) => (
                <Skeleton key={i} className="h-[236px] rounded-[var(--radius-panel)]" />
              ))}
            </div>
          </>
        ) : featured ? (
          <>
            <Featured item={featured} />
            <ul className="relative mt-4 grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-3">
              <AnimatePresence initial={false} mode="popLayout">
                {rest.map((item) => (
                  <Card key={item.id} item={item} />
                ))}
              </AnimatePresence>
            </ul>
          </>
        ) : (
          <EmptyState
            title={filter === "all" ? t("home.newsEmpty") : t("home.newsEmptyCategory")}
            action={
              filter === "all" ? undefined : (
                <Button variant="secondary" onClick={() => setFilter("all")}>
                  {t("home.showAllNews")}
                </Button>
              )
            }
            className="pt-16"
          />
        )}
      </div>
    </div>
  );
}
