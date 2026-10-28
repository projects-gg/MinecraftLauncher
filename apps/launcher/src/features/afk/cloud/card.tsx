// A cloud account on the AFK page: face, name and server, the state line, the paid time with
// "Süreyi uzat" once renewal opens, and one main button (Başlat, Durdur, "Şifreyi düzelt" after the
// server turned its password down, or "Yeniden etkinleştir" on the site once the time is up), the
// console and the settings. With the cloud features: "Envanter" and "Hareket" in its menu (the
// panel's tabs), "Ayarlarını kopyala", and a box while several accounts are chosen (the chosen card
// carries the brand's pixel brackets).
//
// One card per account (AFK-CLEAR): when this computer has the same account (its twin), this card is
// its only one. It says where the account runs ("Bulutta oyunda", "Bu bilgisayarda oyunda"), its
// main button, console and settings are those of that place (the cloud's while it runs nowhere),
// and one line moves it: "Bu bilgisayarda çalıştır" / "Bulutta çalıştır" stops the other place.
import {
  ArrowSquareOut,
  ArrowsClockwise,
  Backpack,
  CaretRight,
  ChatsCircle,
  Cloud,
  Copy,
  Desktop,
  DotsThreeVertical,
  GearSix,
  Key,
  Play,
  SneakerMove,
  Stop,
  StopCircle,
  Terminal,
  Trash,
  X,
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
  Tooltip,
  useMotionPrefs,
  usePressGuard,
} from "@pl/ui";
import { AnimatePresence, motion, type Transition } from "motion/react";
import { type MouseEvent, type Ref, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check } from "@/features/settings/cleanup/check";
import { type AfkAccount, api } from "@/lib/api";
import { AfkHead } from "../afk-head";
import { ClientRow, LiveStats } from "../card-parts";
import { afkFailed, runAfk } from "../errors";
import { AfkPrimaryAction, primaryAction } from "../primary-action";
import { SettingsButton } from "../settings/settings-button";
import { useAfkReconnect } from "../settings/settings-sheet";
import { StateLine } from "../state-line";
import { type CloudAccount, cloudApi } from "./api";
import { appealFirst, isModerationStop } from "./moderation";
import { ModerationLine, openAppeal } from "./moderation-bits";
import { AI_TONE, useAiSentence } from "./plus/rail";
import { plusAi } from "./plus/types";
import { firstClause } from "./plus/words";
import { useAccountReach, useMachineAway } from "./reach";
import { CloudStateLine } from "./state-line";
import {
  type CloudPrimary,
  cloudPrimary,
  endsSoon,
  isInGame,
  serverLabel,
  timeLeft,
} from "./status";
import { type TwinPlace, twinPlace } from "./twins";
import { useGuardedConnect, useRunHere, useRunInCloud } from "./use-cloud";

const glide: Transition = { type: "spring", stiffness: 420, damping: 36, mass: 0.9 };
const REST = "translateY(0px) scale(1)";
const ORDER: CloudPrimary[] = ["start", "stop", "renew", "fixPassword"];

/**
 * The paid time, brighter in its last week, with "Süreyi uzat" beside it while renewal is open,
 * which is at any time unless Studio keeps it to the last days (the site renews). Gold is the
 * link's: a gold time beside a gold link read as one more link (SHEET-UX).
 */
function TimeRow({
  account,
  skewMs,
  here = false,
}: {
  account: CloudAccount;
  skewMs: number;
  /** The card stands under "Bu bilgisayarda": the time is the cloud's, and it says so. */
  here?: boolean;
}) {
  const { t } = useTranslation();
  const now = Date.now() + skewMs;
  const soon = endsSoon(account, now);
  const left = timeLeft(account, now, t);
  const renew = () =>
    cloudApi
      .open("renew", account.id)
      .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));
  return (
    <p className="mt-1.5 flex min-h-5 flex-wrap items-center gap-x-3 gap-y-1 pl-[23px] text-[12px]">
      <span className={cn("tabular-nums", soon ? "font-semibold text-bone-50" : "text-ash-300")}>
        {here && !account.expired ? t("afkClear.twin.timeLeft", { time: left }) : left}
      </span>
      {account.renewal.open && !account.expired ? (
        // Quiet until the last week: gold on every card beside the main button and OYNA said
        // "this needs you" where nothing did (AFK-CLEAR look 2).
        <button
          type="button"
          onClick={() => void renew()}
          className={cn(
            "inline-flex cursor-pointer items-center gap-1 rounded-sm font-medium underline-offset-2 transition-colors duration-150 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300",
            soon ? "text-gold-300" : "text-ash-300 hover:text-bone-100",
          )}
        >
          {t("cloudAfk.actions.renew")}
          <ArrowSquareOut size={12} weight="bold" aria-hidden />
        </button>
      ) : null}
    </p>
  );
}

