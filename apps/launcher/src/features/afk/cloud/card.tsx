// A cloud account on the AFK page, the desktop card's twin: face, name and server, the state line,
// the paid time with "Süreyi uzat" once renewal opens, and one main button (Başlat, Durdur, or
// "Yeniden etkinleştir" on the site once the time is up), the console and the settings. With the
// cloud features: "Envanter" and "Hareket" in its menu (the panel's tabs), "Ayarlarını kopyala", and
// a box while several accounts are chosen (the chosen card carries the brand's pixel brackets).
import {
  ArrowSquareOut,
  ArrowsClockwise,
  Backpack,
  CaretRight,
  ChatsCircle,
  Copy,
  DotsThreeVertical,
  GearSix,
  Play,
  SneakerMove,
  Stop,
  Terminal,
} from "@phosphor-icons/react";
import {
  Button,
  Chip,
  ContextMenu,
  cn,
  easeOutExpo,
  enter,
  IconButton,
  leave,
  Menu,
  type MenuItem,
  PixelBrackets,
  SwapLabel,
  useMotionPrefs,
  usePressGuard,
} from "@pl/ui";
import { AnimatePresence, motion, type Transition } from "motion/react";
import type { MouseEvent, Ref } from "react";
import { useTranslation } from "react-i18next";
import { Check } from "@/features/settings/cleanup/check";
import { AfkHead } from "../afk-head";
import { afkFailed } from "../errors";
import { type CloudAccount, cloudApi } from "./api";
import { AI_TONE, useAiSentence } from "./plus/rail";
import { plusAi } from "./plus/types";
import { CloudStateLine } from "./state-line";
import {
  type CloudPrimary,
  cloudPrimary,
  endsSoon,
  isInGame,
  serverLabel,
  timeLeft,
} from "./status";

const glide: Transition = { type: "spring", stiffness: 420, damping: 36, mass: 0.9 };
const REST = "translateY(0px) scale(1)";
const ORDER: CloudPrimary[] = ["start", "stop", "renew"];

/**
 * The paid time, gold in its last week (as on the site), with "Süreyi uzat" beside it while renewal
 * is open, which is at any time unless Studio keeps it to the last days (the site renews).
 */
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
        className={cn("tabular-nums", endsSoon(account, now) ? "text-gold-200" : "text-ash-300")}
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
  // Stopped until something is fixed (the state line's hint says what, "… düzelt, sonra başlat"):
  // "Başlat" steps back from gold, which called for a start that would stop again (SMALL11).
  // After a plain disconnect ("other") starting again is the fix, and it stays gold.
  const hold = account.hold;
  const fixFirst =
    action === "start" &&
    ((hold?.kind === "cause" && hold.cause !== "other") || hold?.kind === "server_blocked");
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
      variant={action === "stop" || fixFirst ? "secondary" : "primary"}
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

/**
 * An AFK+ account's line on its card: the assistant's state in one sentence and what waits for the
 * player (approvals in gold); the whole line opens the AFK+ panel.
 */
function PlusLine({ account, onOpen }: { account: CloudAccount; onOpen: () => void }) {
  const { t } = useTranslation();
  const sentence = useAiSentence(account, true);
  // The row has room for the state only: its first clause ("Kontörün bitti"); the rest ("13:07'de
  // yenilenir") is in the panel's rail and the row's tooltip, never cut mid-word.
  const brief = sentence.text.split("; ")[0] ?? sentence.text;
  const ai = plusAi(account);
  // A machine that cannot be asked has no current count (the panel shows none either).
  const approvals = ai && ai.state !== "unknown" ? ai.pendingApprovals : 0;
  return (
    <button
      type="button"
      onClick={onOpen}
      aria-label={t("afkPlus.card.openLabel", { name: account.name })}
      className={cn(
        "group/plus mt-3 flex w-full cursor-pointer items-center gap-2.5 rounded-md bg-white/[0.04] px-2.5 py-[7px] text-left outline-none",
        "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/[0.07] active:scale-[0.99] focus-visible:ring-2 focus-visible:ring-gold-300",
      )}
    >
      <span className="shrink-0 font-display text-[13px] font-semibold text-bone-50">
        {t("afkPlus.name")}
      </span>
      <span
        title={sentence.text}
        className={cn(
          "min-w-0 flex-1 truncate text-[12.5px] transition-colors duration-150",
          AI_TONE[sentence.tone],
        )}
      >
        {brief}
      </span>
      {approvals > 0 ? (
        <span className="shrink-0 font-display text-[12.5px] font-semibold text-gold-200 tabular-nums">
          {t("afkPlus.card.approvals", { count: approvals })}
        </span>
      ) : null}
      <CaretRight
        size={14}
        weight="bold"
        aria-hidden
        className="shrink-0 text-ash-400 transition-[color,transform,translate] duration-150 ease-out group-hover/plus:translate-x-0.5 group-hover/plus:text-bone-100 motion-reduce:group-hover/plus:translate-x-0"
      />
    </button>
  );
}

