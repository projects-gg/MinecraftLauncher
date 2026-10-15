import { CaretRight } from "@phosphor-icons/react";
import { Chip, Countdown, cn, Skeleton } from "@pl/ui";
import { Link } from "@tanstack/react-router";
import { motion } from "motion/react";
import { useTranslation } from "react-i18next";
import type { EventItem, HomeContent, NewsItem } from "@/lib/api";
import { formatRelative } from "@/lib/format";

const categoryTone = {
  event: "gold",
  update: "neutral",
  announcement: "neutral",
  patchNotes: "jade",
  community: "neutral",
} as const;

function NewsRow({ item, index }: { item: NewsItem; index: number }) {
  const { t } = useTranslation();
  return (
    <motion.li
      initial={{ opacity: 0, x: 12 }}
      animate={{ opacity: 1, x: 0 }}
      transition={{ delay: 0.08 + index * 0.04, duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
    >
      <Link
        to="/"
        search={{ news: item.id }}
        className="group flex gap-3 rounded-[var(--radius-panel)] p-2 transition-colors duration-150 hover:bg-white/6 focus-visible:outline-2"
      >
        <span className="chamfer relative h-[58px] w-[88px] shrink-0 overflow-hidden bg-obsidian-800 [--cut:6px]">
          {item.coverUrl ? (
            <img
              src={item.coverUrl}
              alt=""
              loading="lazy"
              draggable={false}
              className="size-full object-cover transition-transform duration-500 ease-out group-hover:scale-[1.06]"
            />
          ) : null}
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2">
            <Chip tone={categoryTone[item.category]}>{t(`categories.${item.category}`)}</Chip>
            <span className="text-[11.5px] text-ash-400">{formatRelative(item.publishedAt)}</span>
          </span>
          <span className="mt-1 line-clamp-2 text-[13.5px] leading-snug font-semibold text-bone-100 group-hover:text-bone-50">
            {item.title}
          </span>
        </span>
      </Link>
    </motion.li>
  );
}

function EventBlock({ event }: { event: EventItem }) {
  const { t } = useTranslation();
  return (
    <section className="border-t border-white/7 px-4 pt-4 pb-1">
      <h3 className="text-[12px] font-medium text-ash-400">{t("home.nextEvent")}</h3>
      <p className="mt-1 font-display text-[15px] font-semibold text-bone-50">{event.title}</p>
      <Countdown
        to={new Date(event.startsAt)}
        units={{
          days: t("countdown.days"),
          hours: t("countdown.hours"),
          minutes: t("countdown.minutes"),
          seconds: t("countdown.seconds"),
        }}
        className="mt-3"
      />
    </section>
  );
}

/** The LoL-patcher signature: a right-hand column of news, patch notes and the next event. */
export function NewsColumn({ content, loading }: { content?: HomeContent; loading: boolean }) {
  const { t } = useTranslation();
  const news = (content?.news ?? []).filter((n) => n.id !== content?.hero.newsId).slice(0, 4);
  const event = content?.events[0];
  const notes = content?.patchNotes;

  return (
    <motion.aside
      initial={{ opacity: 0, x: 28 }}
      animate={{ opacity: 1, x: 0 }}
      transition={{ duration: 0.55, ease: [0.16, 1, 0.3, 1], delay: 0.15 }}
      className="glass flex w-[340px] shrink-0 flex-col overflow-hidden rounded-[var(--radius-panel)]"
      aria-label={t("home.newsTitle")}
    >
      <header className="flex items-center justify-between px-4 pt-4 pb-2">
        <h2 className="font-display text-[18px] font-semibold text-bone-50">{t("home.newsTitle")}</h2>
        <Link
          to="/news"
          className="flex items-center gap-0.5 rounded-md px-1.5 py-1 text-[12.5px] font-medium text-ash-300 transition-colors hover:text-gold-300"
        >
          {t("home.allNews")} <CaretRight size={13} weight="bold" />
        </Link>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-2">
        {loading ? (
          <div className="space-y-2 p-2">
            {[0, 1, 2].map((i) => (
              <div key={i} className="flex gap-3">
                <Skeleton className="h-[58px] w-[88px]" />
                <div className="flex-1 space-y-2 pt-1">
                  <Skeleton className="h-3.5 w-24" />
                  <Skeleton className="h-3.5 w-full" />
                </div>
              </div>
            ))}
          </div>
        ) : (
          <ul className="space-y-0.5">
            {news.map((item, i) => (
              <NewsRow key={item.id} item={item} index={i} />
            ))}
          </ul>
        )}

        {notes ? (
          <section className={cn("mt-2 border-t border-white/7 px-2 pt-4 pb-3")}>
            <div className="flex items-baseline justify-between">
              <h3 className="font-display text-[15px] font-semibold text-bone-50">{t("home.patchNotes")}</h3>
              <span className="text-[12px] text-gold-300">
                {t("home.patchVersion", { pack: notes.packName, version: notes.version })}
              </span>
            </div>
            <ul className="mt-2 space-y-1.5">
              {notes.highlights.map((line) => (
                <li key={line} className="flex gap-2 text-[13px] leading-snug text-ash-300">
                  <span className="mt-[7px] size-1 shrink-0 bg-gold-500" />
                  {line}
                </li>
              ))}
            </ul>
          </section>
        ) : null}
      </div>

      {event ? <EventBlock event={event} /> : null}
      <div className="h-3" />
    </motion.aside>
  );
}
