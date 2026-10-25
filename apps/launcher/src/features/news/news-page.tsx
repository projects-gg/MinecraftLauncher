import { Cube } from "@phosphor-icons/react";
import { Button, cn, EmptyState, Segmented, Skeleton, useMotionPrefs } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, type Ref, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type MinecraftNewsItem, type NewsCategory, type NewsItem } from "@/lib/api";
import { formatRelative } from "@/lib/format";
import { liveMinecraft } from "@/lib/minecraft";
import { isNewsNew, newestPublished, useNewsSeen } from "@/stores/news-seen";
import { CoverImage } from "./cover-image";
import { MinecraftLabel, useMinecraftText } from "./minecraft";
import { NewDot, NewLabel } from "./new-dot";

/** Every Projects category, or Minecraft's updates (a source, listed last). */
type Filter = "all" | NewsCategory | "minecraft";

/** What a card shows, for Projects news and Minecraft updates alike. */
interface CardFace {
  id: string;
  coverUrl?: string;
  coverBlurhash?: string;
  /** The category chip, or Minecraft's source mark. */
  label: ReactNode;
  publishedAt: string;
  title: string;
  summary: string;
  isNew: boolean;
  /** Minecraft's updates: a little quieter than Projects news. */
  quiet?: boolean;
}

function Featured({ face }: { face: CardFace }) {
  return (
    <Link
      to="/news"
      search={{ news: face.id }}
      className="group relative block overflow-hidden rounded-[14px] outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
    >
      <CoverImage
        src={face.coverUrl}
        blurhash={face.coverBlurhash}
        className="h-[clamp(200px,38vh,320px)] w-full overflow-hidden bg-obsidian-800"
        imgClassName="group-hover:scale-[1.03]"
        zoomMs={700}
      />
      <div className="absolute inset-0 bg-[linear-gradient(to_top,rgb(10_9_12/0.92),rgb(10_9_12/0.25)_55%,transparent)]" />
      <div className="absolute inset-x-0 bottom-0 p-5">
        <div className="text-shadow-art flex items-center gap-1.5 text-[12.5px] text-bone-100/90">
          {face.label}
          <span aria-hidden>·</span>
          <NewDot on={face.isNew} />
          <span>{formatRelative(face.publishedAt)}</span>
        </div>
        <h2 className="text-shadow-art mt-2 max-w-[26ch] font-display text-[26px] leading-[1.05] font-semibold text-bone-50">
          {face.title}
        </h2>
        <p className="mt-1.5 max-w-[60ch] text-[13.5px] text-bone-100/90">{face.summary}</p>
        <NewLabel on={face.isNew} />
      </div>
    </Link>
  );
}

/**
 * A news card. The page arrives with the page cross-fade; cards that a filter change brings in
 * fade in place and the others glide to their new spots (no rise on every visit).
 */
function Card({ face, ref }: { face: CardFace; ref?: Ref<HTMLLIElement> }) {
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
        search={{ news: face.id }}
        className="group glass block h-full overflow-hidden rounded-[var(--radius-panel)] outline-none transition-transform duration-200 hover:-translate-y-0.5 focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <CoverImage
          src={face.coverUrl}
          blurhash={face.coverBlurhash}
          lazy
          className="aspect-video overflow-hidden bg-obsidian-800"
          imgClassName="group-hover:scale-[1.05]"
        />
        <div className="p-3.5">
          <div className="flex min-w-0 items-center gap-1.5 text-[12px] text-ash-300">
            {face.label}
            <span aria-hidden className="text-ash-500">
              ·
            </span>
            <NewDot on={face.isNew} />
            <span className="truncate">{formatRelative(face.publishedAt)}</span>
          </div>
          <h3
            className={cn(
              "mt-1.5 line-clamp-2 font-display text-[15px] leading-snug font-semibold",
              face.quiet ? "text-bone-100" : "text-bone-50",
            )}
          >
            {face.title}
          </h3>
          <p className="mt-1 line-clamp-2 text-[12.5px] text-ash-300">{face.summary}</p>
          <NewLabel on={face.isNew} />
        </div>
      </Link>
    </motion.li>
  );
}

