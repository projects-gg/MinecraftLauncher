// "Mesajlar" (QoL Q1): every player who wrote to the bot in the last 7 days, and one conversation
// at a time. The owner answers in their own words (no [AFK+] mark: a person writes); then the
// assistant stays quiet in that conversation for 30 minutes, "Şimdi devret" hands it back. A reply
// goes only to someone who wrote in the last 24 hours. Unsafe lines come folded. Read live from the
// machine: when it is away, one quiet sentence. The conversation slides in from the side it
// belongs to; the composer is the chat's (rounded field, the send button turning gold).
import { CaretLeft, DotsThreeVertical, PaperPlaneRight, Prohibit } from "@phosphor-icons/react";
import {
  Button,
  cn,
  enter,
  IconButton,
  leave,
  Menu,
  PlayerHead,
  Skeleton,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed } from "../../errors";
import {
  type PlusInboxLine,
  type PlusInboxPage,
  type PlusInboxThread,
  type PlusThreadPage,
  plusApi,
} from "./api";
import { QuietEmpty } from "./feed-section";
import { GroupTitle } from "./fields";
import { PLUS_LIMITS } from "./model";
import { clockText, dayGroup, dayText, untilPhrase } from "./time";
import { inboxKey, storeUnread, threadKey, usePlusControl, usePlusUi } from "./use-plus";
import { currentLang } from "./words";

/** "14:05" today, "Dün", else the day: when a conversation last moved. */
function useWhen() {
  const { t } = useTranslation();
  const lang = currentLang();
  return (at: number) => {
    const group = dayGroup(at, Date.now());
    if (group === "today") return clockText(at, lang);
    if (group === "yesterday") return t("afkPlus.feed.yesterday");
    return dayText(at, lang);
  };
}

function ThreadRow({ thread, onOpen }: { thread: PlusInboxThread; onOpen: () => void }) {
  const { t } = useTranslation();
  const when = useWhen();
  const who =
    thread.last.from === "you"
      ? `${t("afkPlus.messages.you")}: `
      : thread.last.from === "bot"
        ? `${t("afkPlus.messages.assistant")}: `
        : "";
  return (
    <li>
      <button
        type="button"
        onClick={onOpen}
        className={cn(
          "group flex w-full cursor-pointer items-center gap-3 rounded-md px-3 py-2.5 text-left outline-none",
          "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/5 active:scale-[0.99] focus-visible:ring-2 focus-visible:ring-gold-300",
        )}
      >
        <PlayerHead name={thread.name} size={32} className="shrink-0" />
        <span className="min-w-0 flex-1">
          <span className="flex items-baseline justify-between gap-3">
            <span className="flex min-w-0 items-center gap-1.5">
              <span className="truncate text-[13.5px] font-medium text-bone-50">{thread.name}</span>
              {thread.state === "blocked" ? (
                <Prohibit size={13} className="shrink-0 text-ash-400" aria-hidden />
              ) : null}
            </span>
            <span className="shrink-0 text-[12px] text-ash-400 tabular-nums">
              {when(thread.lastAt)}
            </span>
          </span>
          <span className="mt-0.5 flex items-center justify-between gap-3">
            <span
              className={cn(
                "truncate text-[12.5px]",
                thread.unread ? "text-bone-100" : "text-ash-300",
              )}
            >
              {who}
              {thread.last.text}
            </span>
            {thread.unread ? (
              <span className="shrink-0 font-display text-[12.5px] font-semibold text-gold-200 tabular-nums">
                <span aria-hidden>{thread.unread}</span>
                <span className="sr-only">
                  {t("afkPlus.messages.unread", { count: thread.unread })}
                </span>
              </span>
            ) : null}
          </span>
        </span>
      </button>
    </li>
  );
}

