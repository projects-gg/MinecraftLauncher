import { ArrowSquareOut, CaretRight } from "@phosphor-icons/react";
import {
  cn,
  countdownTickMs,
  easeOutExpo,
  exitOf,
  Skeleton,
  useInteractionMotion,
  usePerf,
} from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { Popover } from "radix-ui";
import { type ReactNode, useEffect, useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { CoverImage } from "@/features/news/cover-image";
import { MinecraftLabel, useMinecraftText } from "@/features/news/minecraft";
import { NewDot, NewLabel } from "@/features/news/new-dot";
import type { EventItem, HomeContent, MinecraftNewsItem, NewsItem, PatchNotes } from "@/lib/api";
import { cardEvents, nextBoundary, phaseOf, timeLeftText } from "@/lib/event-card";
import { dateTimeFormat, formatRelative } from "@/lib/format";
import i18n from "@/lib/i18n";
import { isWebLink, openLink } from "@/lib/links";
import { columnFill, liveMinecraft, NEWS_COLUMN_ROWS } from "@/lib/minecraft";
import { isNewsNew, useNewsSeen } from "@/stores/news-seen";
import { CommunityRow } from "./community-links";
import { EventJoin } from "./event-join";
import { ReminderBell } from "./event-reminders";

function NewsRow({
  item,
  index,
  arrive,
  isNew,
}: {
  item: NewsItem;
  index: number;
  arrive: boolean;
  /** Not seen yet: a small gold dot before the date. */
  isNew: boolean;
}) {
  const { t } = useTranslation();
  return (
    <motion.li
      // A short window keeps three stories, so the patch notes below stay whole.
      className={cn(index >= 3 && "[@media(max-height:700px)]:hidden")}
      initial={arrive ? { opacity: 0, transform: "translateX(10px)" } : false}
      animate={{ opacity: 1, transform: "translateX(0px)" }}
      transition={{ delay: 0.08 + index * 0.04, duration: 0.4, ease: easeOutExpo }}
    >
      <Link
        to="/"
        search={{ news: item.id }}
        className="group flex gap-2.5 rounded-[var(--radius-panel)] p-1.5 transition-colors duration-150 hover:bg-white/6 focus-visible:outline-2"
      >
        <span className="chamfer relative h-[50px] w-[76px] shrink-0 overflow-hidden bg-obsidian-800 [--cut:5px]">
          {item.coverUrl ? (
            <img
              src={item.coverUrl}
              alt=""
              loading="lazy"
              draggable={false}
              className="size-full object-cover transition-transform duration-500 ease-out group-hover:scale-[1.06] motion-reduce:group-hover:scale-100"
            />
          ) : null}
        </span>
        <span className="min-w-0 flex-1">
          {/* What it is and when, in quiet words (a coloured pill per category was a badge over
              every title and a second accent beside the gold). */}
          <span className="flex min-w-0 items-center gap-1.5 text-[11.5px] text-ash-300">
            <span className="shrink-0 font-medium">{t(`categories.${item.category}`)}</span>
            <span aria-hidden className="text-ash-500">
              ·
            </span>
            <NewDot on={isNew} />
            <span className="truncate">{formatRelative(item.publishedAt)}</span>
          </span>
          {/* Balanced, so a second line never holds one word ("İnşa yarışması: Gökyüzü / şehri"). */}
          <span className="mt-1 line-clamp-2 text-[13px] leading-snug font-semibold text-balance text-bone-100 transition-colors duration-150 group-hover:text-bone-50">
            {item.title}
          </span>
          <NewLabel on={isNew} />
        </span>
      </Link>
    </motion.li>
  );
}

/**
 * A Minecraft update in a row Projects news left empty: the same rhythm as a news row, but quieter
 * (the source mark instead of a category, a lighter title) and never "new".
 */
function MinecraftRow({
  item,
  index,
  arrive,
  first,
}: {
  item: MinecraftNewsItem;
  index: number;
  arrive: boolean;
  /** The first Minecraft row under Projects rows: a little air between the two sources. */
  first: boolean;
}) {
  const text = useMinecraftText(item);
  return (
    <motion.li
      className={cn(first && "pt-1.5", index >= 3 && "[@media(max-height:700px)]:hidden")}
      initial={arrive ? { opacity: 0, transform: "translateX(10px)" } : false}
      animate={{ opacity: 1, transform: "translateX(0px)" }}
      transition={{ delay: 0.08 + index * 0.04, duration: 0.4, ease: easeOutExpo }}
    >
      <Link
        to="/"
        search={{ news: item.id }}
        className="group flex gap-2.5 rounded-[var(--radius-panel)] p-1.5 transition-colors duration-150 hover:bg-white/6 focus-visible:outline-2"
      >
        <CoverImage
          src={item.coverUrl}
          blurhash={item.coverBlurhash}
          className="chamfer relative h-[50px] w-[76px] shrink-0 overflow-hidden bg-obsidian-800 [--cut:5px]"
          imgClassName="group-hover:scale-[1.06]"
        />
        <span className="min-w-0 flex-1">
          <span className="flex min-w-0 items-center gap-1.5 text-[11.5px] text-ash-300">
            <MinecraftLabel />
            <span aria-hidden className="text-ash-500">
              ·
            </span>
            <span className="truncate">{formatRelative(item.publishedAt)}</span>
          </span>
          <span className="mt-1 line-clamp-2 text-[13px] leading-snug font-medium text-balance text-bone-100/90 transition-colors duration-150 group-hover:text-bone-50">
            {text.title}
          </span>
        </span>
      </Link>
    </motion.li>
  );
}

/**
 * Bullet lines of the patch notes box (a repeated line is shown once). The square is a quiet mark:
 * gold stays with OYNA and the actions.
 */
function Highlights({ lines }: { lines: string[] }) {
  return (
    <ul className="mt-2 space-y-1.5">
      {[...new Set(lines)].map((line) => (
        <li
          key={line}
          className="flex gap-2 text-[12.5px] leading-snug text-ash-300 [overflow-wrap:anywhere]"
        >
          <span className="mt-[6px] size-1 shrink-0 bg-ash-500" />
          <span className="line-clamp-2">{line}</span>
        </li>
      ))}
    </ul>
  );
}

/**
 * The newest patch notes: a news item (Studio's or the Discord feed's) opens its article; the
 * official pack's changelog is the fallback when no patch notes were published.
 */
function PatchNotesBox({ notes }: { notes: PatchNotes }) {
  const { t } = useTranslation();
  if (notes.kind === "pack") {
    return (
      <section className="mt-1.5 border-t border-white/7 px-2 pt-3 pb-2.5">
        <div className="flex items-baseline justify-between gap-3">
          <h3 className="font-display text-[14px] font-semibold text-bone-50">
            {t("home.patchNotes")}
          </h3>
          <span className="text-[12px] text-ash-300">
            {t("home.patchVersion", { pack: notes.packName, version: notes.version })}
          </span>
        </div>
        <Highlights lines={notes.highlights} />
      </section>
    );
  }
  return (
    <section className="mt-1.5 border-t border-white/7 pt-1.5 pb-1">
      <Link
        to="/"
        search={{ news: notes.newsId }}
        className="group block rounded-[var(--radius-panel)] px-2 pt-2 pb-2 transition-colors duration-150 hover:bg-white/6 focus-visible:outline-2"
      >
        <span className="flex items-baseline justify-between gap-3">
          <span className="font-display text-[14px] font-semibold text-bone-50">
            {t("home.patchNotes")}
          </span>
          {/* What the box does, named like the column's other links ("Tümü", "1 etkinlik daha"):
              a bare "5 saat önce ›" read as a link to a time. */}
          <span className="flex shrink-0 items-center gap-0.5 text-[12.5px] font-medium text-ash-300 transition-colors duration-150 group-hover:text-gold-300">
            {t("newsColumn.readNotes")}
            <CaretRight
              size={13}
              weight="bold"
              aria-hidden
              className="transition-transform duration-200 ease-out group-hover:translate-x-0.5"
            />
          </span>
        </span>
        {/* The notes' title, then when they came out as plain meta (a long title gives way). */}
        <span className="mt-1 flex min-w-0 items-baseline gap-1.5">
          <span className="min-w-0 truncate text-[13px] font-semibold text-bone-100 transition-colors duration-150 group-hover:text-bone-50">
            {notes.title}
          </span>
          <span aria-hidden className="shrink-0 text-[12px] text-ash-500">
            ·
          </span>
          <span className="shrink-0 text-[12px] text-ash-300">
            {formatRelative(notes.publishedAt)}
          </span>
        </span>
        <Highlights lines={notes.highlights} />
      </Link>
    </section>
  );
}

/**
 * The event the card shows, whether it is live, and the other events behind it. The card
 * applies the backend's rule itself (`lib/event-card.ts`: live first, highest priority, then the
 * one that started last; else the soonest upcoming) and wakes up whenever any event starts or
 * ends, so a short event overlays a long one on time and the long one comes back after it; it
 * asks for fresh content at those moments too.
 */
function useEventCard(events: EventItem[] | undefined) {
  const queryClient = useQueryClient();
  const [now, setNow] = useState(() => Date.now());
  const ordered = useMemo(() => cardEvents(events ?? [], now), [events, now]);
  const event = ordered[0];
  const live = event ? phaseOf(event, now) === "live" : false;
  const boundary = nextBoundary(events ?? [], now);

  useEffect(() => {
    if (boundary === undefined) return;
    // setTimeout fires at once beyond ~24.8 days: wake up early then and wait again.
    const wait = Math.min(Math.max(0, boundary - Date.now()) + 250, 2_000_000_000);
    const id = window.setTimeout(() => {
      const at = Date.now();
      setNow(at);
      if (at >= boundary) void queryClient.invalidateQueries({ queryKey: ["home-content"] });
    }, wait);
    return () => window.clearTimeout(id);
  }, [boundary, queryClient]);

  return { event, live, others: ordered.slice(1) };
}

// In the interface's language, looked up at each call (the language can change live).
const clock = () => dateTimeFormat({ hour: "2-digit", minute: "2-digit" });
const weekdayMoment = () => dateTimeFormat({ weekday: "long", hour: "2-digit", minute: "2-digit" });
const dateMoment = () =>
  dateTimeFormat({ day: "numeric", month: "long", hour: "2-digit", minute: "2-digit" });

/**
 * When an upcoming event starts: "Bugün 20:00", "Yarın 20:00", "Cumartesi 20:00" within the
 * week, "25 Ekim 20:00" beyond it (a weekday alone would point at the wrong week).
 */
function startMoment(iso: string, now: number): string {
  const at = new Date(iso);
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(at) - day(new Date(now))) / 86_400_000);
  if (days === 0) return i18n.t("library.when.today", { time: clock().format(at) });
  if (days === 1) return i18n.t("library.when.tomorrow", { time: clock().format(at) });
  return days < 7 ? weekdayMoment().format(at) : dateMoment().format(at);
}

