import { GearSix, House, type Icon, Newspaper, Robot, SquaresFour } from "@phosphor-icons/react";
import { cn, Tooltip } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { Link, useRouterState } from "@tanstack/react-router";
import { motion } from "motion/react";
import { useTranslation } from "react-i18next";
import logo from "@/assets/brand/logo-192.webp";
import { AccountFace } from "@/features/accounts/account-face";
import { useAccountsUi } from "@/features/accounts/store";
import { FriendsRailEntry } from "@/features/social/rail-entry";
import { api } from "@/lib/api";
import { detectOs } from "@/lib/platform";
import { ariaKeyShortcuts, type ShortcutId } from "@/lib/shortcuts";
import { isNewsNew, useNewsSeen } from "@/stores/news-seen";
import { ShortcutKeys } from "./shortcut-keys";

interface Item {
  to: "/" | "/library" | "/news" | "/afk" | "/settings";
  icon: Icon;
  label: string;
  /** The page's shortcut (Ctrl+1 …): its keys in the tooltip, `aria-keyshortcuts` on the link. */
  shortcut?: ShortcutId;
  /**
   * Something new waits behind the link: a small, still gold dot at the icon's corner (it fades
   * in and out), and this name for screen readers while it shows.
   */
  marker?: { on: boolean; label: string };
}

function RailLink({ item, active }: { item: Item; active: boolean }) {
  const IconCmp = item.icon;
  const marked = item.marker?.on === true;
  return (
    <Tooltip
      content={
        item.shortcut ? (
          <span className="flex items-center gap-2">
            {item.label}
            <ShortcutKeys id={item.shortcut} />
          </span>
        ) : (
          item.label
        )
      }
      side="right"
    >
      <Link
        to={item.to}
        aria-label={marked && item.marker ? item.marker.label : item.label}
        aria-current={active ? "page" : undefined}
        aria-keyshortcuts={item.shortcut ? ariaKeyShortcuts(item.shortcut, detectOs()) : undefined}
        className="group relative flex size-[40px] items-center justify-center rounded-lg outline-none transition-transform duration-150 ease-out focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.94]"
      >
        {active ? (
          <motion.span
            layoutId="rail-active"
            className="chamfer absolute -left-[10px] h-6 w-[4px] bg-gold-400 [--cut:3px]"
            transition={{ type: "spring", stiffness: 420, damping: 34 }}
          />
        ) : null}
        <span
          className={cn(
            "absolute inset-[3px] rounded-lg transition-colors duration-150",
            active
              ? "bg-[radial-gradient(circle_at_50%_50%,rgb(232_154_44/0.22),transparent_70%)]"
              : "group-hover:bg-white/6",
          )}
        />
        <IconCmp
          size={22}
          weight={active ? "duotone" : "regular"}
          className={cn(
            "relative transition-colors duration-150",
            active ? "text-gold-300" : "text-ash-400 group-hover:text-bone-100",
          )}
        />
        {item.marker ? (
          <span
            aria-hidden
            className={cn(
              "absolute top-[6px] right-[6px] size-1.5 rounded-full bg-gold-400 transition-opacity duration-200 ease-out",
              marked ? "opacity-100" : "opacity-0",
            )}
          />
        ) : null}
      </Link>
    </Tooltip>
  );
}

/** Whether news the player has not seen waits (not while the news page is open). */
function useNewsWaiting(onNewsPage: boolean): boolean {
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const watermark = useNewsSeen((s) => s.watermark);
  const read = useNewsSeen((s) => s.read);
  if (onNewsPage || !data) return false;
  const now = Date.now();
  return data.news.some((item) => isNewsNew(item, { watermark, read }, now));
}

export function NavRail() {
  const { t } = useTranslation();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const { data: accounts } = useQuery({ queryKey: ["accounts"], queryFn: () => api.accounts() });
  const active = accounts?.find((a) => a.active);
  const openCenter = useAccountsUi((s) => s.openCenter);
  const mac = detectOs() === "macos";
  const isActive = (to: string) => (to === "/" ? pathname === "/" : pathname.startsWith(to));
  const newsWaiting = useNewsWaiting(isActive("/news"));

  const top: Item[] = [
    { to: "/", icon: House, label: t("nav.home"), shortcut: "home" },
    { to: "/library", icon: SquaresFour, label: t("nav.library"), shortcut: "library" },
    {
      to: "/news",
      icon: Newspaper,
      label: t("nav.news"),
      shortcut: "news",
      marker: { on: newsWaiting, label: t("reminders.railNews", { label: t("nav.news") }) },
    },
    { to: "/afk", icon: Robot, label: t("nav.afk"), shortcut: "afk" },
  ];

  return (
    <nav
      aria-label={t("reminders.mainMenu")}
      className={cn(
        "glass relative z-20 flex w-rail shrink-0 flex-col items-center border-y-0 border-l-0 pb-3",
        mac ? "pt-10" : "pt-2.5",
      )}
    >
      {/* The logo goes home too, but the house link is the keyboard's stop (no second "Ana
          Sayfa" in the tab order). */}
      <Link
        to="/"
        aria-label={t("nav.home")}
        tabIndex={-1}
        className="group mb-4 flex size-10 items-center justify-center"
      >
        <img
          src={logo}
          alt=""
          className="size-[34px] transition-[transform,scale] duration-300 group-hover:scale-105 motion-reduce:group-hover:scale-100"
          draggable={false}
        />
      </Link>
      <div className="flex flex-col gap-1">
        {top.map((item) => (
          <RailLink key={item.to} item={item} active={isActive(item.to)} />
        ))}
      </div>
      <div className="mt-auto flex flex-col items-center gap-1.5">
        {/* Only when the player moved the friends button here (its home is the title bar). */}
        <FriendsRailEntry />
        <RailLink
          item={{ to: "/settings", icon: GearSix, label: t("nav.settings"), shortcut: "settings" }}
          active={isActive("/settings")}
        />
        {active ? (
          <Tooltip content={active.username} side="right">
            {/* The account center opens over the page (as from the title bar), the page stays. */}
            <button
              type="button"
              aria-label={t("accounts.center.open")}
              onClick={(event) => openCenter({ from: event.currentTarget })}
              className="mt-1 cursor-pointer rounded-[6px] p-[3px] ring-1 ring-white/10 transition-[box-shadow,transform,scale] duration-150 ease-out hover:ring-gold-400/60 active:scale-[0.94]"
            >
              {/* 32 px: head pixels stay whole at 100, 125, 150 and 200% scaling. */}
              <AccountFace account={active} size={32} />
            </button>
          </Tooltip>
        ) : null}
      </div>
    </nav>
  );
}