/** The move of an account in both places to the other one: its words and its run, or null. */
function useTwinMove(account: CloudAccount, here: AfkAccount | null, place: TwinPlace) {
  const { t } = useTranslation();
  const runHere = useRunHere();
  const runInCloud = useRunInCloud();
  const [busy, setBusy] = useState(false);
  if (!here) return null;
  // The cloud one takes over only while nothing holds it (a stop to fix, the time up).
  const toCloud = place === "here" && !account.hold && !account.expired;
  const toHere = place === "cloud" && here.state.kind !== "stopping";
  if (!toHere && !toCloud) return null;
  const run = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await (toHere ? runHere(account, here) : runInCloud(account, here));
    } finally {
      setBusy(false);
    }
  };
  return {
    toHere,
    busy,
    run,
    label: toHere ? t("afkClear.twin.runHere") : t("afkClear.twin.runInCloud"),
  };
}

/**
 * The line of an account this computer has too: its move to the other place in one plain phrase,
 * the place's mark before it ("Bu bilgisayarda çalıştır" stops the cloud one first; "Bulutta
 * çalıştır" the one here). A quiet text button, not gold: the card's gold is its main button.
 * `waits`: the cloud's machine is out of reach, so neither place can take over (CLOUD-STALE).
 */
function PlaceRow({
  move,
  waits,
}: {
  move: NonNullable<ReturnType<typeof useTwinMove>>;
  waits: boolean;
}) {
  const { t } = useTranslation();
  const Mark = move.toHere ? Desktop : Cloud;
  // A button's shape: as words with a place mark under the state lines it read as one more of
  // them, where the account runs now (AFK-CLEAR look 2).
  return (
    <div className="mt-2 flex pl-[23px]">
      {/* A disabled button takes no pointer: the reason hangs on its wrapper (afk-page.tsx). */}
      <Tooltip content={t("cloudStale.waits")} disabled={!waits}>
        <span className="inline-flex">
          <Button
            size="sm"
            variant="secondary"
            icon={<Mark size={14} weight="bold" aria-hidden className="text-ash-300" />}
            disabled={move.busy || waits}
            onClick={() => void move.run()}
          >
            {move.label}
          </Button>
        </span>
      </Tooltip>
    </div>
  );
}

function PrimaryButton({
  account,
  away,
  onStart,
  onStop,
  onFixPassword,
  className,
}: {
  account: CloudAccount;
  /** Its machine is out of reach (CLOUD-STALE). */
  away: boolean;
  onStart: () => void;
  onStop: () => void;
  onFixPassword: () => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const action = cloudPrimary(account);
  // The word that takes the place of the one just pressed never takes the rest of that double
  // click (@pl/ui press-guard), as on the desktop cards.
  const guard = usePressGuard(action);
  if (action === "none") {
    // A moderation stop that may be appealed: the appeal is the one thing to do (README "Appeals"),
    // calm, not gold.
    if (!appealFirst(account)) return null;
    return (
      <Button
        size="sm"
        variant="secondary"
        icon={<ArrowSquareOut size={14} weight="bold" />}
        onClick={() => openAppeal(account.id, t)}
        className={className}
      >
        {t("afkMod.card.appeal")}
      </Button>
    );
  }
  // Stopped until something is fixed (the state line's hint says what, "… düzelt, sonra başlat"):
  // "Başlat" steps back from gold, which called for a start that would stop again (SMALL11).
  // After a plain disconnect ("other") starting again is the fix, and it stays gold. A refused
  // password has its own button, "Şifreyi düzelt" (AFK-CLEAR).
  const hold = account.hold;
  const fixFirst =
    action === "start" &&
    ((hold?.kind === "cause" && hold.cause !== "other") || hold?.kind === "server_blocked");
  const labels: Record<Exclude<CloudPrimary, "none">, string> = {
    start: t("cloudAfk.actions.start"),
    stop: t("cloudAfk.actions.stop"),
    renew: t("cloudAfk.actions.reactivate"),
    fixPassword: t("afkClear.fixPassword"),
  };
  const icons = {
    start: <Play size={14} weight="fill" />,
    stop: <Stop size={14} weight="fill" />,
    renew: <ArrowSquareOut size={14} weight="bold" />,
    fixPassword: <Key size={14} weight="bold" />,
  };
  const press = (event: MouseEvent) => {
    if (!guard.take(event)) return;
    if (action === "start") onStart();
    else if (action === "stop") onStop();
    else if (action === "fixPassword") onFixPassword();
    else
      void cloudApi
        .open("renew", account.id)
        .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));
  };
  // Its machine out of reach: a start, a stop or a password fix could not take effect there (its
  // client keeps running offline), so each waits until the machine is back, as the line over the
  // list says (CLOUD-STALE). Renewing and an appeal happen on the site and never wait.
  const waits = away && (action === "start" || action === "stop" || action === "fixPassword");
  return (
    // A disabled button takes no pointer: the reason hangs on its wrapper (afk-page.tsx).
    <Tooltip content={t("cloudStale.waits")} disabled={!waits}>
      <span className={cn("inline-flex", className)}>
        <Button
          size="sm"
          variant={action === "stop" || fixFirst ? "secondary" : "primary"}
          icon={icons[action]}
          disabled={waits}
          aria-disabled={guard.armed ? undefined : true}
          onClick={press}
          {...guard.handlers}
          className="w-full"
        >
          <SwapLabel
            labels={ORDER.map((a) => labels[a as Exclude<CloudPrimary, "none">])}
            active={ORDER.indexOf(action)}
          />
        </Button>
      </span>
    </Tooltip>
  );
}

