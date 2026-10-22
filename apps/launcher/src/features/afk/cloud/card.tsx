// A cloud account on the AFK page, the desktop card's twin: face, name and server, the state line,
// the paid time with "Süreyi uzat" once renewal opens, and one main button (Başlat, Durdur, or
// "Yeniden etkinleştir" on the site once the time is up), the console and the settings.
import {
  ArrowSquareOut,
  ArrowsClockwise,
  DotsThreeVertical,
  GearSix,
  Play,
  Stop,
  Terminal,
} from "@phosphor-icons/react";
import {
  Button,
  Chip,
  ContextMenu,
  cn,
  easeOutExpo,
  IconButton,
  Menu,
  type MenuItem,
  SwapLabel,
  useMotionPrefs,
  usePressGuard,
} from "@pl/ui";
import { motion, type Transition } from "motion/react";
import type { MouseEvent, Ref } from "react";
import { useTranslation } from "react-i18next";
import { AfkHead } from "../afk-head";
import { afkFailed } from "../errors";
import { type CloudAccount, cloudApi } from "./api";
import { CloudStateLine } from "./state-line";
import { type CloudPrimary, cloudPrimary, isInGame, serverLabel, timeLeft } from "./status";

const glide: Transition = { type: "spring", stiffness: 420, damping: 36, mass: 0.9 };
const REST = "translateY(0px) scale(1)";
const ORDER: CloudPrimary[] = ["start", "stop", "renew"];

/** The paid time, with "Süreyi uzat" beside it once renewal opens (the site renews). */
function TimeRow({ account, skewMs }: { account: CloudAccount; skewMs: number }) {
  const { t } = useTranslation();
  const now = Date.now() + skewMs;
  const renew = () =>
    cloudApi
      .open("renew", account.id)
      .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));
  return (
    <p className="mt-1.5 flex min-h-5 flex-wrap items-center gap-x-3 gap-y-1 pl-[23px] text-[12px]">
      <span
        className={cn(
          "tabular-nums",
          account.renewal.open && !account.expired ? "text-gold-200" : "text-ash-300",
        )}
      >
        {timeLeft(account, now, t)}
      </span>
      {account.renewal.open && !account.expired ? (
        <button
          type="button"
          onClick={() => void renew()}
          className="inline-flex cursor-pointer items-center gap-1 rounded-sm font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          {t("cloudAfk.actions.renew")}
          <ArrowSquareOut size={12} weight="bold" aria-hidden />
        </button>
      ) : null}
    </p>
  );
}

function PrimaryButton({
  account,
  onStart,
  onStop,
  className,
}: {
  account: CloudAccount;
  onStart: () => void;
  onStop: () => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const action = cloudPrimary(account);
  // The word that takes the place of the one just pressed never takes the rest of that double
  // click (@pl/ui press-guard), as on the desktop cards.
  const guard = usePressGuard(action);
  if (action === "none") return null;
  const labels: Record<Exclude<CloudPrimary, "none">, string> = {
    start: t("cloudAfk.actions.start"),
    stop: t("cloudAfk.actions.stop"),
    renew: t("cloudAfk.actions.reactivate"),
  };
  const icons = {
    start: <Play size={14} weight="fill" />,
    stop: <Stop size={14} weight="fill" />,
    renew: <ArrowSquareOut size={14} weight="bold" />,
  };
  const press = (event: MouseEvent) => {
    if (!guard.take(event)) return;
    if (action === "start") onStart();
    else if (action === "stop") onStop();
    else
      void cloudApi
        .open("renew", account.id)
        .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));
  };
  return (
    <Button
      size="sm"
      variant={action === "stop" ? "secondary" : "primary"}
      icon={icons[action]}
      aria-disabled={guard.armed ? undefined : true}
      onClick={press}
      {...guard.handlers}
      className={className}
    >
      <SwapLabel
        labels={ORDER.map((a) => labels[a as Exclude<CloudPrimary, "none">])}
        active={ORDER.indexOf(action)}
      />
    </Button>
  );
}

