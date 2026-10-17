// Studio's announcement banners on the home screen: short lines over the art, opening their link
// when they have one; a closable banner stays closed on this computer once the player closes it.
import {
  ArrowSquareOut,
  CalendarStar,
  Info,
  Warning,
  WarningOctagon,
  X,
} from "@phosphor-icons/react";
import { cn, easeOutExpo, exitOf, IconButton, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import type { BannerItem } from "@/lib/api";
import { isWebLink, openLink } from "@/lib/links";

const STORAGE_KEY = "pl.home.dismissedBanners";
/** Closed banners remembered (the oldest are forgotten first). */
const KEEP = 50;

const listeners = new Set<() => void>();

function readDismissed(): string {
  try {
    return localStorage.getItem(STORAGE_KEY) ?? "[]";
  } catch {
    return "[]";
  }
}

function dismissedIds(raw: string): Set<string> {
  try {
    const parsed: unknown = JSON.parse(raw);
    return new Set(Array.isArray(parsed) ? parsed.filter((v) => typeof v === "string") : []);
  } catch {
    return new Set();
  }
}

function dismiss(id: string) {
  const ids = [...dismissedIds(readDismissed())].filter((v) => v !== id);
  ids.push(id);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(ids.slice(-KEEP)));
  } catch {
    // Storage is full or blocked: the banner still closes for this session.
  }
  for (const notify of listeners) notify();
}

function useDismissed(): Set<string> {
  const raw = useSyncExternalStore(
    (notify) => {
      listeners.add(notify);
      return () => listeners.delete(notify);
    },
    readDismissed,
    () => "[]",
  );
  return dismissedIds(raw);
}

/** The banners that have not ended, dropping each one when its time is up. */
function useCurrent(banners: BannerItem[]): BannerItem[] {
  const [now, setNow] = useState(() => Date.now());
  const next = banners
    .map((b) => (b.endsAt ? Date.parse(b.endsAt) : Number.NaN))
    .filter((t) => t > now)
    .sort((a, b) => a - b)[0];
  useEffect(() => {
    if (next === undefined) return;
    // setTimeout fires at once beyond ~24.8 days: wake up early then and look again.
    const wait = Math.min(next - Date.now() + 250, 2_000_000_000);
    const id = window.setTimeout(() => setNow(Date.now()), Math.max(0, wait));
    return () => window.clearTimeout(id);
  }, [next]);
  return banners.filter((b) => !b.endsAt || Date.parse(b.endsAt) > now);
}

const tone = {
  info: { icon: Info, className: "text-ash-300" },
  warning: { icon: Warning, className: "text-gold-300" },
  event: { icon: CalendarStar, className: "text-gold-300" },
  critical: { icon: WarningOctagon, className: "text-flare-400" },
} as const;

export function HomeBanners({ banners }: { banners: BannerItem[] | undefined }) {
  const { t } = useTranslation();
  const dismissed = useDismissed();
  const current = useCurrent(banners ?? []).filter((b) => !(b.dismissible && dismissed.has(b.id)));
  const { reduce, layout } = useMotionPrefs();
  const lift = reduce ? "translateY(0px)" : "translateY(-6px)";

  return (
    <ul className="flex flex-col items-start gap-2">
      <AnimatePresence initial={false}>
        {current.map((banner) => {
          const style = tone[banner.level] ?? tone.info;
          const BannerIcon = style.icon;
          const url = banner.url;
          const text = (
            <>
              <BannerIcon size={16} weight="fill" className={cn("shrink-0", style.className)} />
              <span className="min-w-0 text-left">{banner.text}</span>
              {isWebLink(url) ? (
                <ArrowSquareOut size={14} className="shrink-0 text-ash-300" />
              ) : null}
            </>
          );
          return (
            <motion.li
              key={banner.id}
              layout={layout ? "position" : false}
              initial={{ opacity: 0, transform: lift }}
              animate={{
                opacity: 1,
                transform: "translateY(0px)",
                transition: { duration: 0.32, ease: easeOutExpo },
              }}
              exit={{
                opacity: 0,
                transform: lift,
                transition: { duration: exitOf(0.32), ease: easeOutExpo },
              }}
              className={cn(
                "glass flex max-w-[540px] items-center gap-1 rounded-full text-[13px] text-bone-100",
                banner.level === "critical" && "ring-1 ring-flare-400/35",
              )}
            >
              {url ? (
                <button
                  type="button"
                  onClick={() => openLink(url)}
                  className="flex min-w-0 cursor-pointer items-center gap-2.5 rounded-full py-1.5 pl-3.5 transition-colors duration-150 hover:text-bone-50 focus-visible:outline-2"
                >
                  {text}
                </button>
              ) : (
                <span className="flex min-w-0 items-center gap-2.5 py-1.5 pl-3.5">{text}</span>
              )}
              {banner.dismissible ? (
                <IconButton
                  size="sm"
                  label={t("home.dismissBanner")}
                  icon={<X size={13} weight="bold" />}
                  onClick={() => dismiss(banner.id)}
                  className="mr-0.5 shrink-0 rounded-full"
                />
              ) : (
                <span className="w-3 shrink-0" />
              )}
            </motion.li>
          );
        })}
      </AnimatePresence>
    </ul>
  );
}
