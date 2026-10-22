import {
  ArrowsClockwise,
  DotsThreeVertical,
  FolderOpen,
  ForkKnife,
  GearSix,
  Heart,
  Play,
  Stop,
  StopCircle,
  Terminal,
  Trash,
  UserPlus,
  UsersThree,
  X,
} from "@phosphor-icons/react";
import {
  Button,
  Chip,
  ContextMenu,
  cn,
  Dialog,
  EmptyState,
  easeOutExpo,
  enter,
  Field,
  IconButton,
  Input,
  leave,
  Menu,
  type MenuItem,
  Progress,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion, type Transition } from "motion/react";
import { type Ref, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { isAfkIdle } from "@/features/accounts/afk-match";
import { type AfkAccount, api } from "@/lib/api";
import { ContentIcon } from "@/shell/content-icon";
import { AfkConsole } from "./afk-console";
import { AfkHead } from "./afk-head";
import { afkFailed, isFriendly, runAfk } from "./errors";
import { AfkPrimaryAction, primaryAction } from "./primary-action";
import { AfkSettingsSheet, useAfkReconnect } from "./settings/settings-sheet";
import { StateLine } from "./state-line";

const NAME = /^[A-Za-z0-9_]{3,16}$/;

/** Cards gliding when one before them leaves or grows. */
const glide: Transition = { type: "spring", stiffness: 420, damping: 36, mass: 0.9 };
/** Where a card rests; its entrance and exit move from and to the same function list. */
const REST = "translateY(0px) scale(1)";

/**
 * Anything but idle: a stop makes sense (queued, reconnecting and waiting for the client
 * included).
 */
const isActive = (account: AfkAccount) => !isAfkIdle(account.state);

/**
 * Under the state line while the account's client is fetched (its progress) or after it could not
 * be had (what to do). One slot: the outgoing line fades where it was while the new one takes its
 * place.
 */
function ClientRow({ account }: { account: AfkAccount }) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const { state } = account;
  const from = reduce ? "translateY(0px)" : "translateY(3px)";
  return (
    <AnimatePresence initial={false} mode="popLayout">
      {state.kind === "preparing" ? (
        <motion.div
          key="progress"
          className="mt-2.5"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.22) }}
          exit={{ opacity: 0, transition: leave(0.16) }}
        >
          <Progress
            value={state.percent}
            label={t(
              state.step === "downloading" ? "afkConnect.downloading" : "afkConnect.preparing",
            )}
            className="h-1"
          />
        </motion.div>
      ) : state.kind === "clientFailed" ? (
        <motion.p
          key={`hint-${state.problem}`}
          // Said once when it appears; the state line above already shows what happened.
          role="status"
          className="mt-1.5 pl-[23px] text-[12px] leading-snug text-ash-300"
          initial={{ opacity: 0, transform: from }}
          animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.24) }}
          exit={{ opacity: 0, transition: leave(0.16) }}
        >
          <span className="sr-only">{t(`afkConnect.problem.${state.problem}.title`)}. </span>
          {t(`afkConnect.problem.${state.problem}.hint`)}
        </motion.p>
      ) : null}
    </AnimatePresence>
  );
}

/** Health, food and players of a connected account, beside its state (no extra row). */
function Stats({ account }: { account: AfkAccount }) {
  const { t } = useTranslation();
  const stats = [
    {
      key: "health",
      label: t("afk.stats.health"),
      value: account.health,
      icon: <Heart size={14} weight="fill" className="text-flare-400" />,
    },
    {
      key: "food",
      label: t("afk.stats.food"),
      value: account.food,
      icon: <ForkKnife size={14} weight="fill" className="text-gold-400" />,
    },
    {
      key: "players",
      label: t("afk.stats.players"),
      value: account.players,
      icon: <UsersThree size={14} weight="fill" className="text-ash-300" />,
    },
  ];
  return (
    <span className="flex shrink-0 items-center gap-3 text-[12.5px] text-ash-300 tabular-nums">
      {stats.map((stat) => (
        <span key={stat.key} className="flex items-center gap-1" title={stat.label}>
          <span aria-hidden className="flex">
            {stat.icon}
          </span>
          <span className="sr-only">{stat.label}</span>
          {stat.value ?? "-"}
        </span>
      ))}
    </span>
  );
}

