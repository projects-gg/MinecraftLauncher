// Live console for one desktop AFK account in the AFK side panel (sized by its left edge, AFK-UX).
// Closing it never stops the session.
//
// With console looks (a client of 4.2.4 or later while Studio's switch was on at its launch:
// pl-afk decides) lines show in the game's colours: a part's hover as the game's tooltip, a
// command or link waiting in the command line's place for the player's word, a suggestion filling
// the command line, a copy copying; nothing happens on its own. A server's dialog shows there as a
// card (dialog cards); a press goes to pl-afk and the client, which check it again and answer. A
// client of 4.2.5 also shows the game's waiting screen there, and what the server's pause screen
// and quick actions key open as a quiet row above the command line (CONSOLE-ANY).
import { ArrowDown, PaperPlaneRight, X } from "@phosphor-icons/react";
import { cn, enter, IconButton, leave, Switch, toast, useMotionPrefs } from "@pl/ui";
import {
  CONSOLE_TEXTS,
  ConsoleActionCard,
  type ConsoleClick,
  ConsoleText,
  copiedPreview,
  type DialogAct,
  DialogCard,
  type DialogMenu,
  DialogMenus,
  DialogWaiting,
  dialogRefusal,
  safeDialog,
  safeMenus,
} from "@pl/ui/console";
import { fill } from "@pl/ui/inventory";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { Dialog } from "radix-ui";
import {
  type KeyboardEvent,
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { type AfkAccount, type AfkLogLine, api } from "@/lib/api";
import { uiLocale } from "@/lib/format";
import { openWebLink } from "@/lib/links";
import { AfkHead } from "./afk-head";
import { looksSensitive } from "./cloud/sensitive";
import { afkFailed, isFriendly } from "./errors";
import { AfkSidePanel, PanelSizeButton } from "./panel/side-panel";
import { AfkPrimaryAction } from "./primary-action";
import { SecretField } from "./secret";
import { StateLine } from "./state-line";

/** Lines typed per account this session, newest last: ↑ and ↓ walk through them. */
const histories = new Map<string, string[]>();
const HISTORY_MAX = 50;

/** How close to the end still counts as reading the newest lines (px). */
const STICK_PX = 32;

const TIME: Intl.DateTimeFormatOptions = { hour: "2-digit", minute: "2-digit", second: "2-digit" };
/** With the date (the account's chat log settings ask for it). */
const DATE_TIME: Intl.DateTimeFormatOptions = {
  day: "2-digit",
  month: "2-digit",
  year: "numeric",
  ...TIME,
};

/** Whether the client runs, so the console can send (older backends: from the state). */
function isLive(account: AfkAccount): boolean {
  return account.running ?? ["starting", "connecting", "connected"].includes(account.state.kind);
}

/** A clicked command or link waiting for the player's word, with whose line it came from. */
interface Pending {
  click: ConsoleClick;
  who: string | null;
  clock: string;
}

/** A time as the console shows it (an RFC 3339 instant). */
function clockOf(time: string, format: Intl.DateTimeFormatOptions = TIME): string {
  return new Date(time).toLocaleString(uiLocale(), format);
}

const TONE: Record<AfkLogLine["kind"], string> = {
  error: "text-flare-400",
  system: "text-ash-400",
  script: "text-gold-300/80",
  self: "text-gold-200",
  chat: "text-bone-100",
};

/**
 * The lines with the objects and keys of the last read where they are the same line: a new line
 * renders only itself (the console is read whole after every event).
 */
function useSteadyLines(lines: AfkLogLine[]): { line: AfkLogLine; key: string }[] {
  const kept = useRef(new Map<string, AfkLogLine>());
  return useMemo(() => {
    const next = new Map<string, AfkLogLine>();
    const seen = new Map<string, number>();
    const out = lines.map((line) => {
      const base = `${line.time}|${line.st ? "l" : ""}|${line.text}`;
      const n = seen.get(base) ?? 0;
      seen.set(base, n + 1);
      const key = `${base}|${n}`;
      const steady = kept.current.get(key) ?? line;
      next.set(key, steady);
      return { line: steady, key };
    });
    kept.current = next;
    return out;
  }, [lines]);
}

/** One console line: its time and words, in the game's colours when it has its look. */
const LineRow = memo(function LineRow({
  line,
  format,
  lang,
  onPart,
}: {
  line: AfkLogLine;
  format: Intl.DateTimeFormatOptions;
  lang: "tr" | "en";
  /** A part's click; absent while nothing can be sent. */
  onPart?: (line: AfkLogLine, click: ConsoleClick) => void;
}) {
  const onClick = useCallback((click: ConsoleClick) => onPart?.(line, click), [line, onPart]);
  return (
    <p className={cn("break-words", TONE[line.kind])}>
      <span className="mr-2 text-ash-400 tabular-nums">{clockOf(line.time, format)}</span>
      {line.st ? (
        <ConsoleText
          text={line.text}
          st={line.st}
          ev={line.ev}
          lang={lang}
          onClick={onPart ? onClick : undefined}
        >
          {line.text}
        </ConsoleText>
      ) : (
        line.text
      )}
    </p>
  );
});

/** Live console for one AFK account in the side panel. Closing it never stops the session. */
export function AfkConsole({
  account,
  onClose,
  onFixPassword,
}: {
  account: AfkAccount | undefined;
  onClose: () => void;
  /** "Şifreyi düzelt" after the server turned the password down: the settings open instead. */
  onFixPassword?: (id: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language === "en" ? "en" : "tr";
  const { reduce } = useMotionPrefs();
  const queryClient = useQueryClient();
  const [line, setLine] = useState("");
  /** Index in the history while ↑/↓ recall a line (null: typing a new one). */
  const [recall, setRecall] = useState<number | null>(null);
  const draft = useRef("");
  /** New lines arrived while the player read further up. */
  const [unseen, setUnseen] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);
  const [pressing, setPressing] = useState(false);
  /** Why the last press of the dialog card did not go through. */
  const [dialogError, setDialogError] = useState<string | null>(null);
  // A menu's opening that did not go through (its dialog comes as a card when it does).
  const [menuError, setMenuError] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const atEnd = useRef(true);
  const open = Boolean(account);
  const id = account?.id;
  const live = account ? isLive(account) : false;

  const { data: log = [] } = useQuery({
    queryKey: ["afk-log", id],
    queryFn: () => api.afkLog(id ?? ""),
    enabled: open,
  });

  // The server's open dialog (pl-afk keeps it only while Studio's switch allows dialog cards).
  const { data: dialogView = null } = useQuery({
    queryKey: ["afk-dialog", id],
    queryFn: () => api.afkDialog(id ?? ""),
    enabled: open && live,
  });
  const dialog = useMemo(() => {
    if (!dialogView) return null;
    // The game's waiting screen in a dialog's place (client 4.2.5).
    if (dialogView.wait) return { rev: dialogView.rev, dialog: null, at: dialogView.at };
    try {
      const checked = safeDialog(JSON.parse(dialogView.dialog));
      return checked ? { rev: dialogView.rev, dialog: checked, at: dialogView.at } : null;
    } catch {
      return null;
    }
  }, [dialogView]);
  // What the server's pause screen and quick actions key open (client 4.2.5).
  const { data: menusView = null } = useQuery({
    queryKey: ["afk-dialog-menus", id],
    queryFn: () => api.afkDialogMenus(id ?? ""),
    enabled: open && live,
  });
  const menus = useMemo(() => {
    try {
      return safeMenus(menusView ? JSON.parse(menusView) : null);
    } catch {
      return {};
    }
  }, [menusView]);

  // Remembered for every account (AFK settings.json); lines with passwords never reach the UI.
  const { data: global } = useQuery({
    queryKey: ["afk-global-settings"],
    queryFn: () => api.afkGlobalSettings(),
    enabled: open,
  });
  const showScript = global?.showScriptCommands ?? false;
  const setShowScript = (show: boolean) => {
    queryClient.setQueryData(["afk-global-settings"], { showScriptCommands: show });
    void api
      .afkSetShowScriptCommands(show)
      .catch(() => void queryClient.invalidateQueries({ queryKey: ["afk-global-settings"] }));
  };
  const format = account?.showDate ? DATE_TIME : TIME;

  useEffect(
    () => api.onAfkChange(() => void queryClient.invalidateQueries({ queryKey: ["afk-log", id] })),
    [queryClient, id],
  );
  useEffect(
    () =>
      api.onAfkDialog((accountId) => {
        if (accountId !== id) return;
        void queryClient.invalidateQueries({ queryKey: ["afk-dialog", id] });
        void queryClient.invalidateQueries({ queryKey: ["afk-dialog-menus", id] });
      }),
    [queryClient, id],
  );

  const visible = useMemo(
    () => (showScript ? log : log.filter((l) => l.kind !== "script")),
    [log, showScript],
  );
  const rows = useSteadyLines(visible);
  const last = visible.at(-1);
  // Changes with every new line, also once the log is full and its length stays the same.
  const lastKey = `${visible.length}|${last?.time ?? ""}|${last?.text ?? ""}`;

  // Another account (or a fresh open) starts at its newest line with an empty command line.
  // biome-ignore lint/correctness/useExhaustiveDependencies: resets when the account changes
  useEffect(() => {
    atEnd.current = true;
    setUnseen(false);
    setLine("");
    setRecall(null);
    setPending(null);
    setDialogError(null);
  }, [id]);

  // A new dialog (or its close): an error of an earlier press no longer applies.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the dialog changes
  useEffect(() => {
    setDialogError(null);
    setMenuError(null);
  }, [dialog?.rev]);

  // New lines: followed while the player reads the end; otherwise a button offers the way down.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs for every new line
  useLayoutEffect(() => {
    const el = listRef.current;
    if (!el || !visible.length) return;
    if (atEnd.current) el.scrollTop = el.scrollHeight;
    else setUnseen(true);
  }, [lastKey]);

  // The panel's width changes (its edge dragged or settled): the newest lines stay in view.
  const attachList = useCallback((el: HTMLDivElement | null) => {
    listRef.current = el;
    if (!el || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (atEnd.current) el.scrollTop = el.scrollHeight;
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
      listRef.current = null;
    };
  }, []);

  const onScroll = () => {
    const el = listRef.current;
    if (!el) return;
    atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_PX;
    if (atEnd.current) setUnseen(false);
  };

  const toEnd = () => {
    const el = listRef.current;
    if (!el) return;
    atEnd.current = true;
    el.scrollTo({ top: el.scrollHeight, behavior: reduce ? "auto" : "smooth" });
    setUnseen(false);
  };

  /** A typed line, or a clicked command the player said yes to: the same way to the client. */
  const deliver = (text: string, typed: boolean) => {
    if (!text || !account || !live) return;
    if (typed) {
      const list = histories.get(account.id) ?? [];
      // A password line is never kept for ↑ (it is masked while typed, as in the cloud console).
      if (list.at(-1) !== text && !looksSensitive(text)) list.push(text);
      if (list.length > HISTORY_MAX) list.splice(0, list.length - HISTORY_MAX);
      histories.set(account.id, list);
      setRecall(null);
      setLine("");
    }
    // The player's own line shows at the end.
    atEnd.current = true;
    api.afkSend(account.id, text).catch((error: unknown) => {
      afkFailed(error, t("afk.sendFailed"));
      // Nothing typed is lost: the line comes back unless a new one was started.
      if (typed) setLine((current) => current || text);
    });
  };
  const send = () => deliver(line.trim(), true);

  const copy = useCallback(
    (value: string) => {
      navigator.clipboard.writeText(value).then(
        // What was copied, shown (a part never copies something the player has not seen).
        () => toast(fill(CONSOLE_TEXTS[lang].copiedValue, { value: copiedPreview(value) })),
        () => undefined,
      );
    },
    [lang],
  );

  // A part's click: a command or a link waits for the player's word, a suggestion fills the
  // command line (nothing is sent), a copy copies.
  const onPart = useCallback(
    (from: AfkLogLine, click: ConsoleClick) => {
      switch (click.a) {
        case "run_command":
        case "open_url":
          setPending({ click, who: from.who ?? null, clock: clockOf(from.time) });
          return;
        case "suggest_command":
          setLine(click.v);
          setRecall(null);
          requestAnimationFrame(() => inputRef.current?.focus());
          return;
        case "copy_to_clipboard":
          copy(click.v);
      }
    },
    [copy],
  );
  const closePending = () => {
    setPending(null);
    requestAnimationFrame(() => inputRef.current?.focus());
  };
  const confirmPending = () => {
    if (!pending) return;
    const { click } = pending;
    closePending();
    if (click.a === "run_command") deliver(click.v, false);
    else if (click.a === "open_url") openWebLink(click.v);
  };

  // A press on the server's dialog card: a command only as the card asked the player (the card
  // builds it; pl-afk and the client check it again and answer).
  const pressDialog = async (act: DialogAct) => {
    if (!account || !dialog || pressing) return;
    setPressing(true);
    setDialogError(null);
    try {
      const outcome = await api.afkDialogPress(account.id, { rev: dialog.rev, ...act });
      if (!outcome.ok) setDialogError(dialogRefusal(outcome.why, lang));
    } catch (error) {
      setDialogError(isFriendly(error) ? error.message : t("afk.sendFailed"));
    } finally {
      setPressing(false);
    }
  };
  // A menu of the server's pause screen or quick actions key: opens as the game opens it (its
  // dialog comes as a card).
  const openMenu = async (menu: DialogMenu) => {
    if (!account || pressing) return;
    setPressing(true);
    setMenuError(null);
    try {
      const outcome = await api.afkDialogPress(account.id, { open: menu });
      if (!outcome.ok) setMenuError(dialogRefusal(outcome.why, lang));
    } catch (error) {
      setMenuError(isFriendly(error) ? error.message : t("afk.sendFailed"));
    } finally {
      setPressing(false);
    }
  };
  // The card's own clicks: a confirmed link opens, a suggestion fills the command line (shown
  // again once the dialog closes), a copy copies.
  const onDialogClick = (click: ConsoleClick) => {
    if (click.a === "open_url") openWebLink(click.v);
    else if (click.a === "suggest_command") setLine(click.v);
    else if (click.a === "copy_to_clipboard") copy(click.v);
  };
  const card = live && dialog?.dialog && !pending ? { ...dialog, dialog: dialog.dialog } : null;
  const waiting = live && dialog && !dialog.dialog && !pending ? dialog : null;

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (!account) return;
    const list = histories.get(account.id) ?? [];
    if (event.key === "ArrowUp" && list.length) {
      event.preventDefault();
      if (recall === null) draft.current = line;
      const next = recall === null ? list.length - 1 : Math.max(0, recall - 1);
      setRecall(next);
      setLine(list[next] ?? "");
    } else if (event.key === "ArrowDown" && recall !== null) {
      event.preventDefault();
      const next = recall + 1;
      if (next >= list.length) {
        setRecall(null);
        setLine(draft.current);
      } else {
        setRecall(next);
        setLine(list[next] ?? "");
      }
    }
  };

  return (
    <AfkSidePanel
      open={open}
      onClose={onClose}
      // Start in the command line, not on the close button (its tooltip would pop open).
      onOpenAutoFocus={(event) => {
        event.preventDefault();
        const sheet = event.currentTarget as HTMLElement;
        (sheet.querySelector<HTMLElement>("input:not(:disabled)") ?? sheet).focus({
          preventScroll: true,
        });
      }}
    >
      {account ? (
        <>
          <header className="flex items-center gap-3 border-b border-white/8 px-5 py-4">
            <AfkHead nickname={account.nickname} size={36} />
            <div className="min-w-0 flex-1">
              <Dialog.Title className="truncate font-display text-[18px] leading-tight font-semibold text-bone-50">
                {t("afk.consoleTitle", { name: account.nickname })}
              </Dialog.Title>
              <StateLine state={account.state} className="mt-0.5" />
            </div>
            <Switch
              checked={showScript}
              onCheckedChange={setShowScript}
              label={t("afk.showScript")}
            />
            <PanelSizeButton />
            <Dialog.Close asChild>
              <IconButton label={t("common.close")} icon={<X size={18} />} size="sm" />
            </Dialog.Close>
          </header>
          <div className="relative min-h-0 flex-1">
            <div
              ref={attachList}
              onScroll={onScroll}
              className="absolute inset-0 overflow-y-auto px-5 py-3 font-mono text-[12.5px] leading-relaxed"
              data-selectable
              role="log"
              aria-live="polite"
            >
              {rows.length ? (
                rows.map(({ line: row, key }) => (
                  <LineRow
                    key={key}
                    line={row}
                    format={format}
                    lang={lang}
                    onPart={live ? onPart : undefined}
                  />
                ))
              ) : (
                <p className="flex h-full items-center justify-center font-sans text-[13px] text-ash-400">
                  {t("afk.consoleEmpty")}
                </p>
              )}
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
                    {t("afk.newMessages")}
                  </button>
                </motion.div>
              ) : null}
            </AnimatePresence>
          </div>
          {pending ? (
            <div className="border-t border-white/8 p-4">
              <ConsoleActionCard
                click={pending.click}
                lang={lang}
                who={pending.who}
                clock={pending.clock}
                onConfirm={confirmPending}
                onCancel={closePending}
              />
            </div>
          ) : null}
          {card ? (
            <div className="border-t border-white/8 p-4">
              <DialogCard
                key={card.rev}
                dialog={card.dialog}
                lang={lang}
                clock={clockOf(card.at)}
                busy={pressing}
                error={dialogError}
                onAct={(act) => void pressDialog(act)}
                onClick={onDialogClick}
              />
            </div>
          ) : null}
          {waiting ? (
            <div className="border-t border-white/8 p-4">
              <DialogWaiting
                key={waiting.rev}
                lang={lang}
                busy={pressing}
                error={dialogError}
                onBack={() => void pressDialog({ cancel: true })}
              />
            </div>
          ) : null}
          <form
            className={cn(
              "flex flex-col gap-2 border-t border-white/8 p-4",
              (pending || card || waiting) && "hidden",
            )}
            onSubmit={(e) => {
              e.preventDefault();
              send();
            }}
          >
            {/* What the server's pause screen and quick actions key open (client 4.2.5). */}
            {live ? (
              <DialogMenus
                menus={menus}
                lang={lang}
                busy={pressing}
                onOpen={(menu) => void openMenu(menu)}
              />
            ) : null}
            {live && menuError ? (
              <p role="alert" className="text-[12.5px] text-flare-400">
                {menuError}
              </p>
            ) : null}
            <div className="flex gap-2">
              {/* A password command turns secret as it is typed: the AFK surfaces' one look for
                  secret lines (secret.tsx). */}
              <SecretField
                ref={inputRef}
                aria-label={t("afk.consoleInput")}
                placeholder={live ? t("afk.consoleInput") : t("afk.consoleOffline")}
                value={line}
                secret={live && looksSensitive(line)}
                mono={live}
                disabled={!live}
                onChange={setLine}
                onKeyDown={onKeyDown}
                // The shared disabled look, a little brighter: the placeholder says why.
                className="h-[34px] disabled:opacity-60"
              />
              {live ? (
                <IconButton
                  label={t("afk.send")}
                  icon={<PaperPlaneRight size={18} weight="fill" />}
                  type="submit"
                  className="bg-gold-500 text-gold-ink hover:bg-gold-400 hover:text-gold-ink"
                />
              ) : (
                // Bağla; İptal while its client is fetched; Tekrar dene after it could not be.
                <AfkPrimaryAction
                  account={account}
                  size="md"
                  onFixPassword={onFixPassword ? () => onFixPassword(account.id) : undefined}
                />
              )}
            </div>
            {/* One reserved line, as in the 7/24 Bulut console: the note fades in place. */}
            <div className="grid min-h-[18px]">
              <AnimatePresence initial={false}>
                {live && looksSensitive(line) ? (
                  <motion.p
                    key="secret"
                    role="status"
                    className="[grid-area:1/1] text-[12.5px] leading-[18px] text-ash-400"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1, transition: enter(0.15) }}
                    exit={{ opacity: 0, transition: leave(0.1) }}
                  >
                    {t("afkClear.secret.consoleNote")}
                  </motion.p>
                ) : null}
              </AnimatePresence>
            </div>
          </form>
        </>
      ) : null}
    </AfkSidePanel>
  );
}