/** The start under an upcoming event's title; "Yarın" turns into "Bugün" at midnight. */
function StartsAt({ iso }: { iso: string }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const midnight = new Date(now);
    midnight.setHours(24, 0, 0, 0);
    const wait = Math.min(midnight.getTime() - now + 500, 2_000_000_000);
    const id = window.setTimeout(() => setNow(Date.now()), wait);
    return () => window.clearTimeout(id);
  }, [now]);
  return <p className="mt-1 text-[12.5px] text-ash-300 tabular-nums">{startMoment(iso, now)}</p>;
}

/**
 * The list behind "1 etkinlik daha": the other live and upcoming events, upcoming ones with their
 * reminder bell. It opens beside the card (anchored to the whole event block), so the card stays
 * readable; its times are as of the moment it opened. A live one says "Sürüyor" in the text's
 * own colour (the list mixes both kinds; gold stays with the actions).
 */
function OtherEvents({ events, onChosen }: { events: EventItem[]; onChosen: () => void }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [now] = useState(() => Date.now());
  const choose = (event: EventItem) => {
    onChosen();
    if (event.newsId) void navigate({ to: "/", search: { news: event.newsId } });
    else if (event.url) openLink(event.url);
  };
  return (
    <Popover.Portal>
      <Popover.Content
        side="left"
        align="end"
        sideOffset={12}
        collisionPadding={12}
        className="glass-strong pop-motion z-50 w-[264px] rounded-[var(--radius-panel)] p-1.5 shadow-[0_24px_60px_rgb(34_6_6/0.55)]"
      >
        <p className="px-2 pt-1 pb-1.5 text-[12px] font-medium text-ash-300">
          {t("home.otherEvents")}
        </p>
        <ul>
          {events.map((event) => {
            const live = phaseOf(event, now) === "live";
            const clickable = Boolean(event.newsId || event.url);
            const body = (
              <>
                <span className="block truncate text-[13px] font-semibold text-bone-100">
                  {event.title}
                </span>
                <span className="mt-0.5 flex items-center gap-1.5 text-[12px] text-ash-300">
                  {live ? (
                    <span className="font-medium text-bone-100">{t("home.eventRunning")}</span>
                  ) : null}
                  <span className="tabular-nums">
                    {live
                      ? event.openEnded
                        ? t("home.eventStarted", { when: formatRelative(event.startsAt, now) })
                        : t("home.eventEndsIn", {
                            when: timeLeftText(Date.parse(event.endsAt) - now),
                          })
                      : t("home.eventStartsAt", { when: startMoment(event.startsAt, now) })}
                  </span>
                </span>
              </>
            );
            return (
              <li
                key={event.id}
                // The row stays lit while the pointer is on its bell.
                className="relative [&:has([data-bell]:hover)>button]:bg-white/8"
              >
                {clickable ? (
                  <button
                    type="button"
                    onClick={() => choose(event)}
                    className={cn(
                      "block w-full cursor-pointer rounded-md px-2 py-1.5 text-left transition-colors duration-150 hover:bg-white/8 focus-visible:outline-2",
                      !live && "pr-10",
                    )}
                  >
                    {body}
                  </button>
                ) : (
                  <div className={cn("px-2 py-1.5", !live && "pr-10")}>{body}</div>
                )}
                {live ? null : (
                  <ReminderBell event={event} className="absolute inset-y-0 right-1 my-auto" />
                )}
              </li>
            );
          })}
        </ul>
      </Popover.Content>
    </Popover.Portal>
  );
}

