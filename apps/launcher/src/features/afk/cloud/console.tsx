// A cloud account's panel in a side sheet. Its console, as the desktop console: the machine's masked
// lines while the sheet is on screen (the socket closes when it is closed or the window leaves the
// screen), earlier lines on request, and a command line. A password command is sealed by the
// backend before it leaves the launcher; nothing typed is echoed, the server's answer shows.
//
// With the cloud features (CAFK-FEAT) and only while the service offers them: the console's kinds
// (colours, the chips, "Adın geçenler", search, "İndir") and the status strip; the Envanter tab
// beside Konsol, on a solid panel (inside glass Chromium resamples the inventory's pixels).
import { ArrowDown, ArrowUp, Key, PaperPlaneRight, X } from "@phosphor-icons/react";
import {
  Button,
  cn,
  EntranceGate,
  enter,
  GatedMotion,
  IconButton,
  leave,
  scrimMotion,
  sheetMotion,
  sheetOutside,
  TabPanel,
  Tabs,
  useInteractionMotion,
  useMotionPrefs,
} from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { Dialog } from "radix-ui";
import {
  type KeyboardEvent,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import type { CloudBars } from "@/bindings";
import { uiLocale } from "@/lib/format";
import { useWindowAway } from "@/lib/window-away";
import { AfkHead } from "../afk-head";
import { isFriendly } from "../errors";
import { SecretField } from "../secret";
import { type CloudAccount, type CloudLiveEvent, cloudApi } from "./api";
import {
  type KindFilter,
  type KindLine,
  kindTone,
  splitAtName,
  visibleLines,
} from "./console-kinds";
import { ConsoleStrip } from "./console-strip";
import { ConsoleTools } from "./console-tools";
import { InventoryPanel, useInventoryArt } from "./inventory/inventory-panel";
import { MovementPanel } from "./movement/movement-panel";
import { movementShown } from "./movement/use-cloud-movement";
import { useAccountReach } from "./reach";
import { looksSensitive } from "./sensitive";
import { issueKey, textProblem } from "./settings-model";
import { CloudStateLine } from "./state-line";
import { isInGame, stoppedOnPassword } from "./status";
import { type PanelTab, storeAccount, useCloudUi } from "./use-cloud";

type Line = KindLine;
type LinkStatus = "connecting" | "live" | "reconnecting" | "replaced" | "ended" | "refused";

/** The newest lines kept on screen. */
const KEEP = 1000;
/** How close to the end still counts as reading the newest lines (px). */
const STICK_PX = 32;
/** Lines typed per account this session, newest last: ↑ and ↓ walk through them. */
const histories = new Map<string, string[]>();

function Banner({
  status,
  hostOnline,
  noHost,
  onReopen,
}: {
  status: LinkStatus;
  hostOnline: boolean;
  noHost: boolean;
  onReopen: () => void;
}) {
  const { t } = useTranslation();
  const text = noHost
    ? t("cloudAfk.console.noHost")
    : status === "replaced"
      ? t("cloudAfk.console.replaced")
      : status === "ended"
        ? t("cloudAfk.console.ended")
        : status === "refused"
          ? t("cloudAfk.console.refused")
          : status === "reconnecting"
            ? t("cloudAfk.console.linkLost")
            : !hostOnline
              ? t("cloudAfk.console.hostOffline")
              : null;
  return (
    <AnimatePresence initial={false}>
      {text ? (
        // A quiet line in the text colour, the action beside it in gold: no coloured box
        // (projects-taste; AFK-CLEAR's card notes).
        <motion.p
          key={text}
          role="status"
          className="flex items-center gap-2 border-b border-white/8 bg-white/[0.035] px-5 py-2 text-[12.5px] text-bone-100"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.2) }}
          exit={{ opacity: 0, transition: leave(0.14) }}
        >
          <span className="min-w-0 flex-1">{text}</span>
          {status === "replaced" ? (
            <button
              type="button"
              onClick={onReopen}
              className="shrink-0 cursor-pointer rounded-sm font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              {t("cloudAfk.console.reopen")}
            </button>
          ) : null}
        </motion.p>
      ) : null}
    </AnimatePresence>
  );
}