function Line({ line }: { line: PlusInboxLine }) {
  const { t } = useTranslation();
  const [shown, setShown] = useState(false);
  const lang = currentLang();
  const mine = line.from !== "them";
  const label =
    line.from === "bot"
      ? t("afkPlus.messages.assistant")
      : line.from === "you"
        ? t("afkPlus.messages.you")
        : null;
  const hidden = line.hidden === "unsafe" && !shown;
  return (
    <li className={cn("flex flex-col gap-1", mine ? "items-end" : "items-start")}>
      <div
        className={cn(
          "max-w-[78%] rounded-[12px] px-3 py-2 text-[13.5px] leading-[1.45] ring-1 ring-inset",
          line.from === "you"
            ? "bg-[color-mix(in_oklab,var(--color-gold-500)_17%,var(--color-obsidian-800))] ring-gold-400/20"
            : line.from === "bot"
              ? "bg-obsidian-700/70 ring-white/8"
              : "bg-obsidian-800/85 ring-white/7",
        )}
      >
        {hidden ? (
          <p className="text-ash-300 italic">
            {t("afkPlus.messages.hidden")}{" "}
            <button
              type="button"
              onClick={() => setShown(true)}
              className="cursor-pointer rounded-sm font-medium text-gold-300 not-italic underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              {t("afkPlus.messages.show")}
            </button>
          </p>
        ) : (
          <p className="text-bone-50 [overflow-wrap:anywhere] whitespace-pre-wrap">{line.text}</p>
        )}
      </div>
      <p className="px-1 text-[11.5px] text-ash-400 tabular-nums">
        {label ? `${label}, ` : ""}
        {clockText(line.at, lang)}
      </p>
    </li>
  );
}

function Composer({
  accountId,
  name,
  page,
}: {
  accountId: string;
  name: string;
  page: PlusThreadPage;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [reason, setReason] = useState<string | null>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const ready = text.trim().length > 0 && !sending;
  const length = [...text].length;

  const send = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!ready) return;
    setSending(true);
    setReason(null);
    try {
      const outcome = await plusApi.reply(accountId, name, text.trim());
      if (outcome.kind === "sent") {
        setText("");
        queryClient.setQueryData<PlusThreadPage>(threadKey(accountId, name), (current) =>
          current
            ? {
                ...current,
                lines: [...current.lines, outcome.line],
                handoffUntil: outcome.line.at + 30 * 60,
              }
            : current,
        );
        void queryClient.invalidateQueries({ queryKey: inboxKey(accountId) });
      } else if (outcome.kind === "rejected") {
        setReason(
          t(`afkPlus.messages.rejected.${outcome.reason}`, {
            defaultValue: t("afkPlus.messages.rejected.form"),
          }),
        );
      } else if (outcome.kind === "notContacted") {
        setReason(t("afkPlus.messages.cantReply"));
      } else if (outcome.kind === "limited") {
        setReason(t("afkPlus.messages.limited"));
      } else {
        setReason(t("afkPlus.messages.offline"));
      }
    } catch (error) {
      afkFailed(error, t("afkPlus.messages.sendFailed"));
    } finally {
      setSending(false);
      field.current?.focus();
    }
  };

  if (!page.canReply) {
    return (
      <p className="rounded-[var(--radius-panel)] bg-obsidian-950/45 px-3.5 py-2.5 text-[12.5px] text-ash-300 ring-1 ring-white/8">
        {t("afkPlus.messages.cantReply")}
      </p>
    );
  }
  const first = !page.lines.some((line) => line.from === "you") && !page.handoffUntil;
  return (
    <form onSubmit={(event) => void send(event)} className="flex flex-col">
      {first ? (
        <p className="mb-1.5 px-1 text-[12px] text-ash-400">{t("afkPlus.messages.firstNote")}</p>
      ) : null}
      <div
        className={cn(
          "flex items-end gap-0.5 rounded-[var(--radius-panel)] border bg-obsidian-800/80 pr-[3px] transition-[border-color,box-shadow] duration-150",
          "border-white/10 hover:border-white/18 focus-within:border-gold-400/60 focus-within:ring-2 focus-within:ring-gold-400/20",
          reason &&
            "border-flare-400/55 focus-within:border-flare-400/70 focus-within:ring-flare-400/20",
        )}
      >
        <textarea
          ref={field}
          rows={1}
          value={text}
          maxLength={PLUS_LIMITS.replyMax * 2}
          onChange={(event) => {
            setText(event.target.value);
            setReason(null);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              void send();
            }
          }}
          aria-label={t("afkPlus.messages.reply")}
          aria-invalid={reason ? true : undefined}
          aria-describedby={`afk-plus-reply-${name}`}
          placeholder={t("afkPlus.messages.reply")}
          className="h-[38px] min-w-0 flex-1 resize-none bg-transparent py-[9px] pl-3.5 text-[13.5px] leading-5 text-bone-50 outline-none placeholder:text-ash-500"
        />
        <button
          type="submit"
          disabled={!ready}
          aria-label={t("afkPlus.messages.send")}
          className={cn(
            "group/send relative mb-[3px] inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-md outline-none",
            "transition-[transform,scale,color] duration-150 ease-out active:scale-[0.94] focus-visible:ring-2 focus-visible:ring-gold-300",
            ready ? "text-gold-ink" : "cursor-default text-ash-500",
          )}
        >
          <span
            aria-hidden
            className={cn(
              "absolute inset-0 rounded-md bg-[linear-gradient(180deg,var(--color-gold-300),var(--color-gold-500)_55%,var(--color-gold-600))] transition-[opacity,filter] duration-150 group-hover/send:brightness-110",
              ready ? "opacity-100" : "opacity-0",
            )}
          />
          <span className="relative flex">
            <PaperPlaneRight size={17} weight={ready ? "fill" : "regular"} aria-hidden />
          </span>
        </button>
      </div>
      <div
        id={`afk-plus-reply-${name}`}
        className="flex min-h-[20px] items-start justify-between gap-3 px-1 pt-1.5 text-[12px]"
      >
        <span role={reason ? "alert" : undefined} className="min-w-0 text-flare-300">
          {reason}
        </span>
        <span
          className={cn(
            "shrink-0 tabular-nums transition-opacity duration-150",
            length > PLUS_LIMITS.replyMax * 0.8 ? "opacity-100" : "opacity-0",
            length > PLUS_LIMITS.replyMax ? "text-flare-300" : "text-ash-400",
          )}
        >
          {length}/{PLUS_LIMITS.replyMax}
        </span>
      </div>
    </form>
  );
}

