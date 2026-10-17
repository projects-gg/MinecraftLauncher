import { ArrowSquareOut, BookOpenText, WifiHigh, WifiSlash, Wrench } from "@phosphor-icons/react";
import { Button, Chip, Skeleton } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { FriendsInGameStrip } from "@/features/social/home-strip";
import { api, type HomeContent, type Maintenance } from "@/lib/api";
import { isWebLink, openLink } from "@/lib/links";
import { HomeBanners } from "./banners";
import { NewsColumn } from "./news-column";

/** The hero and news column rise in once per start (the boot moment); later visits to Home
 * arrive with the page cross-fade only. */
let homeArrived = false;

function useFirstArrival() {
  const [first] = useState(() => !homeArrived);
  useEffect(() => {
    homeArrived = true;
  }, []);
  return first;
}

function HeroCopy({ content }: { content: HomeContent }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const first = useFirstArrival();
  const featured = content.news.find((n) => n.id === content.hero.newsId);
  // Studio's button: its own text, opening the article or (without one) its link.
  const ctaLabel = content.hero.ctaLabel;
  const ctaUrl = featured ? undefined : content.hero.ctaUrl;

  return (
    <motion.div
      initial={first ? { opacity: 0, transform: "translateY(16px)" } : false}
      animate={{ opacity: 1, transform: "translateY(0px)" }}
      transition={{ duration: 0.7, ease: [0.16, 1, 0.3, 1], delay: 0.1 }}
      className="max-w-[min(540px,100%)]"
    >
      {featured ? (
        <Chip tone="onArt" className="mb-3">
          {t(`categories.${featured.category}`)}
        </Chip>
      ) : null}
      <h1 className="text-shadow-art font-display text-hero font-[640] tracking-[-0.01em] text-bone-50">
        {content.hero.title}
      </h1>
      <p className="text-shadow-art mt-3 max-w-[44ch] text-[15px] leading-normal text-bone-100/90">
        {content.hero.subtitle}
      </p>
      {ctaUrl && ctaLabel ? (
        <div className="mt-5 flex gap-2.5">
          <Button
            variant="secondary"
            size="lg"
            iconRight={isWebLink(ctaUrl) ? <ArrowSquareOut size={16} /> : undefined}
            onClick={() => openLink(ctaUrl)}
          >
            {ctaLabel}
          </Button>
        </div>
      ) : null}
      {featured ? (
        <div className="mt-5 flex gap-2.5">
          <Button
            variant="secondary"
            size="lg"
            icon={<BookOpenText size={18} weight="duotone" />}
            onClick={() => void navigate({ to: "/", search: { news: featured.id } })}
          >
            {ctaLabel ?? t("home.readNews")}
          </Button>
          {featured.cta ? (
            <Button
              variant="ghost"
              size="lg"
              iconRight={isWebLink(featured.cta.url) ? <ArrowSquareOut size={16} /> : undefined}
              onClick={() => featured.cta && openLink(featured.cta.url)}
            >
              {featured.cta.label}
            </Button>
          ) : null}
        </div>
      ) : null}
    </motion.div>
  );
}

/** Published from Studio while the server is down for maintenance. */
function MaintenanceNotice({ maintenance }: { maintenance: Maintenance }) {
  const { t } = useTranslation();
  const until = maintenance.until
    ? new Intl.DateTimeFormat("tr-TR", {
        day: "numeric",
        month: "long",
        hour: "2-digit",
        minute: "2-digit",
      }).format(new Date(maintenance.until))
    : null;
  return (
    <div
      role="status"
      className="glass flex max-w-[540px] gap-3 rounded-[var(--radius-panel)] px-3.5 py-3"
    >
      <Wrench size={18} weight="duotone" className="mt-0.5 shrink-0 text-gold-300" />
      <div className="min-w-0">
        <p className="text-[14px] font-semibold text-bone-50">{maintenance.title}</p>
        <p className="mt-1 text-[13px] leading-relaxed text-bone-100/90">{maintenance.message}</p>
        {until ? (
          <p className="mt-1.5 text-[12.5px] text-ash-300">
            {t("home.maintenanceUntil", { when: until })}
          </p>
        ) : null}
      </div>
    </div>
  );
}

/** How long "Yeniden çevrimiçisin" stays before the strip leaves. */
const BACK_ONLINE_MS = 2400;

/**
 * The calm offline strip: shown while the content servers can't be reached (cached or bundled
 * content is on screen), it briefly says the connection is back and then leaves.
 */
function ConnectionStrip() {
  const { t } = useTranslation();
  const reduce = useReducedMotion() ?? false;
  const { data } = useQuery({
    queryKey: ["content-status"],
    queryFn: () => api.contentStatus(),
    staleTime: 10_000,
  });
  const offline = data?.offline ?? false;
  const [backOnline, setBackOnline] = useState(false);
  const wasOffline = useRef(offline);
  useEffect(() => {
    const cameBack = wasOffline.current && !offline;
    wasOffline.current = offline;
    if (!cameBack) {
      if (offline) setBackOnline(false);
      return;
    }
    setBackOnline(true);
    const timer = setTimeout(() => setBackOnline(false), BACK_ONLINE_MS);
    return () => clearTimeout(timer);
  }, [offline]);

  const state = offline ? "offline" : backOnline ? "back" : null;
  const lift = reduce ? "none" : "translateY(-6px)";
  return (
    <div role="status" aria-live="polite">
      <AnimatePresence mode="wait" initial={false}>
        {state ? (
          <motion.div
            key={state}
            className="glass flex w-fit items-center gap-2 rounded-full px-3.5 py-1.5 text-[13px] text-bone-100"
            initial={{ opacity: 0, transform: lift }}
            animate={{
              opacity: 1,
              transform: "translateY(0px)",
              transition: { duration: 0.32, ease: [0.16, 1, 0.3, 1] },
            }}
            exit={{ opacity: 0, transform: lift, transition: { duration: 0.22, ease: "easeIn" } }}
          >
            {state === "offline" ? (
              <WifiSlash size={16} className="text-gold-300" />
            ) : (
              <WifiHigh size={16} className="text-jade-400" />
            )}
            {state === "offline" ? t("home.offlineBanner") : t("home.backOnline")}
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

export function HomePage() {
  const { data, isPending } = useQuery({
    queryKey: ["home-content"],
    queryFn: () => api.homeContent(),
  });

  return (
    <div className="flex min-h-0 flex-1 gap-5 px-page pt-2 pb-5">
      <section className="flex min-w-0 flex-1 flex-col justify-end pb-2">
        <div className="mb-auto space-y-2">
          {data?.maintenance ? <MaintenanceNotice maintenance={data.maintenance} /> : null}
          <HomeBanners banners={data?.banners} />
          <FriendsInGameStrip />
          <ConnectionStrip />
        </div>
        {isPending || !data ? (
          // Shaped like the hero (chip, headline, subtitle, button), so nothing moves when the
          // content arrives.
          <div className="max-w-[500px]">
            <Skeleton className="mb-3 h-5 w-20" />
            <Skeleton className="h-[44px] w-full" />
            <Skeleton className="mt-3 h-5 w-4/5" />
            <Skeleton className="mt-5 h-[38px] w-36" />
          </div>
        ) : (
          <HeroCopy content={data} />
        )}
      </section>
      <NewsColumn content={data} loading={isPending} />
    </div>
  );
}
