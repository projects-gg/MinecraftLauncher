// Replies (CHAT2 §6). Over the composer, a thin strip: "Yanıtlanıyor: Ayşe" with one line of the
// message and ✕ ("Yanıtı kaldır"; Esc does the same). Over a reply's bubble, a short quote: the
// name, two lines at most, a sticker as a small picture. A press scrolls to the message and
// lights it once; "Önceki bir mesaj" when this computer does not hold it (nothing to press),
// "Mesaj silindi" when it was deleted, no name for someone the player blocked.
import { ArrowBendUpLeft, X } from "@phosphor-icons/react";
import { findSticker } from "@pl/stickers";
import { cn, IconButton } from "@pl/ui";
import { motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { Sticker } from "@/features/emoji";
import type { ChatMessage, ChatReply } from "@/lib/api/social-types";
import { enter, leave, useSocialMotion } from "../motion";

/** What a quote shows of a message (the one answered, as this computer holds it). */
export interface Quoted {
  /** The writer's name, or none (the message is not held, or its writer is no longer seen). */
  name: string | null;
  text?: string;
  sticker?: string;
  deleted: boolean;
  missing: boolean;
  /** Its key here, when it can be scrolled to. */
  key?: string;
}

/** The quote of a reply: the live message from this conversation's page when it holds it, else
 * the backend's copy (it may be older than the page). */
export function quotedOf(
  reply: ChatReply,
  shown: ReadonlyMap<number, ChatMessage>,
  nameOf: (id: string) => string | null,
): Quoted {
  const live = shown.get(reply.id);
  if (live) {
    return {
      name: nameOf(live.from),
      text: live.text,
      sticker: live.sticker,
      deleted: Boolean(live.deleted),
      missing: false,
      key: live.key,
    };
  }
  const name = reply.missing ? null : nameOf(reply.from);
  return {
    name,
    text: reply.text,
    sticker: reply.sticker,
    deleted: reply.deleted,
    missing: reply.missing || name === null,
    key: reply.missing ? undefined : reply.key,
  };
}

/** The quote's words: one line in the strip, two in a bubble's quote. */
function QuoteWords({ quoted, lines }: { quoted: Quoted; lines: 1 | 2 }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language.startsWith("en") ? "en" : "tr";
  const sticker = quoted.sticker ? findSticker(quoted.sticker) : undefined;
  if (quoted.missing) return <span className="text-ash-400">{t("chat2.reply.earlier")}</span>;
  if (quoted.deleted) return <span className="text-ash-400">{t("chat2.reply.deleted")}</span>;
  if (quoted.sticker) {
    return (
      <span className="flex items-center gap-1.5 text-ash-300">
        {sticker ? <Sticker sticker={sticker} size={24} moveOnHover={false} /> : null}
        {sticker ? sticker.name[lang] : t("chat2.reply.sticker")}
      </span>
    );
  }
  return (
    <span
      className={cn(
        "block break-words text-ash-300",
        lines === 1 ? "truncate" : "line-clamp-2 whitespace-pre-wrap",
      )}
    >
      {quoted.text}
    </span>
  );
}

/**
 * A reply's quote: inside its bubble, at the top (`inside`; over the bubble it read as a message of
 * its own, CHAT-PROD leftovers), or over a sticker or a few big emoji, which have no bubble.
 */
export function QuoteBlock({
  quoted,
  mine,
  inside = false,
  onJump,
}: {
  quoted: Quoted;
  mine: boolean;
  inside?: boolean;
  onJump: (key: string) => void;
}) {
  const { t } = useTranslation();
  const pressable = Boolean(quoted.key) && !quoted.missing && !quoted.deleted;
  // Screen readers hear what it is, then the quote ("Yanıtlanan mesaj: Ayşe, …").
  const body = (
    <>
      <span className="sr-only">{t(pressable ? "chat2.reply.jump" : "chat2.reply.quoted")}: </span>
      <span
        aria-hidden
        className="absolute top-1.5 bottom-1.5 left-1.5 w-[2px] rounded-full bg-bone-100/30"
      />
      {quoted.name && !quoted.missing ? (
        <span className="block truncate text-[12px] font-semibold text-ash-300">{quoted.name}</span>
      ) : null}
      <span className="block text-[12.5px] leading-[1.4]">
        <QuoteWords quoted={quoted} lines={2} />
      </span>
    </>
  );
  // Inside, a darker well in the bubble's own fill (either side's), spanning the bubble's width.
  const className = cn(
    "relative block max-w-full min-w-[96px] py-1.5 pr-2.5 pl-4 text-left ring-1 ring-inset",
    inside
      ? "mx-1.5 mt-1.5 rounded-[7px] bg-obsidian-950/35 ring-white/5"
      : cn("mb-1 rounded-[8px] bg-obsidian-800/85 ring-white/6", mine ? "self-end" : "self-start"),
  );
  if (!pressable) {
    return <span className={className}>{body}</span>;
  }
  return (
    <button
      type="button"
      onClick={() => quoted.key && onJump(quoted.key)}
      className={cn(
        className,
        "cursor-pointer outline-none transition-[background-color,transform,scale] duration-150 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.985]",
        inside ? "hover:bg-obsidian-950/50" : "hover:bg-obsidian-700/85",
      )}
    >
      {body}
    </button>
  );
}

/** The strip over the composer while the next message answers one. */
export function ReplyStrip({ quoted, onCancel }: { quoted: Quoted; onCancel: () => void }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  return (
    <motion.div
      className="mb-1.5 flex items-center gap-2 overflow-hidden rounded-[8px] bg-obsidian-800/70 py-1.5 pr-1 pl-2.5 ring-1 ring-white/6 ring-inset"
      initial={{ opacity: 0, transform: reduce ? "none" : "translateY(3px)" }}
      animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.2) }}
      exit={{ opacity: 0, transition: leave(0.14) }}
    >
      <ArrowBendUpLeft size={14} weight="bold" className="shrink-0 text-gold-300" aria-hidden />
      <span className="min-w-0 flex-1 text-[12.5px] leading-[1.35]">
        <span className="block truncate font-semibold text-bone-100">
          {t("chat2.reply.replying", { name: quoted.name ?? t("chat.list.someone") })}
        </span>
        <span className="block truncate">
          <QuoteWords quoted={quoted} lines={1} />
        </span>
      </span>
      <IconButton
        label={t("chat2.reply.cancel")}
        icon={<X size={14} />}
        size="sm"
        tooltipSide="top"
        onClick={onCancel}
      />
    </motion.div>
  );
}
