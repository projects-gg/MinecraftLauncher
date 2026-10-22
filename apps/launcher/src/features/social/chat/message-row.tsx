// One chat message: a bubble (the player's own on the right, gold-tinted; others' on the left with
// their picture at the end of a group and, in crew chat, their name at its start), "düzenlendi",
// the time and where it stands (on its way, sent, not sent with "Tekrar dene"). Its menu ("…" on
// hover or focus, a right click, Shift+F10): copy, edit (15 minutes), "Herkesten sil" (asked in
// place, one sentence), "Rapor et" (others' messages).
//
// A message deleted for everyone stays in its place as a tombstone (FIX2): the same bubble, quiet
// and empty-handed, saying who deleted it ("Bu mesajı sildin", "Bu mesaj silindi", "Ekip lideri
// sildi"…). No menu, no copy, edit, report or reply. The bubble morphs into it: the text fades out,
// the bubble glides to its new size (a transform-based layout animation, corners and ring kept by
// the scale correction) while its fill cross-fades to the tombstone's, and the words fade in.
// Reduced motion and Performance mode: the text fades out, then the words fade in.
import {
  ArrowClockwise,
  Check,
  Clock,
  Copy,
  DotsThree,
  Flag,
  PencilSimple,
  Prohibit,
  Trash,
  WarningCircle,
} from "@phosphor-icons/react";
import {
  Button,
  ContextMenu,
  cn,
  easeOutExpo,
  IconButton,
  Menu,
  type MenuItem,
  toast,
} from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { memo, type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ChatMessage, SocialProfile } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { copyText } from "../actions";
import { enter, leave, rise, swap, useSocialMotion } from "../motion";
import { PersonContextMenu, type PersonPlace } from "../person-menu";
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

/** The bubble's corners (the panels' radius, `--radius-panel`), kept through its morph. */
const RADIUS = 10;
/** The bubble gliding to its new size: the list's entrance curve, a little longer. */
const MORPH = { duration: 0.36, ease: easeOutExpo };

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

/**
 * The bubble: its fill and ring sit on two layers behind the words, the message's and the
 * tombstone's, which cross-fade; the bubble's size change is a layout animation (transform), its
 * corners kept by the scale correction (the radius is a style for that).
 */
function Bubble({
  mine,
  failed,
  sending,
  gone,
  sizeKey,
  children,
}: {
  mine: boolean;
  failed: boolean;
  sending: boolean;
  gone: boolean;
  /** Changes whenever the words inside change (the bubble measures itself only then). */
  sizeKey: string;
  children: ReactNode;
}) {
  const { layout } = useSocialMotion();
  // One transition for both: the layout morph, and the fill's own fade.
  const layer = (shown: boolean, delay = 0) => ({
    initial: false as const,
    animate: { opacity: shown ? 1 : 0 },
    transition: {
      layout: MORPH,
      opacity: shown ? { duration: 0.26, ease: easeOutExpo, delay } : { duration: 0.16 },
    },
  });
  return (
    <motion.div
      layout={layout}
      layoutDependency={sizeKey}
      transition={{ layout: MORPH }}
      style={{ borderRadius: RADIUS }}
      className={cn(
        "relative isolate max-w-full overflow-hidden text-[13.5px] leading-[1.45] transition-opacity duration-200",
        sending && "opacity-70",
      )}
    >
      <motion.span
        aria-hidden
        layout={layout}
        layoutDependency={sizeKey}
        style={{ borderRadius: RADIUS }}
        className={cn(
          "absolute inset-0 -z-10 ring-1 ring-inset transition-[box-shadow] duration-200",
          mine
            ? "bg-[color-mix(in_oklab,var(--color-gold-500)_17%,var(--color-obsidian-800))] ring-gold-400/20"
            : "bg-obsidian-800/85 ring-white/7",
          failed && "ring-flare-400/45",
        )}
        {...layer(!gone)}
      />
      <motion.span
        aria-hidden
        layout={layout}
        layoutDependency={sizeKey}
        style={{ borderRadius: RADIUS }}
        className="absolute inset-0 -z-10 bg-obsidian-900/45 ring-1 ring-inset ring-white/10"
        {...layer(gone, 0.06)}
      />
      {children}
    </motion.div>
  );
}