function Conversation({
  accountId,
  name,
  blocked,
  onBack,
}: {
  accountId: string;
  name: string;
  blocked: boolean;
  onBack: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const control = usePlusControl(accountId);
  const lang = currentLang();
  const query = useQuery({
    queryKey: threadKey(accountId, name),
    queryFn: () => plusApi.thread(accountId, name, null),
    staleTime: 10_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
  const page = query.data;
  const end = useRef<HTMLDivElement>(null);
  const lastAt = page?.lines.at(-1)?.at ?? 0;

  // Read up to the newest line once it is on screen; the rail and the card count what is left.
  useEffect(() => {
    if (!lastAt) return;
    end.current?.scrollIntoView({ block: "end" });
    void plusApi
      .read(accountId, name, lastAt)
      .then(() => {
        const data = queryClient.getQueryData<PlusInboxPage>(inboxKey(accountId));
        if (!data) return;
        const threads = data.threads.map((thread) =>
          thread.name === name ? { ...thread, unread: 0 } : thread,
        );
        queryClient.setQueryData<PlusInboxPage>(inboxKey(accountId), { ...data, threads });
        storeUnread(
          queryClient,
          accountId,
          threads.reduce((sum, thread) => sum + thread.unread, 0),
        );
      })
      .catch(() => {});
  }, [accountId, name, lastAt, queryClient]);

  const handBack = async () => {
    try {
      await plusApi.handBack(accountId, name);
      queryClient.setQueryData<PlusThreadPage>(threadKey(accountId, name), (current) =>
        current ? { ...current, handoffUntil: null } : current,
      );
      toast.success(t("afkPlus.messages.handedBack"));
    } catch (error) {
      afkFailed(error, t("cloudAfk.actions.failed"));
    }
  };

  const toggleBlock = async () => {
    const done = await control(blocked ? "unblock" : "block", name);
    if (done) {
      toast.success(
        t(blocked ? "afkPlus.messages.unblockedDone" : "afkPlus.messages.blockedDone", { name }),
      );
      void queryClient.invalidateQueries({ queryKey: inboxKey(accountId) });
    }
  };

  const handoff =
    page?.handoffUntil && page.handoffUntil * 1000 > Date.now() ? page.handoffUntil : null;

  return (
    <div className="flex flex-1 flex-col">
      <header className="mb-3 flex items-center gap-2">
        <IconButton
          label={t("afkPlus.messages.back")}
          icon={<CaretLeft size={16} weight="bold" />}
          size="sm"
          onClick={onBack}
        />
        <h3 className="min-w-0 flex-1 truncate font-display text-[15px] font-semibold text-bone-50">
          {name}
        </h3>
        <Menu
          trigger={
            <IconButton
              label={t("afkPlus.messages.more")}
              icon={<DotsThreeVertical size={18} weight="bold" />}
              size="sm"
            />
          }
          items={[
            {
              label: blocked ? t("afkPlus.messages.unblock") : t("afkPlus.messages.block"),
              icon: <Prohibit size={16} />,
              onSelect: () => void toggleBlock(),
            },
          ]}
        />
      </header>

      {query.isLoading ? (
        <div className="space-y-3" aria-hidden>
          <Skeleton className="h-9 w-[60%]" />
          <Skeleton className="ml-auto h-9 w-[55%]" />
          <Skeleton className="h-9 w-[40%]" />
        </div>
      ) : !page ? (
        <QuietEmpty icon="server">{t("afkPlus.messages.offline")}</QuietEmpty>
      ) : (
        <>
          <ul className="flex flex-1 flex-col gap-2.5 pb-3">
            {page.lines.map((line, index) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: lines have no ids and only ever append; two in one second share `at`
              <Line key={`${line.at}-${index}`} line={line} />
            ))}
          </ul>
          <div ref={end} />
          <AnimatePresence initial={false}>
            {handoff ? (
              <motion.div
                key="handoff"
                className="mb-2 flex flex-wrap items-center justify-between gap-2 rounded-[var(--radius-panel)] bg-white/[0.04] px-3.5 py-2 text-[12.5px] text-bone-100"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.2) }}
                exit={{ opacity: 0, transition: leave(0.14) }}
              >
                <span>
                  {t("afkPlus.messages.handoff", {
                    when: untilPhrase(handoff, Date.now(), lang, true),
                  })}
                </span>
                <Button size="sm" variant="ghost" onClick={() => void handBack()}>
                  {t("afkPlus.messages.handBack")}
                </Button>
              </motion.div>
            ) : null}
          </AnimatePresence>
          {/* Docked at the bottom of the panel while the conversation scrolls under it. A blocked
              player's conversation says so where the reply would go, with the way back. */}
          <div className="sticky bottom-0 -mx-6 mt-auto border-t border-white/7 bg-[color-mix(in_oklab,var(--color-obsidian-900)_95%,transparent)] px-6 pt-3">
            {blocked ? (
              <div className="mb-3 flex flex-wrap items-center justify-between gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/45 px-3.5 py-2.5 ring-1 ring-white/8">
                <p className="min-w-0 text-[12.5px] text-ash-300">
                  {t("afkPlus.messages.blocked")}
                </p>
                <Button size="sm" variant="ghost" onClick={() => void toggleBlock()}>
                  {t("afkPlus.messages.unblock")}
                </Button>
              </div>
            ) : (
              <Composer accountId={accountId} name={name} page={page} />
            )}
          </div>
        </>
      )}
    </div>
  );
}