/**
 * "20 dakika önce başladı", kept fresh while it is on screen: like the countdown, it stops while
 * nobody can see it (tray, minimized) and is right again the moment it is back.
 */
function StartedAgo({ at }: { at: string }) {
  const { t } = useTranslation();
  const { unseen } = usePerf();
  const [, tick] = useState(0);
  useEffect(() => {
    if (unseen) return;
    tick((n) => n + 1);
    const id = window.setInterval(() => tick((n) => n + 1), 30_000);
    return () => window.clearInterval(id);
  }, [unseen]);
  return <span>{t("home.eventStarted", { when: formatRelative(at) })}</span>;
}

/**
 * "2 saat 14 dakika kaldı" (in the other events list's words): changes once a minute and each
 * second only in the last minute, and stops while nobody can see it; right again the moment it is
 * back.
 */
function TimeLeft({ to }: { to: string }) {
  const { t } = useTranslation();
  const { unseen } = usePerf();
  const [ticks, tick] = useState(0);
  const left = Date.parse(to) - Date.now();
  const wait = unseen ? null : countdownTickMs(left);
  useEffect(() => {
    // Armed again after every tick, also when the next wait equals the last one.
    void ticks;
    if (wait === null) return;
    const id = window.setTimeout(() => tick((n) => n + 1), wait);
    return () => window.clearTimeout(id);
  }, [wait, ticks]);
  return (
    <span className="tabular-nums">{t("home.eventEndsIn", { when: timeLeftText(left) })}</span>
  );
}