function MessageRowView({
  message,
  text,
  tombstone,
  sender,
  place,
  personPlace,
  actions,
  nowMs,
}: {
  message: ChatMessage;
  /** The text shown (an edit on its way shows its new text). */
  text: string;
  /** Deleted for everyone (or by the player, waiting for the answer): what its tombstone says. */
  tombstone: string | null;
  sender: Pick<SocialProfile, "name" | "avatar" | "avatarUrl">;
  place: RowPlace;
  /** Where the sender's own menu opens from (their face and name, a right click; FIX2). */
  personPlace?: PersonPlace;
  actions: RowActions;
  nowMs: number;
}) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const mine = message.mine;
  const gone = tombstone !== null;
  const editable = !gone && message.state === "sent" && (message.editUntil ?? 0) > nowMs;
  const deletable = !gone && message.state === "sent" && (message.deleteUntil ?? 0) > nowMs;
  const failed = message.state === "failed";
  const time = timeFormat().format(new Date(message.at));

  const items: MenuItem[] = gone
    ? []
    : [
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
  if (!gone && !mine && message.id !== undefined) {
    items.push({
      label: t("chat.actions.report"),
      icon: <Flag size={16} />,
      onSelect: () => actions.onReport(message),
    });
  }

  const remove = async () => {
    setBusy(true);
    // The question goes as the tombstone comes; it comes back if the service refused.
    setAsking(false);
    const done = await actions.onDelete(message);
    setBusy(false);
    if (!done) setAsking(false);
  };

  // The words: the text fades out, the tombstone's line fades in (one after the other while the
  // bubble does not glide).
  const words = (
    <AnimatePresence mode={layout ? "popLayout" : "wait"} initial={false}>
      {gone ? (
        <motion.span
          key="gone"
          layout={layout ? "position" : false}
          className="relative flex items-center gap-1.5 px-3 py-[7px] whitespace-nowrap text-ash-300"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: { ...enter(0.22), delay: layout ? 0.12 : 0 } }}
          exit={{ opacity: 0, transition: leave(0.12) }}
        >
          <Prohibit size={14} weight="bold" className="shrink-0 text-ash-400" aria-hidden />
          <span className="sr-only">{mine ? "" : `${sender.name}: `}</span>
          {tombstone}
        </motion.span>
      ) : (
        <motion.span
          key="text"
          layout={layout ? "position" : false}
          className="relative block px-3 py-[7px] break-words whitespace-pre-wrap text-bone-50 data-[theirs]:text-bone-100"
          data-theirs={mine ? undefined : true}
          data-selectable
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.2) }}
          exit={{ opacity: 0, transition: { duration: 0.1, ease: easeOutExpo } }}
        >
          <span className="sr-only">{mine ? "" : `${sender.name}: `}</span>
          {text}
        </motion.span>
      )}
    </AnimatePresence>
  );

  return (
    <ContextMenu items={items} disabled={gone}>
      <div
        className={cn(
          "group/msg relative flex items-end gap-2 px-1",
          mine ? "flex-row-reverse" : "flex-row",
          place.last ? "pb-2.5" : "pb-0.5",
        )}
      >
        {mine ? null : (
          // The face sits by the group's last bubble, over the line with its time; a right click
          // on it (or on the name) opens the sender's own menu.
          <span className={cn("flex w-6 shrink-0 self-end", place.last && "mb-[15px]")}>
            {place.last ? (
              <PersonContextMenu person={{ id: message.from, ...sender }} place={personPlace}>
                <span className="flex">
                  <SocialFace source={sender} size={24} />
                </span>
              </PersonContextMenu>
            ) : null}
          </span>
        )}
        <div
          className={cn("flex min-w-0 max-w-[78%] flex-col", mine ? "items-end" : "items-start")}
        >
          {place.crew && !mine && place.first ? (
            <PersonContextMenu person={{ id: message.from, ...sender }} place={personPlace}>
              <span className="mb-1 ml-1 max-w-full truncate text-[12px] font-semibold text-ash-300">
                {sender.name}
              </span>
            </PersonContextMenu>
          ) : null}
          <Bubble
            mine={mine}
            failed={failed}
            sending={message.state === "sending"}
            gone={gone}
            sizeKey={gone ? `gone:${tombstone}` : `text:${text}`}
          >
            {words}
          </Bubble>
          {place.last || (!gone && (message.state !== "sent" || message.edited)) ? (
            <span
              className={cn(
                "mt-1 flex items-center gap-1.5 px-1 text-[11.5px] leading-none text-ash-400 tabular-nums",
                mine ? "flex-row-reverse" : "flex-row",
              )}
            >
              {mine && !gone ? <StateMark message={message} /> : null}
              {place.last || failed ? (
                <time dateTime={new Date(message.at).toISOString()}>{time}</time>
              ) : null}
              {message.edited && !gone ? <span>{t("chat.state.edited")}</span> : null}
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
            {asking && !gone ? (
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
            // A tombstone keeps the place of the "…" (nothing moves), offers nothing.
            (message.state !== "sent" || gone) && "pointer-events-none invisible",
          )}
        >
          {gone ? (
            <span aria-hidden className="size-7" />
          ) : (
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
          )}
        </span>
      </div>
    </ContextMenu>
  );
}

export const MessageRow = memo(MessageRowView);