export function CloudCard({
  account,
  skewMs,
  index,
  onStart,
  onStop,
  onRestart,
  onConsole,
  onSettings,
  ref,
}: {
  account: CloudAccount;
  skewMs: number;
  index: number;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onConsole: () => void;
  onSettings: () => void;
  ref?: Ref<HTMLLIElement>;
}) {
  const { t } = useTranslation();
  const { reduce, layout } = useMotionPrefs();
  const inGame = isInGame(account);
  const action = cloudPrimary(account);
  const openSite = () =>
    void cloudApi
      .open("account", account.id)
      .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));

  const items: Array<MenuItem | null> = [
    action === "start"
      ? { label: t("cloudAfk.actions.start"), icon: <Play size={16} />, onSelect: onStart }
      : action === "stop"
        ? { label: t("cloudAfk.actions.stop"), icon: <Stop size={16} />, onSelect: onStop }
        : null,
    ...(inGame
      ? [
          {
            label: t("cloudAfk.actions.reconnect"),
            icon: <ArrowsClockwise size={16} />,
            onSelect: onRestart,
          },
        ]
      : []),
    { label: t("cloudAfk.actions.console"), icon: <Terminal size={16} />, onSelect: onConsole },
    { label: t("cloudAfk.actions.settings"), icon: <GearSix size={16} />, onSelect: onSettings },
    null,
    ...(account.renewal.open || account.expired
      ? [
          {
            label: account.expired ? t("cloudAfk.actions.reactivate") : t("cloudAfk.actions.renew"),
            icon: <ArrowSquareOut size={16} />,
            onSelect: () =>
              void cloudApi
                .open("renew", account.id)
                .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed"))),
          },
        ]
      : []),
    {
      label: t("cloudAfk.actions.openSite"),
      icon: <ArrowSquareOut size={16} />,
      onSelect: openSite,
    },
  ];

  return (
    <motion.li
      ref={ref}
      layout={layout ? "position" : false}
      transition={{ layout: glide }}
      initial={{ opacity: 0, transform: reduce ? REST : "translateY(10px) scale(1)" }}
      animate={{
        opacity: 1,
        transform: REST,
        transition: { duration: 0.35, delay: Math.min(index, 8) * 0.04, ease: easeOutExpo },
      }}
      exit={{
        opacity: 0,
        transform: reduce ? REST : "translateY(0px) scale(0.97)",
        transition: { duration: 0.18, ease: easeOutExpo },
      }}
      className="flex"
    >
      <ContextMenu items={items}>
        <div
          className={cn(
            "glass relative flex w-full flex-col rounded-[var(--radius-panel)] p-3.5",
            inGame && "shadow-[inset_0_0_0_1px_rgb(95_203_142/0.22)]",
          )}
        >
          <div className="flex items-start gap-3">
            <AfkHead nickname={account.name} size={40} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <h3 className="truncate font-display text-[15px] font-semibold text-bone-50">
                  {account.name}
                </h3>
                {account.hold?.kind === "suspended" ? (
                  <Chip className="shrink-0">{t("cloudAfk.state.suspended")}</Chip>
                ) : null}
              </div>
              <p
                className="truncate text-[12.5px] text-ash-300"
                title={serverLabel(account.server)}
                data-selectable
              >
                {serverLabel(account.server)}
              </p>
            </div>
            <Menu
              trigger={
                <IconButton
                  label={t("cloudAfk.actions.more")}
                  icon={<DotsThreeVertical size={18} weight="bold" />}
                  size="sm"
                />
              }
              items={items}
            />
          </div>

          <div className="mt-3 flex min-h-5 items-center gap-3">
            <CloudStateLine account={account} skewMs={skewMs} className="flex-1" showHint />
          </div>
          <TimeRow account={account} skewMs={skewMs} />

          {/* Pinned to the bottom: the buttons of a row of cards line up. */}
          <div className="mt-auto flex gap-2 pt-3">
            <PrimaryButton account={account} onStart={onStart} onStop={onStop} className="flex-1" />
            <Button size="sm" variant="ghost" icon={<Terminal size={15} />} onClick={onConsole}>
              {t("cloudAfk.actions.console")}
            </Button>
            <IconButton
              label={t("cloudAfk.actions.settings")}
              icon={<GearSix size={17} />}
              size="sm"
              onClick={onSettings}
            />
          </div>
        </div>
      </ContextMenu>
    </motion.li>
  );
}