/** Opens the event's news article, or its link in the browser; plain when it has neither. */
function EventTarget({ event, children }: { event: EventItem; children: ReactNode }) {
  const className =
    "group -mx-2 block rounded-[var(--radius-panel)] px-2 py-1.5 text-left transition-colors duration-150 hover:bg-white/6 focus-visible:outline-2";
  if (event.newsId) {
    return (
      <Link to="/" search={{ news: event.newsId }} className={className} data-event-target="">
        {children}
      </Link>
    );
  }
  if (event.url) {
    const url = event.url;
    return (
      <button
        type="button"
        onClick={() => openLink(url)}
        className={cn(className, "w-[calc(100%+1rem)]")}
        data-event-target=""
      >
        {children}
      </button>
    );
  }
  return <div className="py-1.5">{children}</div>;
}

function EventBlock({
  event,
  live,
  others,
}: {
  event: EventItem;
  live: boolean;
  /** The other live and upcoming events, behind the card. */
  others: EventItem[];
}) {
  const { t } = useTranslation();
  const glide = useInteractionMotion();
  // Reduced motion keeps the cross-fade only.
  const shift = glide ? 6 : 0;
  const enter = 0.28;
  const [listOpen, setListOpen] = useState(false);
  const titleId = useId();
  return (
    <Popover.Root open={listOpen} onOpenChange={setListOpen}>
      <Popover.Anchor asChild>
        <section className="relative border-t border-white/7 px-3.5 pt-2.5">
          {/* The block's own heading, as the patch notes have theirs (it was a small coloured
              kicker over the event's title), and the other events behind a plain link beside it.
              "Süren etkinlik" says it runs, so the time line below needs no word for it (a bare
              "Etkinlik" also repeated the stories' category). */}
          <div className="flex min-h-[22px] items-center justify-between gap-3">
            <h3 className="font-display text-[14px] font-semibold text-bone-50">
              {live ? t("newsColumn.liveEvent") : t("home.nextEvent")}
            </h3>
            {others.length ? (
              <Popover.Trigger asChild>
                <button
                  type="button"
                  className="-mr-1.5 flex cursor-pointer items-center gap-0.5 rounded-md px-1.5 py-0.5 text-[12.5px] font-medium text-ash-300 tabular-nums transition-colors duration-150 hover:text-gold-300 focus-visible:outline-2 data-[state=open]:text-gold-300"
                >
                  {t("home.moreEvents", { count: others.length })}
                  <CaretRight size={13} weight="bold" aria-hidden />
                </button>
              </Popover.Trigger>
            ) : null}
          </div>
          {/* The leaving event is lifted out of the flow, so a switch cross-fades at once and a
          second switch mid-way retargets instead of waiting. The bell goes with its event. */}
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.div
              key={`${event.id}:${live ? "live" : "next"}`}
              // The card stays lit while the pointer is on its bell (a sibling, not inside it).
              className="relative [&:has([data-bell]:hover)_[data-event-target]]:bg-white/6"
              initial={{ opacity: 0, transform: `translateY(${shift}px)` }}
              animate={{
                opacity: 1,
                transform: "translateY(0px)",
                transition: { duration: enter, ease: easeOutExpo },
              }}
              exit={{
                opacity: 0,
                transform: `translateY(${-shift / 2}px)`,
                transition: { duration: exitOf(enter), ease: easeOutExpo },
              }}
            >
              <EventTarget event={event}>
                <div className="flex items-start justify-between gap-2">
                  <p
                    id={titleId}
                    className="line-clamp-2 text-[13px] leading-snug font-semibold text-balance text-bone-100 transition-colors duration-150 group-hover:text-bone-50"
                  >
                    {event.title}
                  </p>
                  {/* Like the stories above, an article opens without a caret (one already leads to
                      the other events beside the heading); the browser's mark stays. */}
                  {event.url && !event.newsId && isWebLink(event.url) ? (
                    <ArrowSquareOut
                      size={14}
                      aria-hidden
                      className="mt-[2px] shrink-0 text-ash-400 transition-colors duration-200 group-hover:text-gold-300"
                    />
                  ) : null}
                </div>
                {/* One line of time, in the other events list's words: what is left (or since
                    when), or when it starts. Big ticking digits outshouted the panel. */}
                {live ? (
                  <p className="mt-1 min-w-0 text-[12.5px] text-ash-300">
                    {event.openEnded ? (
                      <StartedAgo at={event.startsAt} />
                    ) : (
                      <TimeLeft to={event.endsAt} />
                    )}
                  </p>
                ) : (
                  <StartsAt iso={event.startsAt} />
                )}
              </EventTarget>
              {live ? (
                // On the server: "Katıl" takes the bell's place, level with the time line.
                event.join ? (
                  <EventJoin describedBy={titleId} className="absolute right-0 bottom-[1px]" />
                ) : null
              ) : (
                // Level with the start time.
                <ReminderBell event={event} className="absolute right-[-6px] bottom-[1px]" />
              )}
            </motion.div>
          </AnimatePresence>
        </section>
      </Popover.Anchor>
      {others.length ? <OtherEvents events={others} onChosen={() => setListOpen(false)} /> : null}
    </Popover.Root>
  );
}

