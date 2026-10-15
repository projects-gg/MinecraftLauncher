import { Chip, cn, Segmented, Skeleton } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { motion } from "motion/react";
import { useMemo, useState } from "react";
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
      <div className="aspect-[21/8] w-full overflow-hidden bg-obsidian-800">
        {item.coverUrl ? (
          <img src={item.coverUrl} alt="" className="size-full object-cover transition-transform duration-700 ease-out group-hover:scale-[1.03]" />
        ) : null}
      </div>
      <div className="absolute inset-0 bg-[linear-gradient(to_top,rgb(10_9_12/0.92),rgb(10_9_12/0.25)_55%,transparent)]" />
      <div className="absolute inset-x-0 bottom-0 p-7">
        <div className="flex items-center gap-2.5">
          <Chip tone="gold">{t(`categories.${item.category}`)}</Chip>
          <span className="text-[12.5px] text-bone-100/80">{formatRelative(item.publishedAt)}</span>
        </div>
        <h2 className="mt-2.5 max-w-[24ch] font-display text-[34px] leading-[1.02] font-semibold text-bone-50 [text-wrap:balance]">
          {item.title}
        </h2>
        <p className="mt-2 max-w-[60ch] text-[14.5px] text-bone-100/85">{item.summary}</p>
      </div>
    </Link>
  );
}

function Card({ item, index }: { item: NewsItem; index: number }) {
  const { t } = useTranslation();
  return (
    <motion.li
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.4, delay: Math.min(index, 8) * 0.04, ease: [0.16, 1, 0.3, 1] }}
    >
      <Link
        to="/news"
        search={{ news: item.id }}
        className="group glass block h-full overflow-hidden rounded-[var(--radius-panel)] outline-none transition-transform duration-200 hover:-translate-y-0.5 focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <div className="aspect-video overflow-hidden bg-obsidian-800">
          {item.coverUrl ? (
            <img src={item.coverUrl} alt="" loading="lazy" className="size-full object-cover transition-transform duration-500 ease-out group-hover:scale-[1.05]" />
          ) : null}
        </div>
        <div className="p-4">
          <div className="flex items-center gap-2">
            <Chip>{t(`categories.${item.category}`)}</Chip>
            <span className="text-[12px] text-ash-400">{formatRelative(item.publishedAt)}</span>
          </div>
          <h3 className="mt-2 line-clamp-2 font-display text-[16px] leading-snug font-semibold text-bone-50">{item.title}</h3>
          <p className="mt-1.5 line-clamp-2 text-[13px] text-ash-400">{item.summary}</p>
        </div>
      </Link>
    </motion.li>
  );
}

export function NewsPage() {
  const { t } = useTranslation();
  const [filter, setFilter] = useState<Filter>("all");
  const { data, isPending } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });

  const items = useMemo(
    () => (data?.news ?? []).filter((n) => filter === "all" || n.category === filter),
    [data, filter],
  );
  const [featured, ...rest] = items;

  return (
    <div className="flex min-h-0 flex-1 flex-col px-10 pt-4">
      <header className="flex flex-wrap items-end justify-between gap-4 pb-5">
        <h1 className="font-display text-[32px] leading-tight font-semibold text-bone-50">{t("nav.news")}</h1>
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
      <div className={cn("-mx-2 min-h-0 flex-1 overflow-y-auto px-2 pb-8")}>
        {isPending ? (
          <Skeleton className="aspect-[21/8] w-full rounded-[14px]" />
        ) : featured ? (
          <>
            <Featured item={featured} />
            <ul className="mt-5 grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-4">
              {rest.map((item, i) => (
                <Card key={item.id} item={item} index={i} />
              ))}
            </ul>
          </>
        ) : null}
      </div>
    </div>
  );
}
