// The AFK page: "Bu bilgisayarda", then 7/24 Bulut. Every game account is one card (AFK-CLEAR): an
// account also in 7/24 Bulut (the same name on the same server) is its cloud card, listed where it
// runs now (under "Bu bilgisayarda" while its copy here is busy, else in 7/24 Bulut). Each list has
// its own count and buttons, so "Tümünü durdur" and "Tümünü bağla" stand over what they cover.
import {
  ArrowsClockwise,
  DotsThreeVertical,
  FolderOpen,
  GearSix,
  Key,
  Play,
  Stop,
  StopCircle,
  Terminal,
  Trash,
  UserPlus,
  X,
} from "@phosphor-icons/react";
import {
  Button,
  ContentIcon,
  ContextMenu,
  cn,
  Dialog,
  EmptyState,
  easeOutExpo,
  Field,
  IconButton,
  Input,
  Menu,
  type MenuItem,
  Tooltip,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion, type Transition } from "motion/react";
import { type Ref, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { isAfkIdle } from "@/features/accounts/afk-match";
import { type AfkAccount, api } from "@/lib/api";
import { AfkConsole } from "./afk-console";
import { AfkHead } from "./afk-head";
import { ClientRow, LiveStats } from "./card-parts";
import { CloudCard } from "./cloud/card";
import { useCloudPlaces, useMoveMenuItem } from "./cloud/location";
import { CloudSection, useCloudCardActions } from "./cloud/section";
import { foldedIds, twinsHere } from "./cloud/twins";
import { useCloudPage, useConnectAll, useGuardedConnect } from "./cloud/use-cloud";
import { afkFailed, isFriendly, runAfk } from "./errors";
import { AfkPrimaryAction, primaryAction } from "./primary-action";
import { draftKey, useAfkDrafts } from "./settings/drafts";
import { SettingsButton } from "./settings/settings-button";
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

  // Asks first when the same account runs in 7/24 Bulut (cloud/use-cloud.ts).
  const guardedConnect = useGuardedConnect();
  const connect = () => guardedConnect(account);
  const moveItem = useMoveMenuItem(account);
  const stop = () => runAfk(api.afkStop(account.id), t("afk.stopFailed"));
  const openFolder = () => runAfk(api.afkOpenFolder(account.id), t("afk.openFolderFailed"));

  // The main button's action leads the menu too.
  const main: Record<ReturnType<typeof primaryAction>, MenuItem> = {
    connect: { label: t("afk.connect"), icon: <Play size={16} />, onSelect: connect },
    stop: { label: t("afk.stop"), icon: <Stop size={16} />, onSelect: stop },
    cancel: { label: t("afkConnect.cancel"), icon: <X size={16} />, onSelect: stop },
    retry: { label: t("common.retry"), icon: <ArrowsClockwise size={16} />, onSelect: connect },
    fixPassword: {
      label: t("afkClear.fixPassword"),
      icon: <Key size={16} />,
      onSelect: onSettings,
    },
  };

  // One list for the "more" button and the right click.
  const items: Array<MenuItem | null> = [
    main[primaryAction(account.state)],
    // Fixed in another way (on the server's site): connecting stays one press away.
    ...(primaryAction(account.state) === "fixPassword"
      ? [{ label: t("afk.connect"), icon: <Play size={16} />, onSelect: connect }]
      : []),
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
    ...(moveItem ? [moveItem] : []),
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
      <ContextMenu items={items} title={account.nickname}>
        <div
          className={cn(
            "glass relative flex w-full flex-col rounded-[var(--radius-panel)] p-3.5",
            connected && "shadow-[inset_0_0_0_1px_rgb(95_203_142/0.22)]",
          )}
        >
          <div className="flex items-start gap-3">
            <AfkHead nickname={account.nickname} size={40} />
            <div className="min-w-0 flex-1">
              {/* The name alone: "connects when the launcher opens" is in the state line, where it
                  matters (SMALL10b; a pill beside the name read as a badge). */}
              <h3 className="truncate font-display text-[15px] font-semibold text-bone-50">
                {account.nickname}
              </h3>
              {/* The address alone: the game version is a setting, not a state, and a pill
                  beside it read as a badge with nothing to act on (SHEET-UX). */}
              <p
                className="truncate text-[12.5px] text-ash-300"
                title={account.server}
                data-selectable
              >
                {account.server}
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
              // Turned upwards it would sit under the "⋯" of the card above; opened beside it, the
              // name at its top says whose it is.
              besideWhenNoRoom
              title={account.nickname}
            />
          </div>

          <div className="mt-3 flex min-h-5 items-center gap-3">
            <StateLine state={account.state} autoStart={account.autoStart} className="flex-1" />
            <LiveStats account={account} />
          </div>
          <ClientRow account={account} />

          {stale ? (
            // A quiet note, the gold only on its mark (projects-taste: no coloured boxes).
            <div className="mt-3 flex items-center gap-2 rounded-md bg-white/[0.035] py-1.5 pr-1.5 pl-2.5 ring-1 ring-white/7">
              <ArrowsClockwise size={14} aria-hidden className="shrink-0 text-gold-300" />
              <p className="min-w-0 flex-1 text-[12px] leading-snug text-bone-100">
                {t("afk.settingsChanged")}
              </p>
              <Button
                size="sm"
                variant="ghost"
                disabled={reconnect.isPending}
                onClick={() => reconnect.mutate(account.id)}
              >
                {t("afk.reconnect")}
              </Button>
            </div>
          ) : null}

          {/* Pinned to the bottom: the buttons of a row of cards line up. */}
          <div className="mt-auto flex gap-2 pt-3">
            <AfkPrimaryAction account={account} onFixPassword={onSettings} className="flex-1" />
            <Button size="sm" variant="ghost" icon={<Terminal size={15} />} onClick={onConsole}>
              {t("afk.console")}
            </Button>
            <SettingsButton
              kind="desktop"
              id={account.id}
              label={t("afk.settings")}
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

/** The server turned it away for good (a ban, a password it refused): it waits for a fix. */
const refused = (account: AfkAccount) =>
  account.state.kind === "error" && Boolean(account.state.cause);

/**
 * "Buradakileri durdur" and "Buradakileri bağla" of the "Bu bilgisayarda" list: what it shows (the
 * words say so: "Tümünü …" under the page title read as every account, the cloud's too). Stopping
 * takes every client of the list (the copy here of an account also in 7/24 Bulut while it runs
 * here included); connecting takes its idle accounts only this computer has, never one the server
 * turned away (a refused password tried again can lock the account on the server; pl-afk leaves
 * those out too), and its tooltip says so while the list has one.
 */
function DesktopBulk({
  running,
  idle,
  others,
  skipped,
}: {
  /** The list's clients that are busy now. */
  running: AfkAccount[];
  /** The list's accounts "Buradakileri bağla" would connect. */
  idle: AfkAccount[];
  /** Some desktop accounts are 7/24 Bulut cards: stopping then goes one by one. */
  others: boolean;
  /** The list's accounts the server stopped for good, which connecting leaves out. */
  skipped: number;
}) {
  const { t } = useTranslation();
  const connectAll = useConnectAll();
  const stopAll = () => {
    if (!others) return runAfk(api.afkStopAll(), t("afk.stopFailed"));
    // One by one: each also leaves a running "Buradakileri bağla" (pl-afk `disconnect`).
    for (const account of running) runAfk(api.afkStop(account.id), t("afk.stopFailed"));
  };
  return (
    <div className="flex gap-2">
      {running.length ? (
        // On a ghost button a filled square reads as a checkbox: the outlined stop sign.
        <Button variant="ghost" icon={<StopCircle size={16} />} onClick={stopAll}>
          {t("afkClear.bulk.stopHere")}
        </Button>
      ) : null}
      {/* A disabled button takes no pointer: the reason hangs on its wrapper (event-join.tsx). */}
      <Tooltip content={t("afkClear.bulk.skipped")} disabled={skipped === 0}>
        <span className="inline-flex">
          <Button
            variant="secondary"
            icon={<Play size={15} weight="fill" className="text-gold-300" />}
            onClick={connectAll}
            disabled={idle.length === 0}
          >
            {t("afkClear.bulk.connectHere")}
          </Button>
        </span>
      </Tooltip>
    </div>
  );
}

export function AfkPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: accounts = [], isPending: listPending } = useQuery({
    queryKey: ["afk"],
    queryFn: () => api.afkAccounts(),
  });
  const [consoleId, setConsoleId] = useState<string | null>(null);
  const [settingsId, setSettingsId] = useState<string | null>(null);
  const [removing, setRemoving] = useState<AfkAccount | null>(null);
  const { data: cloud, isPending: cloudPending, skewMs } = useCloudPage();
  const cloudActions = useCloudCardActions(accounts);
  // The lists wait a moment for 7/24 Bulut's first answer, so an account in both places never shows
  // a card here and then moves to the cloud list: at most 600 ms, then the page shows what it has.
  const [waited, setWaited] = useState(false);
  useEffect(() => {
    if (!cloudPending) return;
    const id = window.setTimeout(() => setWaited(true), 600);
    return () => window.clearTimeout(id);
  }, [cloudPending]);
  const isPending = listPending || (cloudPending && !waited);
  // The player has 7/24 Bulut accounts: the cloud list stands under this one.
  const places = useCloudPlaces();
  // An account in both places is its cloud card: here while its copy here is busy.
  const folded = foldedIds(cloud);
  const local = accounts.filter((a) => !folded.has(a.id));
  const visiting = twinsHere(cloud, accounts);
  const listed = local.length + visiting.length;

  useEffect(
    () => api.onAfkChange(() => void queryClient.invalidateQueries({ queryKey: ["afk"] })),
    [queryClient],
  );

  const online =
    local.filter((a) => a.state.kind === "connected").length +
    visiting.filter((twin) => twin.here.state.kind === "connected").length;
  const consoleAccount = accounts.find((a) => a.id === consoleId);
  const settingsAccount = accounts.find((a) => a.id === settingsId);
  const count =
    listed === 0
      ? // An account in both places idles here: its card is in 7/24 Bulut, so not "Hesap yok".
        folded.size > 0
        ? t("afkClear.here.noneRunning")
        : t("afkClear.here.none")
      : online > 0
        ? t("afk.countOnline", { count: listed, online })
        : t("afk.count", { count: listed });
  // No account anywhere: the empty state says what the page is for (no heading over nothing).
  const empty = accounts.length === 0 && !places;

  // Adding or removing an account sends no session event, so the list is asked again here.
  const refresh = () => void queryClient.invalidateQueries({ queryKey: ["afk"] });

  const remove = (account: AfkAccount) => {
    if (settingsId === account.id) setSettingsId(null);
    if (consoleId === account.id) setConsoleId(null);
    // Its unsaved settings go with it.
    useAfkDrafts.getState().hold(draftKey("desktop", account.id), null);
    api
      .afkRemove(account.id)
      .catch((error: unknown) => afkFailed(error, t("afk.removeFailed")))
      .finally(refresh);
  };
  const removingTwin = removing !== null && folded.has(removing.id);
  const running = [
    ...local.filter(isActive),
    ...visiting.map((twin) => twin.here).filter(isActive),
  ];
  const idle = local.filter((a) => !isActive(a) && !refused(a));

  return (
    <div className="flex min-h-0 flex-1 flex-col px-page pt-3">
      <header className="pb-4">
        <h1 className="page-title">{t("afk.title")}</h1>
      </header>

      {/* Positioned: a removed card pops out of the flow here, so it fades where it was even
          when the list is scrolled, clipped with the list. */}
      <div className="page-scroll relative min-h-0 flex-1 pb-5">
        {/* This computer's list: its heading says where these accounts run (and stop with it). */}
        {isPending || empty ? null : (
          <header className="mb-3 flex flex-wrap items-end justify-between gap-3">
            <div>
              <h2 className="font-display text-section font-semibold text-bone-50">
                {t("afkClear.here.title")}
              </h2>
              <p className="mt-0.5 text-[13px] text-ash-300 tabular-nums">{count}</p>
            </div>
            {listed > 0 ? (
              <DesktopBulk
                running={running}
                idle={idle}
                others={folded.size > 0}
                skipped={local.filter(refused).length}
              />
            ) : null}
          </header>
        )}
        {/* The list answers within a frame: nothing (rather than a jumping form) until then. */}
        {isPending ? null : empty ? (
          <EmptyState
            icon={<ContentIcon kind="afk" />}
            title={t("afk.empty.title")}
            body={t("afk.empty.body")}
            className="pt-[8vh] pb-5"
          />
        ) : listed > 0 ? (
          <ul className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-3">
            <AnimatePresence initial={false} mode="popLayout">
              {local.map((a, i) => (
                <AfkCard
                  key={a.id}
                  account={a}
                  index={i}
                  onConsole={() => setConsoleId(a.id)}
                  onSettings={() => setSettingsId(a.id)}
                  onRemove={() => setRemoving(a)}
                />
              ))}
              {/* A 7/24 Bulut account running on this computer now: its card, its buttons. */}
              {visiting.map(({ cloud: account, here }, i) => (
                <CloudCard
                  key={account.id}
                  account={account}
                  here={here}
                  skewMs={skewMs}
                  index={local.length + i}
                  {...cloudActions(account)}
                  onHereConsole={() => setConsoleId(here.id)}
                  onHereSettings={() => setSettingsId(here.id)}
                  onHereRemove={() => setRemoving(here)}
                />
              ))}
            </AnimatePresence>
          </ul>
        ) : null}
        {isPending ? null : <AddForm accounts={accounts} centered={empty} onAdded={refresh} />}
        {/* Nothing of it shows unless 7/24 Bulut answers. */}
        {isPending ? null : (
          <CloudSection
            desktop={accounts}
            centered={empty}
            onHereConsole={setConsoleId}
            onHereSettings={setSettingsId}
            onHereRemove={setRemoving}
          />
        )}
      </div>

      <AfkConsole
        account={consoleAccount}
        onClose={() => setConsoleId(null)}
        onFixPassword={(id) => {
          setConsoleId(null);
          setSettingsId(id);
        }}
      />
      <AfkSettingsSheet
        account={settingsAccount}
        // An account in both places: the title says these are this computer's settings.
        twin={settingsAccount ? folded.has(settingsAccount.id) : false}
        onClose={() => setSettingsId(null)}
      />
      <Dialog
        open={Boolean(removing)}
        onOpenChange={(open) => !open && setRemoving(null)}
        // The copy here of an account also in 7/24 Bulut: the cloud account stays.
        title={
          removingTwin
            ? t("afkClear.twin.removeTitle", { name: removing?.nickname ?? "" })
            : t("afk.removeConfirm.title", { name: removing?.nickname ?? "" })
        }
        description={removingTwin ? t("afkClear.twin.removeBody") : t("afk.removeConfirm.body")}
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
              {removingTwin ? t("afkClear.twin.removeHere") : t("afk.removeConfirm.confirm")}
            </Button>
          </>
        }
      />
    </div>
  );
}
