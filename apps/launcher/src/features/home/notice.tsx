// The notice at the top of the hero column: Studio's maintenance notice, or else an active problem
// on the server's status page (from the Discord bot's feed). One at a time and Studio first. It
// drops in with a fade, leaves with a fade (lifted out of the flow, so the strips below glide up
// at once) and a change of wording cross-fades in place. It is information only: nothing waits
// for it (docs/design.md §5).
import {
  ArrowSquareOut,
  type Icon,
  Megaphone,
  Warning,
  WarningOctagon,
  Wrench,
} from "@phosphor-icons/react";
import { cn, easeOutExpo, exitOf, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type HomeStatus, type Maintenance } from "@/lib/api";
import { dateTimeFormat } from "@/lib/format";
import { isWebLink, openWebLink } from "@/lib/links";
import { useWindowAway } from "@/lib/window-away";
import { nextLook } from "./status-recheck";

/** Entrance length; the exit and the strips' glide follow it. */
export const NOTICE_ENTER = 0.32;

type Tone = "maintenance" | HomeStatus["level"];

const tones: Record<Tone, { icon: Icon; className: string; ring?: string }> = {
  maintenance: { icon: Wrench, className: "text-gold-300" },
  // Services are down: the one notice in the error colour, with a faint ring.
  outage: { icon: WarningOctagon, className: "text-flare-400", ring: "ring-1 ring-flare-400/30" },
  degraded: { icon: Warning, className: "text-gold-300" },
  notice: { icon: Megaphone, className: "text-gold-300" },
};

/**
 * The status notice until its `expiresAt`, then nothing, without waiting for a refresh (a bot
 * that stopped leaves no stale notice behind). Call it where the strips below are rendered too,
 * so they glide when it leaves.
 */
export function useActiveStatus(status: HomeStatus | undefined): HomeStatus | undefined {
  const [now, setNow] = useState(() => Date.now());
  const end = status ? Date.parse(status.expiresAt) : Number.NaN;
  useEffect(() => {
    if (!(end > Date.now())) return;
    // setTimeout fires at once beyond ~24.8 days; a notice lives 15 minutes at most anyway.
    const wait = Math.min(end - Date.now() + 50, 2_000_000_000);
    const id = window.setTimeout(() => setNow(Date.now()), Math.max(0, wait));
    return () => window.clearTimeout(id);
  }, [end]);
  return status && end > now ? status : undefined;
}

/**
 * While a status notice is on screen (and the window is), asks the servers again now and then
 * and once shortly before the notice expires (`nextLook`): it leaves soon after the problem is
 * over and stays while the bot keeps renewing it (a renewal changes `expiresAt` and starts the
 * plan over). Nothing is polled otherwise.
 */
function useStatusRecheck(expiresAt: string | undefined) {
  const away = useWindowAway();
  useEffect(() => {
    if (!expiresAt || away) return;
    const end = Date.parse(expiresAt);
    let id = 0;
    const schedule = () => {
      const look = nextLook(end, Date.now());
      if (!look) return;
      id = window.setTimeout(() => {
        void api.contentRecheck();
        if (!look.last) schedule();
      }, look.wait);
    };
    schedule();
    return () => window.clearTimeout(id);
  }, [expiresAt, away]);
}

function NoticeCard({
  tone,
  title,
  message,
  footer,
}: {
  tone: Tone;
  title: string;
  message?: string;
  footer?: ReactNode;
}) {
  const style = tones[tone];
  const NoticeIcon = style.icon;
  return (
    <div
      className={cn(
        // As wide as its words (up to 540 px), like the strips under it.
        "glass flex w-fit max-w-[540px] gap-3 rounded-[var(--radius-panel)] py-3 pr-4 pl-3.5",
        style.ring,
      )}
    >
      <NoticeIcon
        size={18}
        weight="duotone"
        aria-hidden
        className={cn("mt-0.5 shrink-0", style.className)}
      />
      <div className="min-w-0">
        <p className="text-[14px] font-semibold text-balance text-bone-50">{title}</p>
        {message ? (
          <p className="mt-1 text-[13px] leading-relaxed text-pretty text-bone-100/90">{message}</p>
        ) : null}
        {footer}
      </div>
    </div>
  );
}

function maintenanceUntil(until: string | undefined): string | null {
  if (!until) return null;
  return dateTimeFormat({
    day: "numeric",
    month: "long",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(until));
}

function StatusPageLink({ url }: { url: string }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={() => openWebLink(url)}
      className="-mx-1 mt-1.5 inline-flex cursor-pointer items-center gap-1 rounded-[4px] px-1 text-[12.5px] font-medium text-ash-300 transition-colors duration-150 hover:text-bone-50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-gold-300"
    >
      {t("home.statusPage")}
      <ArrowSquareOut size={13} aria-hidden />
    </button>
  );
}

/**
 * Studio's maintenance notice or, while there is none, the status notice (pass it through
 * `useActiveStatus`). A live region announces what comes in.
 */
export function HomeNotice({
  maintenance,
  status,
}: {
  maintenance: Maintenance | undefined;
  status: HomeStatus | undefined;
}) {
  const { t } = useTranslation();
  const { reduce, layout } = useMotionPrefs();
  const shownStatus = maintenance ? undefined : status;
  useStatusRecheck(shownStatus?.expiresAt);

  let key: string | null = null;
  let card: ReactNode = null;
  if (maintenance) {
    const until = maintenanceUntil(maintenance.until);
    key = "maintenance";
    card = (
      <NoticeCard
        tone="maintenance"
        title={maintenance.title}
        message={maintenance.message}
        footer={
          until ? (
            <p className="mt-1.5 text-[12.5px] text-ash-300">
              {t("home.maintenanceUntil", { when: until })}
            </p>
          ) : null
        }
      />
    );
  } else if (shownStatus) {
    // A renewed notice keeps its key (no motion); new wording cross-fades.
    key = `status:${shownStatus.level}:${shownStatus.title}:${shownStatus.message}`;
    card = (
      <NoticeCard
        tone={shownStatus.level}
        title={shownStatus.title}
        message={shownStatus.message}
        footer={
          shownStatus.url && isWebLink(shownStatus.url) ? (
            <StatusPageLink url={shownStatus.url} />
          ) : null
        }
      />
    );
  }

  const lift = reduce ? "translateY(0px)" : "translateY(-6px)";
  return (
    <div role="status" aria-live="polite" className="relative">
      <AnimatePresence mode="popLayout" initial={false}>
        {key ? (
          <motion.div
            key={key}
            className="pb-2"
            initial={{ opacity: 0, transform: lift }}
            animate={{
              opacity: 1,
              transform: "translateY(0px)",
              // The strips below glide out of the way first (most of the way in 90 ms), so the
              // notice never shows through them.
              transition: { duration: NOTICE_ENTER, ease: easeOutExpo, delay: layout ? 0.09 : 0 },
            }}
            exit={{
              opacity: 0,
              transform: lift,
              transition: { duration: exitOf(NOTICE_ENTER), ease: easeOutExpo },
            }}
          >
            {card}
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}