function AfkCard({
  account,
  onConsole,
  onSettings,
  onRemove,
  index,
  ref,
}: {
  account: AfkAccount;
  onConsole: () => void;
  onSettings: () => void;
  onRemove: () => void;
  index: number;
  /** Set by the list's AnimatePresence (popLayout measures the leaving card). */
  ref?: Ref<HTMLLIElement>;
}) {
  const { t } = useTranslation();
  const { reduce, layout } = useMotionPrefs();
  const reconnect = useAfkReconnect();
  const connected = account.state.kind === "connected";
  const stale =
    account.settingsChanged && ["starting", "connecting", "connected"].includes(account.state.kind);

  const connect = () => runAfk(api.afkConnect(account.id), t("afk.connectFailed"));
  const stop = () => runAfk(api.afkStop(account.id), t("afk.stopFailed"));
  const openFolder = () => runAfk(api.afkOpenFolder(account.id), t("afk.openFolderFailed"));

  // The main button's action leads the menu too.
  const main: Record<ReturnType<typeof primaryAction>, MenuItem> = {
    connect: { label: t("afk.connect"), icon: <Play size={16} />, onSelect: connect },
    stop: { label: t("afk.stop"), icon: <Stop size={16} />, onSelect: stop },
    cancel: { label: t("afkConnect.cancel"), icon: <X size={16} />, onSelect: stop },
    retry: { label: t("common.retry"), icon: <ArrowsClockwise size={16} />, onSelect: connect },
  };

  // One list for the "more" button and the right click.
  const items: Array<MenuItem | null> = [
    main[primaryAction(account.state)],
    ...(connected
      ? [
          {
            label: t("afk.reconnect"),
            icon: <ArrowsClockwise size={16} />,
            onSelect: () => reconnect.mutate(account.id),
          },
        ]
      : []),
    { label: t("afk.console"), icon: <Terminal size={16} />, onSelect: onConsole },
    { label: t("afk.settings"), icon: <GearSix size={16} />, onSelect: onSettings },
    { label: t("afk.openFolder"), icon: <FolderOpen size={16} />, onSelect: openFolder },
    null,
    { label: t("afk.remove"), icon: <Trash size={16} />, danger: true, onSelect: onRemove },
  ];

  return (
    <motion.li
      ref={ref}
      layout={layout ? "position" : false}
      transition={{ layout: glide }}
      // Transform strings, as on the library's cards; reduced motion keeps the fades only.
      initial={{ opacity: 0, transform: reduce ? REST : "translateY(10px) scale(1)" }}
      animate={{
        opacity: 1,
        transform: REST,
        transition: { duration: 0.35, delay: Math.min(index, 8) * 0.04, ease: easeOutExpo },
      }}
      // Removed: fades (and settles to 97%) in place while the cards after it glide over.
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
            connected && "shadow-[inset_0_0_0_1px_rgb(95_203_142/0.22)]",
          )}
        >
          <div className="flex items-start gap-3">
            <AfkHead nickname={account.nickname} size={40} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <h3 className="truncate font-display text-[15px] font-semibold text-bone-50">
                  {account.nickname}
                </h3>
                {account.autoStart ? <Chip>{t("afk.autoStart")}</Chip> : null}
              </div>
              {/* The address keeps its line: the version chip moves under it when both do not
                  fit (three cards in a row at 1200 px, "Automatic version" in English). */}
              <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[12.5px] text-ash-300">
                <span
                  className="min-w-0 max-w-full truncate"
                  title={account.server}
                  data-selectable
                >
                  {account.server}
                </span>
                {!account.mcVersionRecommended ? (
                  <Chip className="shrink-0">
                    {account.mcVersion === "auto"
                      ? t("afk.versionAuto")
                      : t("afk.versionPinned", { version: account.mcVersion })}
                  </Chip>
                ) : null}
              </p>
            </div>
            <Menu
              trigger={
                <IconButton
                  label={t("afk.more")}
                  icon={<DotsThreeVertical size={18} weight="bold" />}
                  size="sm"
                />
              }
              items={items}
            />
          </div>

          <div className="mt-3 flex min-h-5 items-center gap-3">
            <StateLine state={account.state} className="flex-1" />
            <AnimatePresence initial={false}>
              {connected ? (
                <motion.span
                  key="stats"
                  className="flex"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1, transition: enter(0.22) }}
                  exit={{ opacity: 0, transition: leave(0.16) }}
                >
                  <Stats account={account} />
                </motion.span>
              ) : null}
            </AnimatePresence>
          </div>
          <ClientRow account={account} />

          {stale ? (
            <div className="mt-3 flex items-center gap-2 rounded-md bg-gold-500/10 py-1.5 pr-1.5 pl-2.5 ring-1 ring-gold-400/20">
              <p className="min-w-0 flex-1 text-[12px] leading-snug text-gold-200">
                {t("afk.settingsChanged")}
              </p>
              <Button
                size="sm"
                variant="ghost"
                icon={<ArrowsClockwise size={14} />}
                disabled={reconnect.isPending}
                onClick={() => reconnect.mutate(account.id)}
              >
                {t("afk.reconnect")}
              </Button>
            </div>
          ) : null}

          {/* Pinned to the bottom: the buttons of a row of cards line up. */}
          <div className="mt-auto flex gap-2 pt-3">
            <AfkPrimaryAction account={account} className="flex-1" />
            <Button size="sm" variant="ghost" icon={<Terminal size={15} />} onClick={onConsole}>
              {t("afk.console")}
            </Button>
            <IconButton
              label={t("afk.settings")}
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