/** A Projects news item as a card face: its category in words, as the meta line's first part. */
function useNewsFace() {
  const { t } = useTranslation();
  return (item: NewsItem, isNew: boolean): CardFace => ({
    id: item.id,
    coverUrl: item.coverUrl,
    label: <span className="shrink-0 font-medium">{t(`categories.${item.category}`)}</span>,
    publishedAt: item.publishedAt,
    title: item.title,
    summary: item.summary,
    isNew,
  });
}

/** A Minecraft update as a card face, in the interface's language; never new. */
function minecraftFace(
  item: MinecraftNewsItem,
  text: { title: string; summary: string },
): CardFace {
  return {
    id: item.id,
    coverUrl: item.coverUrl,
    coverBlurhash: item.coverBlurhash,
    label: <MinecraftLabel />,
    publishedAt: item.publishedAt,
    title: text.title,
    summary: text.summary,
    isNew: false,
    quiet: true,
  };
}

function MinecraftFeatured({ item }: { item: MinecraftNewsItem }) {
  return <Featured face={minecraftFace(item, useMinecraftText(item))} />;
}

function MinecraftCard({ item, ref }: { item: MinecraftNewsItem; ref?: Ref<HTMLLIElement> }) {
  return <Card ref={ref} face={minecraftFace(item, useMinecraftText(item))} />;
}

/**
 * New news keeps its dot for the whole visit: the page judges by the watermark it arrived with
 * (an opened article loses its dot at once). Everything the page shows counts as seen from the
 * moment it shows it, so the dots are gone on the next visit, and Home, rendered as the player
 * leaves, already draws its list without them (counting them at unmount came a render too late:
 * the dots faded out on arrival and left gaps). Minecraft's updates never count: they are never
 * new.
 */
function useVisitSeen(news: NewsItem[] | undefined) {
  const [watermark] = useState(() => useNewsSeen.getState().watermark);
  const read = useNewsSeen((s) => s.read);
  useEffect(() => {
    const newest = newestPublished(news ?? []);
    if (newest) useNewsSeen.getState().advance(newest);
  }, [news]);
  return { watermark, read };
}