/** One console line: its time, its words in its kind's colour, a private line's name standing out. */
function LineText({
  line,
  clock,
  onInventory,
}: {
  line: Line;
  clock: string;
  onInventory?: () => void;
}) {
  const { t } = useTranslation();
  const parts = splitAtName(line);
  return (
    <p className={cn("break-words", kindTone(line))}>
      <span className="mr-2 text-ash-400 tabular-nums select-none">{clock}</span>
      {parts ? (
        <>
          {parts[0]}
          <span className="text-gold-300">{parts[1]}</span>
          {parts[2]}
        </>
      ) : (
        line.text
      )}
      {line.goto === "inventory" && onInventory ? (
        <button
          type="button"
          onClick={onInventory}
          className="ml-2 cursor-pointer font-sans text-[12px] font-medium text-gold-300 underline-offset-2 outline-none select-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          {t("cloudAfkFeat.console.showInInventory")}
        </button>
      ) : null}
    </p>
  );
}

export function CloudConsole({
  account,
  skewMs,
  onStart,
  onRestart,
  onOpenSettings,
  onClose,
}: {
  account: CloudAccount | undefined;
  skewMs: number;
  onStart: (account: CloudAccount) => void;
  /** "Şimdi yeniden bağla" (the inventory comes at the next connection). */
  onRestart: (account: CloudAccount) => void;
  onOpenSettings: (id: string) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const animate = useInteractionMotion();
  const { reduce } = useMotionPrefs();
  const away = useWindowAway();
  const queryClient = useQueryClient();
  const tab = useCloudUi((s) => s.consoleTab);
  const setTab = useCloudUi((s) => s.setConsoleTab);
  const open = Boolean(account);
  const id = account?.id;
  const host = account?.host ?? null;
  const kinds = Boolean(account?.features?.consoleKinds);
  const [inventoryGone, setInventoryGone] = useState(false);
  const [movementGone, setMovementGone] = useState(false);
  const inventory =
    (account?.features?.inventory?.mode ?? "off") !== "off" && Boolean(host) && !inventoryGone;
  const movement = movementShown(account) && !movementGone;
  const tabbed = inventory || movement;
  const shownTab: PanelTab =
    (tab === "inventory" && inventory) || (tab === "movement" && movement) ? tab : "console";
  const [lines, setLines] = useState<Line[]>([]);
  const [status, setStatus] = useState<LinkStatus>("connecting");
  const [hostOnline, setHostOnline] = useState(true);
  const [skipped, setSkipped] = useState(0);
  const [earlierBusy, setEarlierBusy] = useState(false);
  const [earlierDone, setEarlierDone] = useState(false);
  const [text, setText] = useState("");
  const [sendError, setSendError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [recall, setRecall] = useState<number | null>(null);
  const [unseen, setUnseen] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [filter, setFilter] = useState<KindFilter>("all");
  const [query, setQuery] = useState("");
  const [bars, setBars] = useState<CloudBars | null>(null);
  const draft = useRef("");
  const listRef = useRef<HTMLDivElement>(null);
  const atEnd = useRef(true);
  const accountRef = useRef(account);
  accountRef.current = account;
  const art = useInventoryArt(account);
  const inventoryUnavailable = useCallback(() => setInventoryGone(true), []);
  const movementUnavailable = useCallback(() => setMovementGone(true), []);

  // Another account (or a fresh open) starts empty, at its newest line.
  // biome-ignore lint/correctness/useExhaustiveDependencies: resets when the account changes
  useEffect(() => {
    setLines([]);
    setSkipped(0);
    setEarlierDone(false);
    setText("");
    setSendError(null);
    setRecall(null);
    setUnseen(false);
    setStatus("connecting");
    setHostOnline(true);
    setFilter("all");
    setQuery("");
    setBars(null);
    setInventoryGone(false);
    setMovementGone(false);
    atEnd.current = true;
  }, [id]);

  const onEvent = useCallback(
    (event: CloudLiveEvent) => {
      const current = accountRef.current;
      if (!current) return;
      switch (event.kind) {
        case "status":
          if (event.host === current.host) setStatus(event.status);
          break;
        case "hello":
          // A new connection watches again: the machine sends the strip anew if it holds any.
          if (event.host === current.host) {
            setBars(null);
            setHostOnline(event.online);
          }
          break;
        case "machine":
          if (event.host === current.host) {
            setHostOnline(event.online);
            // What the server kept on screen is not known any more: a return brings it anew.
            if (!event.online) setBars(null);
          }
          break;
        case "state":
          if (event.acct === current.id)
            storeAccount(queryClient, { ...current, live: event.live });
          break;
        case "lines":
          if (event.acct !== current.id) break;
          if (event.skipped) setSkipped((n) => n + event.skipped);
          setLines((old) =>
            event.snapshot ? event.lines.slice(-KEEP) : [...old, ...event.lines].slice(-KEEP),
          );
          break;
        case "bars":
          if (event.acct === current.id) setBars(event.bars);
          break;
        case "problem":
          if (event.code === "host_offline") setHostOnline(false);
          break;
      }
    },
    [queryClient],
  );

  useEffect(() => {
    if (!open) return;
    return cloudApi.onLive(onEvent);
  }, [open, onEvent]);

  // The socket runs only while this panel is on screen.
  useEffect(() => {
    void attempt;
    if (!open || !id || !host || away) return;
    let alive = true;
    // A watch brings the strip again when the server shows one: nothing stale stays.
    setBars(null);
    cloudApi.watch(id).catch(() => {
      if (alive) setStatus("refused");
    });
    return () => {
      alive = false;
      void cloudApi.unwatch(id);
    };
  }, [open, id, host, away, attempt]);

  const shown = kinds ? visibleLines(lines, filter, account?.name ?? "", query) : lines;
  const last = shown.at(-1);
  const lastKey = `${shown.length}|${last?.at ?? ""}|${last?.text ?? ""}|${shownTab}`;

  // New lines are followed while the player reads the end; otherwise a button offers the way down.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs for every new line
  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list || !shown.length) return;
    if (atEnd.current) list.scrollTop = list.scrollHeight;
    else setUnseen(true);
  }, [lastKey]);

  // A new chip or search, or coming back to the console, starts at the newest line.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the view changes
  useLayoutEffect(() => {
    atEnd.current = true;
    setUnseen(false);
    const list = listRef.current;
    if (list) list.scrollTop = list.scrollHeight;
  }, [filter, query, shownTab]);

  const onScroll = () => {
    const list = listRef.current;
    if (!list) return;
    atEnd.current = list.scrollHeight - list.scrollTop - list.clientHeight < STICK_PX;
    if (atEnd.current) setUnseen(false);
  };

  const toEnd = () => {
    const list = listRef.current;
    if (!list) return;
    atEnd.current = true;
    list.scrollTo({ top: list.scrollHeight, behavior: reduce ? "auto" : "smooth" });
    setUnseen(false);
  };

  const loadEarlier = async () => {
    if (!id) return;
    setEarlierBusy(true);
    try {
      const first = lines[0];
      const page = await cloudApi.logs(id, first ? first.at : null);
      const list = listRef.current;
      const before = list?.scrollHeight ?? 0;
      setLines((old) => [...page.lines, ...old].slice(-KEEP * 2));
      if (!page.lines.length || !page.next) setEarlierDone(true);
      // The lines the player was reading stay where they are.
      requestAnimationFrame(() => {
        if (list) list.scrollTop += list.scrollHeight - before;
      });
    } catch (error) {
      setSendError(isFriendly(error) ? error.message : t("cloudAfk.console.sendFailed"));
    } finally {
      setEarlierBusy(false);
    }
  };

  const running = account ? isInGame(account) : false;
  // Its machine out of reach (CLOUD-STALE): the title says what was known last, the strip goes, and
  // the line waits for the connection, since nothing typed could reach the game.
  const stale = useAccountReach(account).stale;
  const canType = running && !stale;
  const sensitive = looksSensitive(text);

  const send = async () => {
    const line = text.trim();
    if (!line || !account || !canType || sending) return;
    // The settings' rule and words (pl_cloud_afk::check_text): said here, nothing is sent.
    const problem = textProblem(line);
    if (problem) {
      const key = issueKey({ place: "console", index: null, field: "text", problem });
      setSendError(t(`cloudAfk.settings.issues.${key}`));
      return;
    }
    setSending(true);
    setSendError(null);
    const list = histories.get(account.id) ?? [];
    if (list.at(-1) !== line) list.push(line);
    if (list.length > 50) list.splice(0, list.length - 50);
    histories.set(account.id, list);
    setRecall(null);
    setText("");
    atEnd.current = true;
    try {
      await cloudApi.send(account.id, line);
    } catch (error) {
      setSendError(isFriendly(error) ? error.message : t("cloudAfk.console.sendFailed"));
      // Nothing typed is lost: the line comes back unless a new one was started.
      setText((current) => current || line);
    } finally {
      setSending(false);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (!account) return;
    const list = histories.get(account.id) ?? [];
    if (event.key === "ArrowUp" && list.length) {
      event.preventDefault();
      if (recall === null) draft.current = text;
      const next = recall === null ? list.length - 1 : Math.max(0, recall - 1);
      setRecall(next);
      setText(list[next] ?? "");
    } else if (event.key === "ArrowDown" && recall !== null) {
      event.preventDefault();
      const next = recall + 1;
      if (next >= list.length) {
        setRecall(null);
        setText(draft.current);
      } else {
        setRecall(next);
        setText(list[next] ?? "");
      }
    }
  };

  const clock = (at: number) =>
    new Date(at * 1000).toLocaleTimeString(uiLocale(), {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });

  const consoleBody = account ? (
    <>
      {kinds ? (
        <>
          <ConsoleTools
            account={account}
            filter={filter}
            onFilter={setFilter}
            query={query}
            onQuery={setQuery}
          />
          <ConsoleStrip bars={canType ? bars : null} skewMs={skewMs} />
        </>
      ) : null}
      <div className="relative min-h-0 flex-1">
        <div
          ref={listRef}
          onScroll={onScroll}
          className="absolute inset-0 overflow-y-auto px-5 py-3 font-mono text-[12.5px] leading-relaxed"
          data-selectable
          role="log"
          aria-live="polite"
          aria-relevant="additions"
        >
          {!earlierDone && lines.length > 0 ? (
            <div className="mb-2 flex justify-center font-sans">
              <Button
                variant="ghost"
                size="sm"
                icon={<ArrowUp size={13} />}
                disabled={earlierBusy}
                onClick={() => void loadEarlier()}
              >
                {t("cloudAfk.console.earlier")}
              </Button>
            </div>
          ) : null}
          {shown.length ? (
            shown.map((line, index) => (
              <LineText
                // biome-ignore lint/suspicious/noArrayIndexKey: lines arrive in order and are never edited
                key={`${line.at}-${index}`}
                line={line}
                clock={clock(line.at)}
                onInventory={inventory ? () => setTab("inventory") : undefined}
              />
            ))
          ) : (
            <p className="flex h-full items-center justify-center font-sans text-[13px] text-ash-400">
              {lines.length
                ? query.trim()
                  ? t("cloudAfkFeat.console.noSearchMatch")
                  : t("cloudAfkFeat.console.noMatch")
                : stale
                  ? t("cloudStale.console.noLines")
                  : status === "connecting" && host
                    ? t("cloudAfk.console.connecting")
                    : t("cloudAfk.console.empty")}
            </p>
          )}
          {skipped ? (
            <p className="mt-1 font-sans text-[12px] text-ash-400">
              {t("cloudAfk.console.skipped", { count: skipped })}
            </p>
          ) : null}
        </div>
        <AnimatePresence>
          {unseen ? (
            <motion.div
              key="unseen"
              className="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center"
              initial={{ opacity: 0, transform: reduce ? "none" : "translateY(6px)" }}
              animate={{
                opacity: 1,
                transform: reduce ? "none" : "translateY(0px)",
                transition: enter(0.2),
              }}
              exit={{ opacity: 0, transition: leave(0.14) }}
            >
              <button
                type="button"
                onClick={toEnd}
                className="pointer-events-auto flex h-8 cursor-pointer items-center gap-1.5 rounded-full bg-obsidian-800/95 px-3 text-[12.5px] font-medium text-bone-50 shadow-[0_8px_24px_rgb(34_6_6/0.5)] ring-1 ring-white/12 transition-[background-color,transform,scale] duration-150 outline-none hover:bg-obsidian-700 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97]"
              >
                <ArrowDown size={14} weight="bold" aria-hidden />
                {t("cloudAfk.console.newMessages")}
              </button>
            </motion.div>
          ) : null}
        </AnimatePresence>
      </div>
      <form
        className="flex flex-col gap-2 border-t border-white/8 p-4"
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
      >
        <div className="flex gap-2">
          {/* A password command turns secret as it is typed (secret.tsx: the AFK surfaces' one
              look for secret lines). */}
          <SecretField
            aria-label={t("cloudAfk.console.input")}
            placeholder={
              canType
                ? t("cloudAfk.console.input")
                : stale
                  ? t("cloudStale.console.offline")
                  : t("cloudAfk.console.offline")
            }
            value={text}
            secret={sensitive && canType}
            mono={canType}
            disabled={!canType}
            maxLength={256}
            onChange={(next) => {
              setText(next);
              setSendError(null);
            }}
            onKeyDown={onKeyDown}
            // Shut, its words say why: a still field rather than a faded one, so they stay
            // readable (a placeholder's ash-500 under 60 % read at about 2:1).
            className="h-[34px] disabled:border-white/6 disabled:bg-white/[0.02] disabled:opacity-100 disabled:placeholder:text-ash-400"
          />
          {canType ? (
            <IconButton
              label={t("cloudAfk.console.send")}
              icon={<PaperPlaneRight size={18} weight="fill" />}
              type="submit"
              disabled={sending || !text.trim()}
              className="bg-gold-500 text-gold-ink hover:bg-gold-400 hover:text-gold-ink"
            />
          ) : stoppedOnPassword(account) ? (
            // A start would stop again: the settings open on the login line (AFK-CLEAR).
            <Button
              variant="primary"
              icon={<Key size={15} weight="bold" />}
              onClick={() => onOpenSettings(account.id)}
            >
              {t("afkClear.fixPassword")}
            </Button>
          ) : !stale && (account.desired !== "run" || account.hold?.kind === "cause") ? (
            // Its machine out of reach, a start waits as on the card (CLOUD-STALE).
            <Button variant="primary" onClick={() => onStart(account)}>
              {t("cloudAfk.actions.start")}
            </Button>
          ) : null}
        </div>
        {/* One reserved line: the note or the error fades in place, nothing moves. */}
        <div className="grid min-h-[18px]">
          <AnimatePresence initial={false}>
            {sendError || (sensitive && canType) ? (
              <motion.p
                key={sendError ? "error" : "sealed"}
                role={sendError ? "alert" : "status"}
                className={cn(
                  "[grid-area:1/1] text-[12.5px] leading-[18px]",
                  // A note, not an action: never gold.
                  sendError ? "text-flare-400" : "text-ash-400",
                )}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.15) }}
                exit={{ opacity: 0, transition: leave(0.1) }}
              >
                {sendError ?? t("afkClear.secret.consoleNote")}
              </motion.p>
            ) : null}
          </AnimatePresence>
        </div>
      </form>
    </>
  ) : null;

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <AnimatePresence>
        {open && account ? (
          <EntranceGate key="sheet">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-40 bg-obsidian-950/50"
                  {...scrimMotion(!animate)}
                />
              </Dialog.Overlay>
              <Dialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                // A press outside closes the panel, but never a toast's button.
                {...sheetOutside}
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  const sheet = event.currentTarget as HTMLElement;
                  // Opened on Hareket: the panel itself, so W, A, S, D walk at once.
                  (
                    sheet.querySelector<HTMLElement>("[data-movement-panel]") ??
                    sheet.querySelector<HTMLElement>("input:not(:disabled)") ??
                    sheet
                  ).focus({
                    preventScroll: true,
                  });
                }}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className={cn(
                    "fixed top-0 right-0 bottom-0 z-50 flex w-[min(560px,92vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none",
                    // The inventory's pixels stay crisp only on a solid surface: obsidian with the
                    // glass edge, no blur (docs/design.md §6 "Cloud inventory").
                    inventory
                      ? "border-l border-white/7 bg-obsidian-900 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),-30px_0_80px_rgb(34_6_6/0.5)]"
                      : "glass-strong",
                  )}
                  {...sheetMotion(!animate)}
                >
                  <header className="flex items-center gap-3 border-b border-white/8 px-5 py-4">
                    <AfkHead nickname={account.name} size={36} />
                    <div className="min-w-0 flex-1">
                      <Dialog.Title className="truncate font-display text-[18px] leading-tight font-semibold text-bone-50">
                        {tabbed
                          ? account.name
                          : t("cloudAfk.console.title", { name: account.name })}
                      </Dialog.Title>
                      <CloudStateLine account={account} skewMs={skewMs} className="mt-0.5" />
                    </div>
                    <Dialog.Close asChild>
                      <IconButton label={t("common.close")} icon={<X size={18} />} size="sm" />
                    </Dialog.Close>
                  </header>
                  <Banner
                    status={status}
                    hostOnline={hostOnline}
                    noHost={!host}
                    onReopen={() => {
                      setStatus("connecting");
                      setAttempt((n) => n + 1);
                    }}
                  />
                  {tabbed ? (
                    <Tabs
                      value={shownTab}
                      onValueChange={setTab}
                      label={t("cloudAfkFeat.tabs.label")}
                      items={[
                        { value: "console", label: t("cloudAfkFeat.tabs.console") },
                        ...(inventory
                          ? [
                              {
                                value: "inventory" as const,
                                label: t("cloudAfkFeat.tabs.inventory"),
                              },
                            ]
                          : []),
                        ...(movement
                          ? [{ value: "movement" as const, label: t("cloudAfkMove.tab") }]
                          : []),
                      ]}
                      className="[&>[role=tablist]]:px-3.5"
                    >
                      <TabPanel value="console" className="flex flex-col overflow-hidden pt-1">
                        {consoleBody}
                      </TabPanel>
                      {inventory ? (
                        <TabPanel value="inventory" className="px-5 pt-5">
                          <InventoryPanel
                            account={account}
                            active={shownTab === "inventory"}
                            link={attempt}
                            art={art.art}
                            titleGlyph={art.titleGlyph}
                            onReconnect={() => onRestart(account)}
                            onOpenSettings={() => onOpenSettings(account.id)}
                            onUnavailable={inventoryUnavailable}
                          />
                        </TabPanel>
                      ) : null}
                      {movement ? (
                        <TabPanel value="movement" className="px-5 pt-5">
                          <MovementPanel
                            account={account}
                            active={shownTab === "movement"}
                            link={attempt}
                            onReconnect={() => onRestart(account)}
                            onUnavailable={movementUnavailable}
                          />
                        </TabPanel>
                      ) : null}
                    </Tabs>
                  ) : (
                    consoleBody
                  )}
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}