/**
 * New account by its player name. Rendered in one place for both states, so typing (and the
 * focus) survives the first account turning the empty page into the list.
 */
function AddForm({
  accounts,
  centered,
  onAdded,
}: {
  accounts: AfkAccount[];
  centered: boolean;
  onAdded: () => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const add = () => {
    const trimmed = name.trim();
    if (!NAME.test(trimmed)) return setError(t("afk.invalidName"));
    if (accounts.some((a) => a.nickname.toLowerCase() === trimmed.toLowerCase()))
      return setError(t("afk.duplicate"));
    setBusy(true);
    api
      .afkAdd(trimmed)
      .then(() => setName(""))
      // Command errors carry a friendly message (name rules, duplicates, disk).
      .catch((e: unknown) => setError(isFriendly(e) ? e.message : t("afk.addFailed")))
      .finally(() => {
        setBusy(false);
        onAdded();
      });
  };

  return (
    <form
      noValidate
      className={cn("w-full max-w-[460px]", centered ? "mx-auto" : "mt-5")}
      onSubmit={(e) => {
        e.preventDefault();
        add();
      }}
    >
      <Field
        label={t("afk.nickname")}
        help={t("afk.nicknameHelp")}
        error={error}
        htmlFor="afk-name"
      >
        <div className="flex gap-2">
          <Input
            id="afk-name"
            value={name}
            maxLength={16}
            invalid={Boolean(error)}
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => {
              setName(e.target.value);
              setError(null);
            }}
          />
          <Button
            type="submit"
            variant={centered ? "primary" : "secondary"}
            icon={<UserPlus size={16} />}
            disabled={busy}
          >
            {t("afk.add")}
          </Button>
        </div>
      </Field>
    </form>
  );
}

