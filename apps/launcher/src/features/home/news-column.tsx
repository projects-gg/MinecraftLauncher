import { ArrowSquareOut, CaretRight } from "@phosphor-icons/react";
import { Chip, Countdown, cn, easeOutExpo, exitOf, Skeleton, useInteractionMotion } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { Popover } from "radix-ui";
import { type ReactNode, useEffect, useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { CoverImage } from "@/features/news/cover-image";
import { MinecraftChip, useMinecraftText } from "@/features/news/minecraft";
import { NewDot, NewLabel } from "@/features/news/new-dot";
import type { EventItem, HomeContent, MinecraftNewsItem, NewsItem, PatchNotes } from "@/lib/api";
import { cardEvents, nextBoundary, phaseOf } from "@/lib/event-card";
import { dateTimeFormat, formatDuration, formatRelative } from "@/lib/format";
import i18n from "@/lib/i18n";
import { isWebLink, openLink } from "@/lib/links";
import { columnFill, liveMinecraft, NEWS_COLUMN_ROWS } from "@/lib/minecraft";
import { isNewsNew, useNewsSeen } from "@/stores/news-seen";
import { EventJoin } from "./event-join";
import { ReminderBell } from "./event-reminders";

const categoryTone = {
  event: "gold",
  update: "neutral",
  announcement: "neutral",
  patchNotes: "jade",
  community: "neutral",
} as const;

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
              className="size-full object-cover transition-transform duration-500 ease-out group-hover:scale-[1.06]"
            />
          ) : null}
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2">
            <Chip tone={categoryTone[item.category]}>{t(`categories.${item.category}`)}</Chip>
            <span className="flex min-w-0 items-center gap-1.5">
              <NewDot on={isNew} />
              <span className="truncate text-[11.5px] text-ash-300">
                {formatRelative(item.publishedAt)}
              </span>
            </span>
          </span>
          <span className="mt-1 line-clamp-2 text-[13px] leading-snug font-semibold text-bone-100 group-hover:text-bone-50">
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
          <span className="flex items-center gap-2">
            <MinecraftChip />
            <span className="truncate text-[11.5px] text-ash-300">
              {formatRelative(item.publishedAt)}
            </span>
          </span>
          <span className="mt-1 line-clamp-2 text-[13px] leading-snug font-medium text-bone-100/90 transition-colors duration-150 group-hover:text-bone-50">
            {text.title}
          </span>
        </span>
      </Link>
    </motion.li>
  );
}