/**
 * An AFK+ account's line on its card: the assistant's state in one sentence and what waits for the
 * player (the approvals in bone, their weight sets them apart: gold is never a sentence); the whole
 * line opens the AFK+ panel.
 */
function PlusLine({ account, onOpen }: { account: CloudAccount; onOpen: () => void }) {
  const { t } = useTranslation();
  const sentence = useAiSentence(account, true);
  // The row has room for the state only: its first clause ("Kontörün bitti"); the rest ("13:07'de
  // yenilenir") is in the panel's rail and the row's tooltip, never cut mid-word.
  const brief = firstClause(sentence.text);
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
        <span className="shrink-0 text-[12.5px] font-semibold text-bone-50 tabular-nums">
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

/** What the copy here offers while the card speaks for this computer (its menu's first group). */
function useHereItems(here: AfkAccount | null, onFixPassword: () => void): MenuItem[] {
  const { t } = useTranslation();
  const connect = useGuardedConnect();
  const reconnect = useAfkReconnect();
  if (!here) return [];
  const stop = () => runAfk(api.afkStop(here.id), t("afk.stopFailed"));
  const items: Record<ReturnType<typeof primaryAction>, MenuItem> = {
    connect: { label: t("afk.connect"), icon: <Play size={16} />, onSelect: () => connect(here) },
    stop: { label: t("afk.stop"), icon: <StopCircle size={16} />, onSelect: stop },
    cancel: { label: t("afkConnect.cancel"), icon: <X size={16} />, onSelect: stop },
    retry: {
      label: t("common.retry"),
      icon: <ArrowsClockwise size={16} />,
      onSelect: () => connect(here),
    },
    fixPassword: {
      label: t("afkClear.fixPassword"),
      icon: <Key size={16} />,
      onSelect: onFixPassword,
    },
  };
  return [
    items[primaryAction(here.state)],
    ...(here.state.kind === "connected"
      ? [
          {
            label: t("afk.reconnect"),
            icon: <ArrowsClockwise size={16} />,
            onSelect: () => reconnect.mutate(here.id),
          },
        ]
      : []),
  ];
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
  here = null,
  onHereConsole,
  onHereSettings,
  onHereRemove,
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
  /** The same account on this computer (one card for both; AFK-CLEAR). */
  here?: AfkAccount | null;
  /** The copy here: its console, its settings, its removal (the cloud account stays). */
  onHereConsole?: () => void;
  onHereSettings?: () => void;
  onHereRemove?: () => void;
  /** Several accounts are being chosen: a box instead of the menu, the card toggles it. */
  selecting?: boolean;
  selected?: boolean;
  onSelect?: (selected: boolean) => void;
  ref?: Ref<HTMLLIElement>;
}) {
  const { t } = useTranslation();
  const { reduce, layout } = useMotionPrefs();
  const place: TwinPlace = here ? twinPlace(account, here) : "cloud";
  // The card speaks for the copy on this computer (its state, button, console and settings).
  const local = place === "here" ? here : null;
  // Its machine out of reach, the card makes no live claim (the line says what was known last) and
  // what needs the machine waits.
  const stale = useAccountReach(account).stale;
  const away = useMachineAway(account);
  const inGame = local ? local.state.kind === "connected" : isInGame(account) && !stale;
  const action = cloudPrimary(account);
  const hereConsole = onHereConsole ?? onConsole;
  const hereSettings = onHereSettings ?? onSettings;
  const hereItems = useHereItems(local, hereSettings);
  const move = useTwinMove(account, here, place);
  const openSite = () =>
    void cloudApi
      .open("account", account.id)
      .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));
  const settingsLabel = here ? t("afkClear.twin.cloudSettings") : t("cloudAfk.actions.settings");

  // Groups: the account itself, where it runs, its panel's tabs, its settings, then the site, and
  // last the copy here's removal. A separator stands only between groups (never first, last or
  // twice: a group may be empty).
  const items = separated([
    ...(local
      ? hereItems
      : [
          // Its machine out of reach, what needs the machine waits, as on the card (CLOUD-STALE).
          action === "start"
            ? {
                label: t("cloudAfk.actions.start"),
                icon: <Play size={16} />,
                onSelect: onStart,
                disabled: away,
              }
            : action === "stop"
              ? {
                  label: t("cloudAfk.actions.stop"),
                  icon: <StopCircle size={16} />,
                  onSelect: onStop,
                  disabled: away,
                }
              : action === "fixPassword"
                ? {
                    label: t("afkClear.fixPassword"),
                    icon: <Key size={16} />,
                    onSelect: onSettings,
                    disabled: away,
                  }
                : null,
          // Fixed elsewhere (the site, another computer): starting stays one press away.
          ...(action === "fixPassword"
            ? [
                {
                  label: t("cloudAfk.actions.start"),
                  icon: <Play size={16} />,
                  onSelect: onStart,
                  disabled: away,
                },
              ]
            : []),
          ...(isInGame(account)
            ? [
                {
                  label: t("cloudAfk.actions.reconnect"),
                  icon: <ArrowsClockwise size={16} />,
                  onSelect: onRestart,
                  disabled: away,
                },
              ]
            : []),
        ]),
    ...(move
      ? [
          {
            label: move.label,
            icon: move.toHere ? <Desktop size={16} /> : <Cloud size={16} />,
            onSelect: () => void move.run(),
            disabled: away,
          },
        ]
      : []),
    null,
    {
      label: t("cloudAfk.actions.console"),
      icon: <Terminal size={16} />,
      onSelect: local ? hereConsole : onConsole,
    },
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
    { label: settingsLabel, icon: <GearSix size={16} />, onSelect: onSettings },
    ...(here
      ? [
          {
            label: t("afkClear.twin.hereSettings"),
            icon: <Desktop size={16} />,
            onSelect: hereSettings,
          },
        ]
      : []),
    ...(onCopy
      ? [
          {
            // A twin has two sets: the copy takes the cloud's, and says so.
            label: here ? t("afkClear.twin.copyCloud") : t("cloudAfkFeat.copy.menu"),
            icon: <Copy size={16} />,
            onSelect: onCopy,
          },
        ]
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
    null,
    ...(here && onHereRemove
      ? [
          {
            label: t("afkClear.twin.removeHere"),
            icon: <Trash size={16} />,
            danger: true,
            onSelect: onHereRemove,
          },
        ]
      : []),
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
      <ContextMenu items={items} title={account.name}>
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
                {/* A moderation stop says its category on the state line, not who stopped it. */}
                {account.hold?.kind === "suspended" && !isModerationStop(account) ? (
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
                // Turned upwards it sat under the "⋯" of the card above (FEAT-L); opened beside it,
                // it can sit over the next card, so its top says whose it is (SHEET-UX).
                besideWhenNoRoom
                title={account.name}
              />
            )}
          </div>

          <div className="mt-3 flex min-h-5 items-center gap-3">
            {local ? (
              <>
                <StateLine state={local.state} className="flex-1" />
                <LiveStats account={local} />
              </>
            ) : (
              <CloudStateLine
                account={account}
                skewMs={skewMs}
                className="flex-1"
                // "Şifreyi düzelt" says what the hint would ("Ayarlar'da giriş şifresini düzelt").
                showHint={action !== "fixPassword"}
              />
            )}
          </div>
          {local ? <ClientRow account={local} /> : null}
          <TimeRow account={account} skewMs={skewMs} here={local !== null} />
          <ModerationLine account={account} />
          {move ? <PlaceRow move={move} waits={away} /> : null}
          {onPlus ? <PlusLine account={account} onOpen={onPlus} /> : null}

          {/* Pinned to the bottom: the buttons of a row of cards line up. */}
          <div className="mt-auto flex gap-2 pt-3">
            {local ? (
              <AfkPrimaryAction account={local} onFixPassword={hereSettings} className="flex-1" />
            ) : (
              <PrimaryButton
                account={account}
                away={away}
                onStart={onStart}
                onStop={onStop}
                onFixPassword={onSettings}
                className="flex-1"
              />
            )}
            <Button
              size="sm"
              variant="ghost"
              icon={<Terminal size={15} />}
              onClick={local ? hereConsole : onConsole}
            >
              {t("cloudAfk.actions.console")}
            </Button>
            {local ? (
              <SettingsButton
                kind="desktop"
                id={local.id}
                label={t("afkClear.twin.hereSettings")}
                onClick={hereSettings}
              />
            ) : (
              <SettingsButton
                kind="cloud"
                id={account.id}
                label={settingsLabel}
                onClick={onSettings}
              />
            )}
          </div>
        </div>
      </ContextMenu>
    </motion.li>
  );
}
