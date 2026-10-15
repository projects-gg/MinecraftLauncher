import {
  type Icon,
  GearSix,
  House,
  Newspaper,
  Robot,
  SquaresFour,
} from "@phosphor-icons/react";
import { cn, PlayerHead, Tooltip } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { Link, useRouterState } from "@tanstack/react-router";
import { motion } from "motion/react";
import { useTranslation } from "react-i18next";
import logo from "@/assets/brand/logo-192.webp";
import { api } from "@/lib/api";
import { detectOs } from "@/lib/platform";

interface Item {
  to: "/" | "/library" | "/news" | "/afk" | "/settings";
  icon: Icon;
  label: string;
}

function RailLink({ item, active }: { item: Item; active: boolean }) {
  const IconCmp = item.icon;
  return (
    <Tooltip content={item.label} side="right">
      <Link
        to={item.to}
        aria-label={item.label}
        aria-current={active ? "page" : undefined}
        className="group relative flex size-12 items-center justify-center rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        {active ? (
          <motion.span
            layoutId="rail-active"
            className="chamfer absolute -left-[13px] h-7 w-[5px] bg-gold-400 [--cut:3px]"
            transition={{ type: "spring", stiffness: 420, damping: 34 }}
          />
        ) : null}
        <span
          className={cn(
            "absolute inset-1 rounded-lg transition-colors duration-150",
            active
              ? "bg-[radial-gradient(circle_at_50%_50%,rgb(232_154_44/0.22),transparent_70%)]"
              : "group-hover:bg-white/6",
          )}
        />
        <IconCmp
          size={24}
          weight={active ? "duotone" : "regular"}
          className={cn(
            "relative transition-colors duration-150",
            active ? "text-gold-300" : "text-ash-400 group-hover:text-bone-100",
          )}
        />
      </Link>
    </Tooltip>
  );
}

export function NavRail() {
  const { t } = useTranslation();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const { data: accounts } = useQuery({ queryKey: ["accounts"], queryFn: () => api.accounts() });
  const active = accounts?.find((a) => a.active);
  const mac = detectOs() === "macos";

  const top: Item[] = [
    { to: "/", icon: House, label: t("nav.home") },
    { to: "/library", icon: SquaresFour, label: t("nav.library") },
    { to: "/news", icon: Newspaper, label: t("nav.news") },
    { to: "/afk", icon: Robot, label: t("nav.afk") },
  ];
  const isActive = (to: string) => (to === "/" ? pathname === "/" : pathname.startsWith(to));

  return (
    <nav
      aria-label="Ana menü"
      className={cn(
        "glass relative z-20 flex w-[72px] shrink-0 flex-col items-center border-y-0 border-l-0 pb-4",
        mac ? "pt-11" : "pt-3",
      )}
    >
      <Link to="/" aria-label={t("nav.home")} className="group mb-5 flex size-12 items-center justify-center">
        <img
          src={logo}
          alt=""
          className="size-11 drop-shadow-[0_0_10px_rgb(232_154_44/0.35)] transition-[filter,transform] duration-300 group-hover:scale-105 group-hover:drop-shadow-[0_0_16px_rgb(240_184_73/0.7)]"
          draggable={false}
        />
      </Link>
      <div className="flex flex-col gap-1.5">
        {top.map((item) => (
          <RailLink key={item.to} item={item} active={isActive(item.to)} />
        ))}
      </div>
      <div className="mt-auto flex flex-col items-center gap-2">
        <RailLink item={{ to: "/settings", icon: GearSix, label: t("nav.settings") }} active={isActive("/settings")} />
        {active ? (
          <Tooltip content={active.username} side="right">
            <Link
              to="/settings"
              search={{ section: "accounts" }}
              aria-label={t("nav.accounts")}
              className="mt-1 rounded-[6px] p-0.5 ring-1 ring-white/10 transition hover:ring-gold-400/60"
            >
              <PlayerHead name={active.username} src={active.headUrl} size={36} />
            </Link>
          </Tooltip>
        ) : null}
      </div>
    </nav>
  );
}