/** Bullet lines of the patch notes box (a repeated line is shown once). */
function Highlights({ lines }: { lines: string[] }) {
  return (
    <ul className="mt-2 space-y-1.5">
      {[...new Set(lines)].map((line) => (
        <li
          key={line}
          className="flex gap-2 text-[12.5px] leading-snug text-ash-300 [overflow-wrap:anywhere]"
        >
          <span className="mt-[6px] size-1 shrink-0 bg-gold-500" />
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
          <span className="text-[12px] text-gold-300">
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
          <span className="flex shrink-0 items-center gap-1 text-[12px] text-gold-300">
            {formatRelative(notes.publishedAt)}
            <CaretRight
              size={12}
              weight="bold"
              aria-hidden
              className="transition-transform duration-200 ease-out group-hover:translate-x-0.5"
            />
          </span>
        </span>
        <span className="mt-1 block truncate text-[13px] font-semibold text-bone-100 group-hover:text-bone-50">
          {notes.title}
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

/** "5 gün 3 sa", "2 sa 14 dk": time left in the other events list. */
function spanText(ms: number): string {
  const day = 86_400_000;
  if (ms < day) return formatDuration(ms / 1000);
  const days = Math.floor(ms / day);
  const hours = Math.floor((ms % day) / 3_600_000);
  return hours
    ? i18n.t("reminders.daysHours", { days, hours })
    : i18n.t("reminders.days", { days });
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
  return <p className="mt-0.5 text-[12.5px] text-ash-300 tabular-nums">{startMoment(iso, now)}</p>;
}

/**
 * The list behind "+1 etkinlik": the other live and upcoming events, upcoming ones with their
 * reminder bell. It opens beside the card (anchored to the whole event block), so the card stays
 * readable; its times are as of the moment it opened.
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
                    <span className="font-medium text-gold-300">{t("home.eventRunning")}</span>
                  ) : null}
                  <span className="tabular-nums">
                    {live
                      ? event.openEnded
                        ? t("home.eventStarted", { when: formatRelative(event.startsAt, now) })
                        : t("home.eventEndsIn", { when: spanText(Date.parse(event.endsAt) - now) })
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

/** "20 dakika önce başladı", kept fresh while it is on screen. */
function StartedAgo({ at }: { at: string }) {
  const { t } = useTranslation();
  const [, tick] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => tick((n) => n + 1), 30_000);
    return () => window.clearInterval(id);
  }, []);
  return (
    <p className="mt-1.5 text-[12.5px] text-ash-300">
      {t("home.eventStarted", { when: formatRelative(at) })}
    </p>
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
  const units = {
    days: t("countdown.days"),
    hours: t("countdown.hours"),
    minutes: t("countdown.minutes"),
    seconds: t("countdown.seconds"),
  };
  // Reduced motion keeps the cross-fade only.
  const shift = glide ? 6 : 0;
  const enter = 0.28;
  const hasTarget = Boolean(event.newsId || event.url);
  const [listOpen, setListOpen] = useState(false);
  const titleId = useId();
  return (
    <Popover.Root open={listOpen} onOpenChange={setListOpen}>
      <Popover.Anchor asChild>
        <section className="relative border-t border-white/7 px-3.5 pt-1.5">
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
                <div className="flex items-center justify-between gap-2">
                  <span
                    className={cn(
                      "text-[12px] font-medium",
                      live ? "text-gold-300" : "text-ash-300",
                    )}
                  >
                    {live ? t("home.eventLive") : t("home.nextEvent")}
                  </span>
                  {event.newsId || (event.url && !isWebLink(event.url)) ? (
                    // Inside the launcher (an article, a projects:// link).
                    <CaretRight
                      size={13}
                      weight="bold"
                      aria-hidden
                      className="text-ash-400 transition-[color,transform] duration-200 ease-out group-hover:translate-x-0.5 group-hover:text-gold-300"
                    />
                  ) : event.url ? (
                    <ArrowSquareOut
                      size={14}
                      aria-hidden
                      className="text-ash-400 transition-colors duration-200 group-hover:text-gold-300"
                    />
                  ) : null}
                </div>
                <p
                  id={titleId}
                  className="mt-0.5 line-clamp-2 font-display text-[14px] leading-snug font-semibold text-bone-50"
                >
                  {event.title}
                </p>
                {live ? null : <StartsAt iso={event.startsAt} />}
                {live && event.openEnded ? (
                  <StartedAgo at={event.startsAt} />
                ) : (
                  <div className="mt-2 flex items-end gap-3">
                    <Countdown to={new Date(live ? event.endsAt : event.startsAt)} units={units} />
                    {live ? (
                      <span className="text-[11px] font-medium text-ash-300">
                        {t("home.eventLeft")}
                      </span>
                    ) : null}
                  </div>
                )}
              </EventTarget>
              {live ? (
                // On the server: "Katıl" takes the bell's place, level with the countdown's digits
                // (or with the "started" line of an open-ended event) and flush with the caret.
                event.join ? (
                  <EventJoin
                    describedBy={titleId}
                    className={cn(
                      "absolute right-0",
                      event.openEnded ? "bottom-[1px]" : "bottom-[22px]",
                    )}
                  />
                ) : null
              ) : (
                // Level with the countdown's digits; its glyph lines up with the caret above.
                <ReminderBell event={event} className="absolute right-[-6px] bottom-[22px]" />
              )}
            </motion.div>
          </AnimatePresence>
          {others.length ? (
            <Popover.Trigger asChild>
              <button
                type="button"
                className={cn(
                  "hit-area absolute top-[11px] cursor-pointer rounded-full bg-white/8 px-2 py-0.5 text-[11px] font-medium text-ash-300 tabular-nums transition-[color,background-color,transform] duration-150 [--hit:6px] hover:bg-white/12 hover:text-bone-50 focus-visible:outline-2 active:scale-[0.96] data-[state=open]:bg-white/12 data-[state=open]:text-bone-50",
                  hasTarget ? "right-[38px]" : "right-3.5",
                )}
              >
                {t("home.moreEvents", { count: others.length })}
              </button>
            </Popover.Trigger>
          ) : null}
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
  // The patch notes box already shows its article; the hero shows its own.
  const boxed = notes?.kind === "news" ? notes.newsId : undefined;
  const news = (content?.news ?? [])
    .filter((n) => n.id !== content?.hero.newsId && n.id !== boxed)
    .slice(0, NEWS_COLUMN_ROWS);
  const watermark = useNewsSeen((s) => s.watermark);
  const read = useNewsSeen((s) => s.read);
  const { event, live, others } = useEventCard(content?.events);
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
      <div className="h-2.5" />
    </motion.aside>
  );
}
