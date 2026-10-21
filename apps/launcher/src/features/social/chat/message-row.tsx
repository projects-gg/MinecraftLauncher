// One chat message: a bubble (the player's own on the right, gold-tinted; others' on the left with
// their picture at the end of a group and, in crew chat, their name at its start), "düzenlendi",
// the time and where it stands (on its way, sent, not sent with "Tekrar dene"). Its menu ("…" on
// hover or focus, a right click, Shift+F10): copy, edit (15 minutes), "Herkesten sil" (asked in
// place, one sentence), "Rapor et" (others' messages).
import {
  ArrowClockwise,
  Check,
  Clock,
  Copy,
  DotsThree,
  Flag,
  PencilSimple,
  Trash,
  WarningCircle,
} from "@phosphor-icons/react";
import { Button, ContextMenu, cn, IconButton, Menu, type MenuItem, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { memo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ChatMessage, SocialProfile } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { copyText } from "../actions";
import { enter, leave, rise, swap, useSocialMotion } from "../motion";
import { SocialFace } from "../social-face";

/** What a row needs from its conversation. */
export interface RowPlace {
  /** Crew chat: names show over others' groups. */
  crew: boolean;
  /** The first of its sender's group (name), the last (picture, time). */
  first: boolean;
  last: boolean;
}

export interface RowActions {
  onEdit: (message: ChatMessage) => void;
  onDelete: (message: ChatMessage) => Promise<boolean>;
  onReport: (message: ChatMessage) => void;
  onRetry: (message: ChatMessage) => void;
  onDiscard: (message: ChatMessage) => void;
  /** A failed message's text goes back into the composer (link, phone, word list). */
  onRewrite: (message: ChatMessage) => void;
}

const timeFormat = () => dateTimeFormat({ hour: "2-digit", minute: "2-digit" });

/** Failures the same text would meet again: the player writes it another way. */
const REWRITE = new Set(["link", "phone", "words", "invalid"]);

function StateMark({ message }: { message: ChatMessage }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  if (!message.mine) return null;
  const label =
    message.state === "sending"
      ? t("chat.state.sending")
      : message.state === "failed"
        ? t("chat.state.failed")
        : t("chat.state.sent");
  return (
    <span className="relative inline-flex size-3.5 items-center justify-center" title={label}>
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span key={message.state} className="absolute inline-flex" {...swap(reduce, 0.18)}>
          {message.state === "sending" ? (
            <Clock size={12} weight="bold" className="text-ash-400" aria-hidden />
          ) : message.state === "failed" ? (
            <WarningCircle size={13} weight="fill" className="text-flare-400" aria-hidden />
          ) : (
            <Check size={12} weight="bold" className="text-gold-300" aria-hidden />
          )}
        </motion.span>
      </AnimatePresence>
      <span className="sr-only">{label}</span>
    </span>
  );
}

function MessageRowView({
  message,
  text,
  sender,
  place,
  actions,
  nowMs,
}: {
  message: ChatMessage;
  /** The text shown (an edit on its way shows its new text). */
  text: string;
  sender: Pick<SocialProfile, "name" | "avatar" | "avatarUrl">;
  place: RowPlace;
  actions: RowActions;
  nowMs: number;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const mine = message.mine;
  const editable = message.state === "sent" && (message.editUntil ?? 0) > nowMs;
  const deletable = message.state === "sent" && (message.deleteUntil ?? 0) > nowMs;
  const failed = message.state === "failed";
  const time = timeFormat().format(new Date(message.at));

  const items: MenuItem[] = [
    {
      label: t("chat.actions.copy"),
      icon: <Copy size={16} />,
      onSelect: () => {
        void copyText(message.text).then((copied) => {
          if (copied) toast.success(t("chat.actions.copied"));
        });
      },
    },
  ];
  if (mine && editable) {
    items.push({
      label: t("chat.actions.edit"),
      icon: <PencilSimple size={16} />,
      onSelect: () => actions.onEdit(message),
    });
  }
  if (deletable) {
    items.push({
      label: t("chat.actions.delete"),
      icon: <Trash size={16} />,
      danger: true,
      onSelect: () => setAsking(true),
    });
  }
  if (!mine && message.id !== undefined) {
    items.push({
      label: t("chat.actions.report"),
      icon: <Flag size={16} />,
      onSelect: () => actions.onReport(message),
    });
  }

  const remove = async () => {
    setBusy(true);
    const done = await actions.onDelete(message);
    setBusy(false);
    if (!done) setAsking(false);
  };

  return (
    <ContextMenu items={items}>
      <div
        className={cn(
          "group/msg relative flex items-end gap-2 px-1",
          mine ? "flex-row-reverse" : "flex-row",
          place.last ? "pb-2.5" : "pb-0.5",
        )}
      >
        {mine ? null : (
          <span className="flex w-6 shrink-0 self-end">
            {place.last ? <SocialFace source={sender} size={24} /> : null}
          </span>
        )}
        <div
          className={cn("flex min-w-0 max-w-[78%] flex-col", mine ? "items-end" : "items-start")}
        >
          {place.crew && !mine && place.first ? (
            <span className="mb-1 ml-1 max-w-full truncate text-[12px] font-semibold text-ash-300">
              {sender.name}
            </span>
          ) : null}
          <div
            className={cn(
              "max-w-full rounded-[var(--radius-panel)] px-3 py-[7px] text-[13.5px] leading-[1.45] break-words whitespace-pre-wrap",
              "transition-[opacity,box-shadow,background-color] duration-200",
              mine
                ? "bg-[color-mix(in_oklab,var(--color-gold-500)_17%,var(--color-obsidian-800))] text-bone-50 ring-1 ring-gold-400/20"
                : "bg-obsidian-800/85 text-bone-100 ring-1 ring-white/7",
              message.state === "sending" && "opacity-70",
              failed && "ring-flare-400/45",
            )}
            data-selectable
          >
            <span className="sr-only">{mine ? "" : `${sender.name}: `}</span>
            {text}
          </div>
          {place.last || message.state !== "sent" || message.edited ? (
            <span
              className={cn(
                "mt-1 flex items-center gap-1.5 px-1 text-[11.5px] leading-none text-ash-400 tabular-nums",
                mine ? "flex-row-reverse" : "flex-row",
              )}
            >
              {mine ? <StateMark message={message} /> : null}
              {place.last || failed ? (
                <time dateTime={new Date(message.at).toISOString()}>{time}</time>
              ) : null}
              {message.edited ? <span>{t("chat.state.edited")}</span> : null}
            </span>
          ) : null}
          <AnimatePresence initial={false}>
            {failed ? (
              <motion.span
                key="failed"
                className="mt-1.5 flex flex-wrap items-center justify-end gap-x-2 gap-y-1 px-1"
                {...rise(reduce, 4, 0.2)}
              >
                <span className="text-[12px] text-flare-300">
                  {t(`chat.failure.${message.failure ?? "connection"}`)}
                </span>
                {REWRITE.has(message.failure ?? "") ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7"
                    onClick={() => actions.onRewrite(message)}
                  >
                    {t("chat.actions.edit")}
                  </Button>
                ) : (
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7"
                    icon={<ArrowClockwise size={13} />}
                    onClick={() => actions.onRetry(message)}
                  >
                    {t("chat.actions.retry")}
                  </Button>
                )}
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-7"
                  onClick={() => actions.onDiscard(message)}
                >
                  {t("chat.actions.discard")}
                </Button>
              </motion.span>
            ) : null}
          </AnimatePresence>
          <AnimatePresence initial={false}>
            {asking ? (
              <motion.div
                key="ask"
                className={cn(
                  "mt-1.5 flex flex-col gap-2 px-1",
                  mine ? "items-end" : "items-start",
                )}
                initial={{ opacity: 0, transform: reduce ? "none" : "translateY(-3px)" }}
                animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.2) }}
                exit={{ opacity: 0, transition: leave(0.14) }}
              >
                <p className="text-[12.5px] text-bone-100">{t("chat.actions.deleteAsk")}</p>
                <span className="flex gap-1.5">
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7"
                    onClick={() => setAsking(false)}
                  >
                    {t("chat.actions.cancel")}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    className="h-7"
                    disabled={busy}
                    onClick={() => void remove()}
                  >
                    {t("chat.actions.delete")}
                  </Button>
                </span>
              </motion.div>
            ) : null}
          </AnimatePresence>
        </div>
        <span
          className={cn(
            "flex shrink-0 self-center opacity-0 transition-opacity duration-150",
            "group-hover/msg:opacity-100 focus-within:opacity-100 has-[[data-state=open]]:opacity-100",
            message.state !== "sent" && "pointer-events-none invisible",
          )}
        >
          <Menu
            items={items}
            trigger={
              <IconButton
                label={t("chat.actions.menu")}
                icon={<DotsThree size={18} weight="bold" />}
                size="sm"
                tooltipSide="top"
              />
            }
          />
        </span>
      </div>
    </ContextMenu>
  );
}

export const MessageRow = memo(MessageRowView);
