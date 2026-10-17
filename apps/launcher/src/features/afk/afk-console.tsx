import { ArrowDown, PaperPlaneRight, Play, X } from "@phosphor-icons/react";
import {
  Button,
  cn,
  EntranceGate,
  enter,
  GatedMotion,
  IconButton,
  Input,
  leave,
  Switch,
  scrimMotion,
  sheetMotion,
  useInteractionMotion,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { Dialog } from "radix-ui";
import { type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { type AfkAccount, api } from "@/lib/api";
import { AfkHead } from "./afk-head";
import { afkFailed, runAfk } from "./errors";
import { StateLine } from "./state-line";

/** Lines typed per account this session, newest last: ↑ and ↓ walk through them. */
const histories = new Map<string, string[]>();
const HISTORY_MAX = 50;

/** How close to the end still counts as reading the newest lines (px). */
const STICK_PX = 32;

/** Whether the client runs, so the console can send (older backends: from the state). */
function isLive(account: AfkAccount): boolean {
  return account.running ?? ["starting", "connecting", "connected"].includes(account.state.kind);
}

/** Live console for one AFK account in a side sheet. Closing it never stops the session. */
export function AfkConsole({
  account,
  onClose,
}: {
  account: AfkAccount | undefined;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const animate = useInteractionMotion();
  const { reduce } = useMotionPrefs();
  const queryClient = useQueryClient();
  const [line, setLine] = useState("");
  /** Index in the history while ↑/↓ recall a line (null: typing a new one). */
  const [recall, setRecall] = useState<number | null>(null);
  const draft = useRef("");
  /** New lines arrived while the player read further up. */
  const [unseen, setUnseen] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const atEnd = useRef(true);
  const open = Boolean(account);
  const id = account?.id;
  const live = account ? isLive(account) : false;

  const { data: log = [] } = useQuery({
    queryKey: ["afk-log", id],
    queryFn: () => api.afkLog(id ?? ""),
    enabled: open,
  });

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
  const timeFormat: Intl.DateTimeFormatOptions = account?.showDate
    ? {
        day: "2-digit",
        month: "2-digit",
        year: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      }
    : { hour: "2-digit", minute: "2-digit", second: "2-digit" };

  useEffect(
    () => api.onAfkChange(() => void queryClient.invalidateQueries({ queryKey: ["afk-log", id] })),
    [queryClient, id],
  );

  const visible = showScript ? log : log.filter((l) => l.kind !== "script");
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
  }, [id]);

  // New lines: followed while the player reads the end; otherwise a button offers the way down.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs for every new line
  useLayoutEffect(() => {
    const el = listRef.current;
    if (!el || !visible.length) return;
    if (atEnd.current) el.scrollTop = el.scrollHeight;
    else setUnseen(true);
  }, [lastKey]);

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

  const send = () => {
    const text = line.trim();
    if (!text || !account || !live) return;
    const list = histories.get(account.id) ?? [];
    if (list.at(-1) !== text) list.push(text);
    if (list.length > HISTORY_MAX) list.splice(0, list.length - HISTORY_MAX);
    histories.set(account.id, list);
    setRecall(null);
    setLine("");
    // The player's own line shows at the end.
    atEnd.current = true;
    api.afkSend(account.id, text).catch((error: unknown) => {
      afkFailed(error, t("afk.sendFailed"));
      // Nothing typed is lost: the line comes back unless a new one was started.
      setLine((current) => current || text);
    });
  };

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
                // Start in the command line, not on the close button (its tooltip would pop open).
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  const sheet = event.currentTarget as HTMLElement;
                  (sheet.querySelector<HTMLElement>("input:not(:disabled)") ?? sheet).focus({
                    preventScroll: true,
                  });
                }}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-50 flex w-[min(560px,92vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...sheetMotion(!animate)}
                >
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
                    <Dialog.Close asChild>
                      <IconButton label={t("common.close")} icon={<X size={18} />} size="sm" />
                    </Dialog.Close>
                  </header>
                  <div className="relative min-h-0 flex-1">
                    <div
                      ref={listRef}
                      onScroll={onScroll}
                      className="absolute inset-0 overflow-y-auto px-5 py-3 font-mono text-[12.5px] leading-relaxed"
                      data-selectable
                      role="log"
                      aria-live="polite"
                    >
                      {visible.length ? (
                        visible.map((l) => (
                          <p
                            key={l.id}
                            className={cn(
                              "break-words",
                              l.kind === "error" && "text-flare-400",
                              l.kind === "system" && "text-ash-400",
                              l.kind === "script" && "text-gold-300/80",
                              l.kind === "self" && "text-gold-200",
                              l.kind === "chat" && "text-bone-100",
                            )}
                          >
                            <span className="mr-2 text-ash-400 tabular-nums">
                              {new Date(l.time).toLocaleString("tr-TR", timeFormat)}
                            </span>
                            {l.text}
                          </p>
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
                            className="pointer-events-auto flex h-8 cursor-pointer items-center gap-1.5 rounded-full bg-obsidian-800/95 px-3 text-[12.5px] font-medium text-bone-50 shadow-[0_8px_24px_rgb(34_6_6/0.5)] ring-1 ring-white/12 transition-[background-color,transform] duration-150 outline-none hover:bg-obsidian-700 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97]"
                          >
                            <ArrowDown size={14} weight="bold" aria-hidden />
                            {t("afk.newMessages")}
                          </button>
                        </motion.div>
                      ) : null}
                    </AnimatePresence>
                  </div>
                  <form
                    className="flex gap-2 border-t border-white/8 p-4"
                    onSubmit={(e) => {
                      e.preventDefault();
                      send();
                    }}
                  >
                    <Input
                      aria-label={t("afk.consoleInput")}
                      placeholder={live ? t("afk.consoleInput") : t("afk.consoleOffline")}
                      value={line}
                      disabled={!live}
                      onChange={(e) => setLine(e.target.value)}
                      onKeyDown={onKeyDown}
                      spellCheck={false}
                      autoComplete="off"
                      // The shared disabled look, a little brighter: the placeholder says why.
                      className={cn("disabled:opacity-60", live && "font-mono text-[13px]")}
                    />
                    {live ? (
                      <IconButton
                        label={t("afk.send")}
                        icon={<PaperPlaneRight size={18} weight="fill" />}
                        type="submit"
                        className="bg-gold-500 text-gold-ink hover:bg-gold-400 hover:text-gold-ink"
                      />
                    ) : (
                      <Button
                        type="button"
                        variant="primary"
                        icon={<Play size={14} weight="fill" />}
                        disabled={account.state.kind === "stopping"}
                        onClick={() => runAfk(api.afkConnect(account.id), t("afk.connectFailed"))}
                      >
                        {t("afk.connect")}
                      </Button>
                    )}
                  </form>
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}