/** Whether the column already settled in during this start (see `NewsColumn`). */
let columnArrived = false;

/** The LoL-patcher signature: a right-hand column of news, patch notes and the next event. */
export function NewsColumn({ content, loading }: { content?: HomeContent; loading: boolean }) {
  const { t } = useTranslation();
  const notes = content?.patchNotes;
  const { event, live, others } = useEventCard(content?.events);
  // The patch notes box already shows its article, the hero its own and the event card the one
  // about its event (the Discord feed makes a story and an event from one message): each story
  // shows once.
  const boxed = notes?.kind === "news" ? notes.newsId : undefined;
  const carded = event?.newsId;
  const news = (content?.news ?? [])
    .filter((n) => n.id !== content?.hero.newsId && n.id !== boxed && n.id !== carded)
    .slice(0, NEWS_COLUMN_ROWS);
  const watermark = useNewsSeen((s) => s.watermark);
  const read = useNewsSeen((s) => s.read);
  // The column settles in once per start (the boot moment), not on every return to Home.
  const [arrive] = useState(() => !columnArrived);
  useEffect(() => {
    columnArrived = true;
  }, []);
  const now = Date.now();
  // Minecraft's updates only take the rows Projects news leaves empty.
  const minecraft = columnFill(news.length, liveMinecraft(content?.minecraft, now));

  return (
    <motion.aside
      initial={arrive ? { opacity: 0, transform: "translateX(24px)" } : false}
      animate={{ opacity: 1, transform: "translateX(0px)" }}
      transition={{ duration: 0.55, ease: easeOutExpo, delay: 0.15 }}
      // As tall as its content, up to the page height (then the news list scrolls).
      className="glass flex max-h-full w-news shrink-0 flex-col self-start overflow-hidden rounded-[var(--radius-panel)]"
      aria-label={t("home.newsTitle")}
    >
      <header className="flex items-center justify-between px-3.5 pt-3 pb-1.5">
        <h2 className="font-display text-[16px] font-semibold text-bone-50">
          {t("home.newsTitle")}
        </h2>
        <Link
          to="/news"
          className="-mr-1 flex items-center gap-0.5 rounded-md px-1.5 py-1 text-[12.5px] font-medium text-ash-300 transition-colors hover:text-gold-300"
        >
          {t("home.allNews")} <CaretRight size={13} weight="bold" />
        </Link>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-2">
        {loading ? (
          <div className="space-y-2 p-1.5">
            {[0, 1, 2].map((i) => (
              <div key={i} className="flex gap-2.5">
                <Skeleton className="h-[50px] w-[76px]" />
                <div className="flex-1 space-y-2 pt-1">
                  <Skeleton className="h-3.5 w-24" />
                  <Skeleton className="h-3.5 w-full" />
                </div>
              </div>
            ))}
          </div>
        ) : !content?.news.length && !notes && !minecraft.length ? (
          // Nothing published yet (or nothing cached offline): one calm line, not an empty panel.
          <p className="px-1.5 pt-1 pb-2 text-[13px] text-ash-300">{t("home.noNews")}</p>
        ) : (
          <ul className="space-y-px">
            {news.map((item, i) => (
              <NewsRow
                key={item.id}
                item={item}
                index={i}
                arrive={arrive}
                isNew={isNewsNew(item, { watermark, read }, now)}
              />
            ))}
            {minecraft.map((item, i) => (
              <MinecraftRow
                key={item.id}
                item={item}
                index={news.length + i}
                arrive={arrive}
                first={i === 0 && news.length > 0}
              />
            ))}
          </ul>
        )}

        {notes ? <PatchNotesBox notes={notes} /> : null}
      </div>

      {event ? <EventBlock event={event} live={live} others={others} /> : null}
      {/* Wiki, Discord, forum and the social accounts: outside the scrolling part, so they stay
          in view at every window size. */}
      {loading || content?.links?.community.length ? (
        <CommunityRow links={content?.links?.community} loading={loading} />
      ) : (
        <div className="h-2.5" />
      )}
    </motion.aside>
  );
}