export function MessagesSection({ accountId }: { accountId: string }) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const thread = usePlusUi((s) => s.thread);
  const setThread = usePlusUi((s) => s.setThread);
  const query = useQuery({
    queryKey: inboxKey(accountId),
    queryFn: () => plusApi.inbox(accountId, null),
    staleTime: 15_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
  const threads = query.data?.threads ?? [];
  const open = threads.find((one) => one.name === thread);
  const slide = (x: number) => (reduce ? "none" : `translateX(${x}px)`);

  return (
    <AnimatePresence mode="popLayout" initial={false}>
      {thread ? (
        <motion.div
          key={`thread-${thread}`}
          initial={{ opacity: 0, transform: slide(18) }}
          animate={{ opacity: 1, transform: slide(0), transition: enter(0.26) }}
          exit={{ opacity: 0, transform: slide(18), transition: leave(0.18) }}
          className="flex flex-1 flex-col"
        >
          <Conversation
            accountId={accountId}
            name={thread}
            blocked={open?.state === "blocked"}
            onBack={() => setThread(null)}
          />
        </motion.div>
      ) : (
        <motion.div
          key="list"
          initial={{ opacity: 0, transform: slide(-18) }}
          animate={{ opacity: 1, transform: slide(0), transition: enter(0.26) }}
          exit={{ opacity: 0, transform: slide(-18), transition: leave(0.18) }}
        >
          <GroupTitle>{t("afkPlus.messages.title")}</GroupTitle>
          {query.isLoading ? (
            <div className="space-y-2.5 pt-1" aria-hidden>
              <Skeleton className="h-12 w-full" />
              <Skeleton className="h-12 w-full" />
              <Skeleton className="h-12 w-full" />
            </div>
          ) : query.isError ? (
            // The rail already says the assistant can't be reached: here, what still works.
            <QuietEmpty icon="server">{t("afkPlus.messages.offlineBody")}</QuietEmpty>
          ) : threads.length ? (
            <ul className="-mx-3 flex flex-col">
              {threads.map((one) => (
                <ThreadRow key={one.name} thread={one} onOpen={() => setThread(one.name)} />
              ))}
            </ul>
          ) : (
            <QuietEmpty icon="chat">{t("afkPlus.messages.empty")}</QuietEmpty>
          )}
        </motion.div>
      )}
    </AnimatePresence>
  );
}