export function AfkPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: accounts = [], isPending } = useQuery({
    queryKey: ["afk"],
    queryFn: () => api.afkAccounts(),
  });
  const [consoleId, setConsoleId] = useState<string | null>(null);
  const [settingsId, setSettingsId] = useState<string | null>(null);
  const [removing, setRemoving] = useState<AfkAccount | null>(null);

  useEffect(
    () => api.onAfkChange(() => void queryClient.invalidateQueries({ queryKey: ["afk"] })),
    [queryClient],
  );

  const anyActive = accounts.some(isActive);
  const anyIdle = accounts.some((a) => !isActive(a));
  const online = accounts.filter((a) => a.state.kind === "connected").length;
  const consoleAccount = accounts.find((a) => a.id === consoleId);
  const settingsAccount = accounts.find((a) => a.id === settingsId);

  // Adding or removing an account sends no session event, so the list is asked again here.
  const refresh = () => void queryClient.invalidateQueries({ queryKey: ["afk"] });

  const remove = (account: AfkAccount) => {
    if (settingsId === account.id) setSettingsId(null);
    if (consoleId === account.id) setConsoleId(null);
    api
      .afkRemove(account.id)
      .catch((error: unknown) => afkFailed(error, t("afk.removeFailed")))
      .finally(refresh);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col px-page pt-3">
      <header className="flex flex-wrap items-end justify-between gap-3 pb-4">
        <div>
          <h1 className="page-title">{t("afk.title")}</h1>
          {/* A count like the library's: what the page is for is said by the empty state. */}
          <p className="mt-0.5 text-[13px] text-ash-300 tabular-nums">
            {isPending
              ? t("common.loading")
              : online > 0
                ? t("afk.countOnline", { count: accounts.length, online })
                : t("afk.count", { count: accounts.length })}
          </p>
        </div>
        {accounts.length ? (
          <div className="flex gap-2">
            {anyActive ? (
              // On a ghost button a filled square reads as a checkbox: the outlined stop sign.
              <Button
                variant="ghost"
                icon={<StopCircle size={16} />}
                onClick={() => runAfk(api.afkStopAll(), t("afk.stopFailed"))}
              >
                {t("afk.stopAll")}
              </Button>
            ) : null}
            <Button
              variant="secondary"
              icon={<Play size={15} weight="fill" className="text-gold-300" />}
              onClick={() => runAfk(api.afkConnectAll(), t("afk.connectFailed"))}
              disabled={!anyIdle}
            >
              {t("afk.connectAll")}
            </Button>
          </div>
        ) : null}
      </header>

      {/* Positioned: a removed card pops out of the flow here, so it fades where it was even
          when the list is scrolled, clipped with the list. */}
      <div className="relative -mx-2 min-h-0 flex-1 overflow-y-auto px-2 pb-5">
        {/* The list answers within a frame: nothing (rather than a jumping form) until then. */}
        {isPending ? null : accounts.length === 0 ? (
          <EmptyState
            icon={<ContentIcon kind="afk" />}
            title={t("afk.empty.title")}
            body={t("afk.empty.body")}
            className="pt-[8vh] pb-5"
          />
        ) : (
          <ul className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-3">
            <AnimatePresence initial={false} mode="popLayout">
              {accounts.map((a, i) => (
                <AfkCard
                  key={a.id}
                  account={a}
                  index={i}
                  onConsole={() => setConsoleId(a.id)}
                  onSettings={() => setSettingsId(a.id)}
                  onRemove={() => setRemoving(a)}
                />
              ))}
            </AnimatePresence>
          </ul>
        )}
        {isPending ? null : (
          <AddForm accounts={accounts} centered={accounts.length === 0} onAdded={refresh} />
        )}
      </div>

      <AfkConsole account={consoleAccount} onClose={() => setConsoleId(null)} />
      <AfkSettingsSheet account={settingsAccount} onClose={() => setSettingsId(null)} />
      <Dialog
        open={Boolean(removing)}
        onOpenChange={(open) => !open && setRemoving(null)}
        title={t("afk.removeConfirm.title", { name: removing?.nickname ?? "" })}
        description={t("afk.removeConfirm.body")}
        closeLabel={t("common.close")}
        width={440}
        footer={
          <>
            <Button variant="ghost" onClick={() => setRemoving(null)}>
              {t("afk.removeConfirm.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                if (removing) remove(removing);
                setRemoving(null);
              }}
            >
              {t("afk.removeConfirm.confirm")}
            </Button>
          </>
        }
      />
    </div>
  );
}