export function NewsPage() {
  const { t } = useTranslation();
  const [filter, setFilter] = useState<Filter>("all");
  const { data, isPending } = useQuery({
    queryKey: ["home-content"],
    queryFn: () => api.homeContent(),
  });
  const seen = useVisitSeen(data?.news);
  const featuredFace = useNewsFace();
  const cardFace = useNewsFace();
  const minecraft = useMemo(() => liveMinecraft(data?.minecraft, Date.now()), [data]);
  // The Minecraft filter exists only while there is something under it.
  const active: Filter = filter === "minecraft" && !minecraft.length ? "all" : filter;

  const items = useMemo(
    () =>
      active === "minecraft"
        ? []
        : (data?.news ?? []).filter((n) => active === "all" || n.category === active),
    [data, active],
  );
  const [featured, ...rest] = items;
  // Projects news leads the page while there is any; Minecraft's updates come after it, and
  // only "Tümü" and the Minecraft filter show them.
  const shownMinecraft = active === "all" || active === "minecraft" ? minecraft : [];
  const [leadMinecraft, ...moreMinecraft] = featured ? [] : shownMinecraft;
  const sectionMinecraft = featured ? shownMinecraft : [];
  const now = Date.now();

  const options: { value: Filter; label: string }[] = [
    { value: "all", label: t("home.allNews") },
    { value: "event", label: t("categories.event") },
    { value: "update", label: t("categories.update") },
    { value: "patchNotes", label: t("categories.patchNotes") },
    { value: "announcement", label: t("categories.announcement") },
    { value: "community", label: t("categories.community") },
    ...(minecraft.length ? [{ value: "minecraft" as const, label: t("minecraft.label") }] : []),
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col px-page pt-3">
      <header className="flex flex-wrap items-end justify-between gap-3 pb-4">
        <h1 className="page-title">{t("nav.news")}</h1>
        <Segmented<Filter>
          value={active}
          onValueChange={setFilter}
          label={t("nav.news")}
          options={options}
        />
      </header>
      {/* Positioned: what a filter change takes away pops out of the flow here, clipped with
          the list, even when the list is scrolled. */}
      <div className={cn("relative -mx-2 min-h-0 flex-1 overflow-y-auto px-2 pb-6")}>
        {isPending ? (
          <>
            <Skeleton className="h-[clamp(200px,38vh,320px)] w-full rounded-[14px]" />
            <div className="mt-4 grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-3">
              {[0, 1, 2, 3].map((i) => (
                <Skeleton key={i} className="h-[236px] rounded-[var(--radius-panel)]" />
              ))}
            </div>
          </>
        ) : (
          // A category with nothing in it cross-fades with the news like one category with
          // another (it used to snap in and out; POLISH6).
          <AnimatePresence initial={false} mode="popLayout">
            {featured || leadMinecraft ? (
              <motion.div key="news" {...LIST_FADE}>
                {/* A filter change cross-fades the big card in place, like the cards below it. */}
                <div className="relative">
                  <AnimatePresence initial={false} mode="popLayout">
                    <motion.div
                      key={featured?.id ?? leadMinecraft?.id}
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1, transition: { duration: 0.2 } }}
                      exit={{ opacity: 0, transition: { duration: 0.12 } }}
                    >
                      {featured ? (
                        <Featured face={featuredFace(featured, isNewsNew(featured, seen, now))} />
                      ) : leadMinecraft ? (
                        <MinecraftFeatured item={leadMinecraft} />
                      ) : null}
                    </motion.div>
                  </AnimatePresence>
                </div>
                <ul className="relative mt-4 grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-3">
                  <AnimatePresence initial={false} mode="popLayout">
                    {rest.map((item) => (
                      <Card key={item.id} face={cardFace(item, isNewsNew(item, seen, now))} />
                    ))}
                    {moreMinecraft.map((item) => (
                      <MinecraftCard key={item.id} item={item} />
                    ))}
                  </AnimatePresence>
                </ul>
                <AnimatePresence initial={false}>
                  {sectionMinecraft.length ? (
                    <motion.section
                      key="minecraft"
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1, transition: { duration: 0.2 } }}
                      exit={{ opacity: 0, transition: { duration: 0.12 } }}
                      aria-labelledby="news-minecraft"
                      className="mt-7"
                    >
                      <h2
                        id="news-minecraft"
                        className="mb-2.5 flex items-center gap-2 px-1 font-display text-[15px] font-semibold text-ash-300"
                      >
                        <Cube size={15} aria-hidden />
                        {t("minecraft.label")}
                      </h2>
                      <ul className="relative grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-3">
                        {sectionMinecraft.map((item) => (
                          <MinecraftCard key={item.id} item={item} />
                        ))}
                      </ul>
                    </motion.section>
                  ) : null}
                </AnimatePresence>
              </motion.div>
            ) : (
              <motion.div key="empty" {...LIST_FADE}>
                <EmptyState
                  title={active === "all" ? t("home.newsEmpty") : t("home.newsEmptyCategory")}
                  action={
                    active === "all" ? undefined : (
                      <Button variant="secondary" onClick={() => setFilter("all")}>
                        {t("home.showAllNews")}
                      </Button>
                    )
                  }
                  className="pt-16"
                />
              </motion.div>
            )}
          </AnimatePresence>
        )}
      </div>
    </div>
  );
}

/** The news and the empty category trade places with the cards' own fade. */
const LIST_FADE = {
  initial: { opacity: 0 },
  animate: { opacity: 1, transition: { duration: 0.2 } },
  exit: { opacity: 0, transition: { duration: 0.12 } },
} as const;