/** Menu items with a separator (null) only between groups: none first or last, never two in a row. */
function separated(items: Array<MenuItem | null>): Array<MenuItem | null> {
  const out: Array<MenuItem | null> = [];
  for (const item of items) {
    if (item === null && (out.length === 0 || out[out.length - 1] === null)) continue;
    out.push(item);
  }
  if (out[out.length - 1] === null) out.pop();
  return out;
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
  onPlus,
  onUpgrade,
  onInventory,
  onMovement,
  onCopy,
  selecting = false,
  selected = false,
  onSelect,
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
  /** An AFK+ account (while AFK+ shows): its line opens the panel. */
  onPlus?: () => void;
  /** A plain account the player may upgrade (the site does it). */
  onUpgrade?: () => void;
  /** While the service shows this account's inventory: the panel opens on it. */
  onInventory?: () => void;
  /** While this account's movement is on: the panel opens on Hareket. */
  onMovement?: () => void;
  /** "Ayarlarını kopyala" (several accounts, the cloud features on). */
  onCopy?: () => void;
  /** Several accounts are being chosen: a box instead of the menu, the card toggles it. */
  selecting?: boolean;
  selected?: boolean;
  onSelect?: (selected: boolean) => void;
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

  // Groups: the account itself, its panel's tabs, its settings, then the site. A separator stands
  // only between groups (never first, last or twice: the first group may be empty).
  const items = separated([
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
    null,
    { label: t("cloudAfk.actions.console"), icon: <Terminal size={16} />, onSelect: onConsole },
    ...(onInventory
      ? [
          {
            label: t("cloudAfkFeat.tabs.inventory"),
            icon: <Backpack size={16} />,
            onSelect: onInventory,
          },
        ]
      : []),
    ...(onMovement
      ? [{ label: t("cloudAfkMove.tab"), icon: <SneakerMove size={16} />, onSelect: onMovement }]
      : []),
    null,
    { label: t("cloudAfk.actions.settings"), icon: <GearSix size={16} />, onSelect: onSettings },
    ...(onCopy
      ? [{ label: t("cloudAfkFeat.copy.menu"), icon: <Copy size={16} />, onSelect: onCopy }]
      : []),
    ...(onPlus
      ? [{ label: t("afkPlus.card.open"), icon: <ChatsCircle size={16} />, onSelect: onPlus }]
      : []),
    null,
    ...(onUpgrade
      ? [
          {
            label: t("afkPlus.card.upgrade"),
            icon: <ArrowSquareOut size={16} />,
            onSelect: onUpgrade,
          },
        ]
      : []),
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
  ]);

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
        {/* While choosing, a press on the card (not on its buttons) toggles it. */}
        {/* biome-ignore lint/a11y/noStaticElementInteractions: the box is the control; the card only widens its target */}
        {/* biome-ignore lint/a11y/useKeyWithClickEvents: the box takes the keyboard */}
        <div
          onClick={
            selecting && onSelect
              ? (event) => {
                  const target = event.target as HTMLElement;
                  if (target.closest("button, a, input, [role=checkbox]")) return;
                  onSelect(!selected);
                }
              : undefined
          }
          className={cn(
            "glass relative flex w-full flex-col rounded-[var(--radius-panel)] p-3.5",
            inGame && "shadow-[inset_0_0_0_1px_rgb(95_203_142/0.22)]",
            selecting && "cursor-pointer",
          )}
        >
          <AnimatePresence initial={false}>
            {selecting && selected ? (
              <motion.span
                key="chosen"
                className="pointer-events-none absolute inset-0"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.16) }}
                exit={{ opacity: 0, transition: leave(0.12) }}
              >
                <PixelBrackets />
              </motion.span>
            ) : null}
          </AnimatePresence>
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
            {selecting && onSelect ? (
              <span className="flex size-8 shrink-0 items-center justify-center">
                <Check
                  checked={selected}
                  onCheckedChange={onSelect}
                  label={t("cloudAfkFeat.bulk.pick", { name: account.name })}
                />
              </span>
            ) : (
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
            )}
          </div>

          <div className="mt-3 flex min-h-5 items-center gap-3">
            <CloudStateLine account={account} skewMs={skewMs} className="flex-1" showHint />
          </div>
          <TimeRow account={account} skewMs={skewMs} />
          {onPlus ? <PlusLine account={account} onOpen={onPlus} /> : null}

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
