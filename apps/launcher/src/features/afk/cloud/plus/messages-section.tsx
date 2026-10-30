// A player's conversation with the bot (QoL Q1), opened under their row in "Etkinlik" ("Cevap yaz";
// PLUS-ACTIVITY: the conversations are no section of their own any more, the site's and the phone's
// same place). The owner answers in their own words (no [AFK+] mark: a person writes); then the
// assistant stays quiet in that conversation for 30 minutes, "Şimdi devret" hands it back. A reply
// goes only to someone who wrote in the last 24 hours. Unsafe lines come folded. Read live from the
// machine only while it is open: when it is away, one quiet sentence. The composer is the chat's
// (rounded field, the send button turning gold only once there is something to send).
import { DotsThreeVertical, PaperPlaneRight, Prohibit, UserPlus } from "@phosphor-icons/react";
import {
  Button,
  cn,
  enter,
  IconButton,
  leave,
  Menu,
  Skeleton,
  scrollBehavior,
  toast,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed } from "../../errors";
import {
  type PlusInboxLine,
  type PlusInboxPage,
  type PlusPanel,
  type PlusThreadPage,
  plusApi,
} from "./api";
import { QuietEmpty } from "./fields";
import { PLUS_LIMITS } from "./model";
import { clockText, untilPhrase } from "./time";
import {
  inboxKey,
  plusKey,
  storeUnread,
  threadKey,
  usePlusControl,
  usePlusInstantSave,
} from "./use-plus";
import { currentLang } from "./words";

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
              className="cursor-pointer rounded-sm font-medium text-bone-100 not-italic underline underline-offset-2 outline-none hover:text-bone-50 focus-visible:ring-2 focus-visible:ring-gold-300"
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
  autoFocus = false,
}: {
  accountId: string;
  name: string;
  page: PlusThreadPage;
  /** Opened by "Cevap yaz": the field takes the focus once (the scroll is the conversation's). */
  autoFocus?: boolean;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [reason, setReason] = useState<string | null>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const ready = text.trim().length > 0 && !sending;
  const length = [...text].length;
  const focusOnce = useRef(autoFocus);
  useEffect(() => {
    if (!focusOnce.current) return;
    focusOnce.current = false;
    field.current?.focus({ preventScroll: true });
  }, []);

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
        setReason(t("afkPlus.messages.sendAway"));
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
            // Empty: the quiet plate (the site's); gold only once there is something to send.
            ready ? "text-gold-ink" : "cursor-default bg-white/[0.05] text-ash-400",
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

/**
 * The conversation with one player, under their steps: its title ("Yazışma"; the row above names
 * the player), its lines, the hand-back while the player holds it, and the reply field (or why
 * there is none). Opening it reads it and marks it read up to its newest line.
 */
export function Conversation({
  accountId,
  name,
  blocked,
  autoFocus = false,
}: {
  accountId: string;
  name: string;
  blocked: boolean;
  /** Opened by "Cevap yaz": the reply field takes the focus once it shows. */
  autoFocus?: boolean;
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

  // Read up to the newest line once it is on screen; "Seni bekleyenler" and the rail count what is
  // left. The reply field comes into view (the least scroll that shows it).
  useEffect(() => {
    if (!lastAt) return;
    end.current?.scrollIntoView({ block: "nearest", behavior: scrollBehavior() });
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

  // "Arkadaşlarıma ekle": the name joins "Kimlerle"'s list (afkplus-free.md §7.6), saved at once.
  const instant = usePlusInstantSave(accountId);
  const { data: panel } = useQuery<PlusPanel>({
    queryKey: plusKey(accountId),
    queryFn: () => plusApi.panel(accountId),
    enabled: false,
  });
  const friends = panel?.settings.config.who.friends ?? [];
  const same = (other: string) => other.toLowerCase() === name.toLowerCase();
  const friendsMax = panel?.settings.rules.friendsMax || PLUS_LIMITS.friendsMax;
  const canAddFriend =
    Boolean(panel) && !blocked && !friends.some(same) && friends.length < friendsMax;
  const addFriend = async () => {
    const done = await instant((latest) =>
      latest.who.friends.some(same)
        ? latest
        : { ...latest, who: { ...latest.who, friends: [...latest.who.friends, name] } },
    );
    if (done) toast.success(t("afkPlus.messages.friendAdded", { name }));
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
    <section aria-label={t("afkPlus.audit.conversation")} className="flex flex-col">
      <header className="mb-2.5 flex items-center gap-2">
        <h4 className="min-w-0 flex-1 truncate text-[13px] font-semibold text-bone-100">
          {t("afkPlus.audit.conversation")}
        </h4>
        <Menu
          trigger={
            <IconButton
              label={t("afkPlus.messages.more")}
              icon={<DotsThreeVertical size={18} weight="bold" />}
              size="sm"
            />
          }
          items={[
            ...(canAddFriend
              ? [
                  {
                    label: t("afkPlus.messages.addFriend"),
                    icon: <UserPlus size={16} />,
                    onSelect: () => void addFriend(),
                  },
                ]
              : []),
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
          <ul className="flex flex-col gap-2.5 pb-3">
            {page.lines.map((line, index) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: lines have no ids and only ever append; two in one second share `at`
              <Line key={`${line.at}-${index}`} line={line} />
            ))}
          </ul>
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
          {/* A blocked player's conversation says so where the reply would go, with the way back. */}
          {blocked ? (
            <div className="flex flex-wrap items-center justify-between gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/45 px-3.5 py-2.5 ring-1 ring-white/8">
              <p className="min-w-0 text-[12.5px] text-ash-300">{t("afkPlus.messages.blocked")}</p>
              <Button size="sm" variant="ghost" onClick={() => void toggleBlock()}>
                {t("afkPlus.messages.unblock")}
              </Button>
            </div>
          ) : (
            <Composer accountId={accountId} name={name} page={page} autoFocus={autoFocus} />
          )}
          <div ref={end} />
        </>
      )}
    </section>
  );
}
