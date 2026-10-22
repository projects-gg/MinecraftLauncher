// Reactions under a bubble (CHAT2 §7): chamfered 22 px chips (an action: the two-shape rule), the
// player's own with a gold edge, the count in tabular figures that rolls when it changes; a new
// chip springs in once (a fade under reduced motion). A press opens who reacted ("Sen" first, each
// with their own menu) and one button: "Ben de 👍", or "Tepkimi kaldır" on the player's own. The
// quick row (the toolbar's 😊+, the right click's top row) offers six emoji and "+", the whole
// picker; a pick lands on its chip with a small pop.
import { DotsThree, Plus } from "@phosphor-icons/react";
import { Button, cn, easeOutExpo, IconButton, Menu, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { Popover } from "radix-ui";
import { memo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { EmojiPickerButton } from "@/features/emoji";
import type { ChatReaction, SocialProfile } from "@/lib/api/social-types";
import { PersonContextMenu, type PersonPlace, usePersonItems } from "../person-menu";
import { SocialFace } from "../social-face";
import { REACTIONS_PER_PERSON, reactors, reactVerdict } from "./reactions";
import { SenderName } from "./sender-name";

type People = (id: string) => Pick<SocialProfile, "name" | "avatar" | "avatarUrl">;

/** A spring in for a new chip, the roll of a changing count. */
const CHIP_IN = { duration: 0.16, ease: easeOutExpo };

function Count({ value }: { value: number }) {
  const { reduce } = useMotionPrefs();
  return (
    <span className="relative inline-flex h-[14px] min-w-[7px] justify-center overflow-hidden tabular-nums">
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span
          key={value}
          className="inline-block leading-[14px]"
          initial={{ opacity: 0, transform: reduce ? "none" : "translateY(70%)" }}
          animate={{ opacity: 1, transform: "translateY(0%)", transition: CHIP_IN }}
          exit={{
            opacity: 0,
            transform: reduce ? "none" : "translateY(-70%)",
            transition: { duration: 0.12, ease: easeOutExpo },
          }}
        >
          {value}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

/** The message reactions are on, for "Rapor et" from the list (the reaction is the evidence). */
export interface ReactionOrigin {
  conv: string;
  id: number;
}

/** One person who reacted: their own menu on "…" and on a right click (FIX2-D's person menu). */
function ReactorRow({
  id,
  person,
  self,
  place,
}: {
  id: string;
  person: ReturnType<People>;
  self: boolean;
  place?: PersonPlace;
}) {
  const { t } = useTranslation();
  const ref = { id, ...person };
  const items = usePersonItems(ref, place);
  const row = (
    <span className="group/reactor flex h-9 items-center gap-2.5 rounded-md pr-0.5 pl-1.5 text-[13px] text-bone-100">
      <SocialFace source={person} size={24} />
      <SenderName name={self ? t("chat2.reactions.you") : person.name} className="flex-1" />
      {self ? null : (
        <Menu
          align="end"
          items={items}
          trigger={
            <IconButton
              label={t("friendMenus.options", { name: person.name })}
              size="sm"
              icon={<DotsThree size={16} weight="bold" />}
              className="opacity-0 transition-opacity duration-150 group-hover/reactor:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
            />
          }
        />
      )}
    </span>
  );
  return self ? (
    row
  ) : (
    <PersonContextMenu person={ref} place={place}>
      {row}
    </PersonContextMenu>
  );
}

/** Who reacted with one emoji, and the one thing to do about it. */
function ReactorsPanel({
  reaction,
  mineCount,
  me,
  people,
  personPlace,
  origin,
  reactable,
  onToggle,
}: {
  reaction: ChatReaction;
  /** How many kinds the player gave this message. */
  mineCount: number;
  me: string | undefined;
  people: People;
  personPlace?: PersonPlace;
  origin?: ReactionOrigin;
  /** The message takes reactions now: "Ben de" / "Tepkimi kaldır" show. */
  reactable: boolean;
  onToggle: (emoji: string, on: boolean) => void;
}) {
  const { t } = useTranslation();
  const list = reactors(reaction, me);
  const full = !reaction.mine && mineCount >= REACTIONS_PER_PERSON;
  return (
    <div className="flex w-[232px] flex-col">
      <div className="flex items-center gap-2.5 px-3 pt-3 pb-2">
        <span aria-hidden className="text-[22px] leading-none">
          {reaction.e}
        </span>
        <span className="text-[13px] font-semibold text-bone-50">
          {t("chat2.reactions.people", { count: reaction.by.length })}
        </span>
      </div>
      <ul
        aria-label={t("chat2.reactions.list")}
        className="max-h-[188px] overflow-y-auto overscroll-contain px-1.5 pb-1.5"
      >
        {list.map((id) => {
          const self = id === me;
          const place: PersonPlace | undefined = origin
            ? { ...personPlace, reaction: { conv: origin.conv, id: origin.id, e: reaction.e } }
            : personPlace;
          return (
            <li key={id}>
              <ReactorRow id={id} person={people(id)} self={self} place={place} />
            </li>
          );
        })}
      </ul>
      {reactable ? (
        <div className="border-t border-white/6 p-2">
          <Button
            size="sm"
            variant={reaction.mine ? "ghost" : "secondary"}
            className="w-full"
            disabled={full}
            data-reactors-action=""
            onClick={() => onToggle(reaction.e, !reaction.mine)}
          >
            {reaction.mine
              ? t("chat2.reactions.remove")
              : t("chat2.reactions.meToo", { emoji: reaction.e })}
          </Button>
          {full ? (
            <p className="mt-1.5 px-1 text-[12px] text-ash-400">{t("chat2.reactions.full")}</p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function Chip({
  reaction,
  fresh,
  mineCount,
  me,
  people,
  personPlace,
  origin,
  reactable,
  onToggle,
}: {
  reaction: ChatReaction;
  /** It came while the conversation is open: it springs in. */
  fresh: boolean;
  mineCount: number;
  me: string | undefined;
  people: People;
  personPlace?: PersonPlace;
  origin?: ReactionOrigin;
  reactable: boolean;
  onToggle: (emoji: string, on: boolean) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const [open, setOpen] = useState(false);
  const fromKeys = useRef(false);
  const count = reaction.by.length;
  const label = !reaction.mine
    ? t("chat2.reactions.chip", { emoji: reaction.e, count })
    : count === 1
      ? t("chat2.reactions.chipMineAlone", { emoji: reaction.e })
      : t("chat2.reactions.chipMine", { emoji: reaction.e, count: count - 1 });
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <motion.button
          type="button"
          aria-label={label}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") fromKeys.current = true;
          }}
          onPointerDown={() => {
            fromKeys.current = false;
          }}
          className={cn(
            "group/chip chamfer relative flex h-[22px] cursor-pointer items-center gap-1 px-[7px] outline-none [--cut:5px]",
            "transition-[background-color,transform,scale] duration-150 active:scale-[0.94]",
            reaction.mine
              ? "bg-[color-mix(in_oklab,var(--color-gold-500)_20%,var(--color-obsidian-800))] hover:bg-[color-mix(in_oklab,var(--color-gold-500)_28%,var(--color-obsidian-800))]"
              : "bg-obsidian-800 hover:bg-obsidian-700",
          )}
          initial={fresh ? { opacity: 0, transform: reduce ? "none" : "scale(0.6)" } : false}
          animate={{
            opacity: 1,
            transform: "scale(1)",
            transition: reduce
              ? { duration: 0.16 }
              : { type: "spring", stiffness: 520, damping: 26, mass: 0.7 },
          }}
          exit={{ opacity: 0, transform: reduce ? "none" : "scale(0.7)", transition: CHIP_IN }}
        >
          <span
            aria-hidden
            className={cn(
              "chamfer pointer-events-none absolute inset-0 [--cut:5px]",
              reaction.mine
                ? "shadow-[inset_0_0_0_1px_rgb(240_184_73/0.55)]"
                : "shadow-[inset_0_0_0_1px_rgb(255_255_255/0.07)]",
              // The keyboard's ring follows the chamfer.
              "group-focus-visible/chip:shadow-[inset_0_0_0_2px_var(--color-gold-300)]",
            )}
          />
          <span aria-hidden className="relative text-[13px] leading-none">
            {reaction.e}
          </span>
          <span
            aria-hidden
            className={cn(
              "relative text-[12px] font-semibold",
              reaction.mine ? "text-gold-200" : "text-ash-300",
            )}
          >
            <Count value={count} />
          </span>
        </motion.button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="start"
          sideOffset={6}
          collisionPadding={12}
          onOpenAutoFocus={(event) => {
            // From the keyboard the focus goes straight to the button: Enter, Enter is "ben de".
            if (!fromKeys.current) {
              event.preventDefault();
              return;
            }
            event.preventDefault();
            (event.currentTarget as HTMLElement | null)
              ?.querySelector<HTMLButtonElement>("[data-reactors-action]")
              ?.focus();
          }}
          className="pop-motion z-50 overflow-hidden rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] outline-none"
        >
          <ReactorsPanel
            reaction={reaction}
            mineCount={mineCount}
            me={me}
            people={people}
            personPlace={personPlace}
            origin={origin}
            reactable={reactable}
            onToggle={(emoji, on) => {
              setOpen(false);
              onToggle(emoji, on);
            }}
          />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

/** The chips of a message, in the order the kinds were first given. */
export const ReactionChips = memo(function ReactionChips({
  reactions,
  mine,
  me,
  people,
  personPlace,
  origin,
  reactable,
  fresh,
  onToggle,
}: {
  reactions: readonly ChatReaction[];
  /** The player's own message: the chips line up on the right. */
  mine: boolean;
  me: string | undefined;
  people: People;
  personPlace?: PersonPlace;
  /** The message they are on ("Rapor et" from the list carries the reaction). */
  origin?: ReactionOrigin;
  /** The message takes reactions now (else the chips only show who reacted). */
  reactable: boolean;
  /** Chips that come now spring in (the message is open on screen). */
  fresh: boolean;
  onToggle: (emoji: string, on: boolean) => void;
}) {
  const mineCount = reactions.filter((r) => r.mine).length;
  if (!reactions.length) return null;
  return (
    <div className={cn("mt-1 flex flex-wrap gap-1 px-0.5", mine ? "justify-end" : "justify-start")}>
      <AnimatePresence initial={false}>
        {reactions.map((reaction) => (
          <Chip
            key={reaction.e}
            reaction={reaction}
            fresh={fresh}
            mineCount={mineCount}
            me={me}
            people={people}
            personPlace={personPlace}
            origin={origin}
            reactable={reactable}
            onToggle={onToggle}
          />
        ))}
      </AnimatePresence>
    </div>
  );
});

/** The quick row: six emoji and "+" (the whole picker, which closes on a pick). */
export function QuickReactions({
  emoji,
  given,
  onPick,
  onMore,
}: {
  emoji: readonly string[];
  /** The ones the player gave (a gold tint). */
  given: (emoji: string) => boolean;
  onPick: (emoji: string) => void;
  /** "+" picked an emoji from the whole picker. */
  onMore: (emoji: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      role="toolbar"
      aria-label={t("chat2.toolbar.react")}
      className="flex items-center gap-0.5 p-1"
    >
      {emoji.map((e) => (
        <button
          key={e}
          type="button"
          aria-label={t("chat2.reactions.give", { emoji: e })}
          onClick={() => onPick(e)}
          className={cn(
            "chamfer flex size-[34px] cursor-pointer items-center justify-center outline-none [--cut:6px]",
            "transition-[background-color,transform,scale] duration-150 hover:bg-white/10 focus-visible:bg-white/12 active:scale-[0.9]",
            given(e) && "bg-gold-500/16 hover:bg-gold-500/24",
          )}
        >
          <span aria-hidden className="text-[20px] leading-none">
            {e}
          </span>
        </button>
      ))}
      <EmojiPickerButton
        onEmoji={onMore}
        closeOnEmoji
        label={t("chat2.reactions.more")}
        icon={<Plus size={16} weight="bold" aria-hidden />}
        side="top"
        align="center"
      />
    </div>
  );
}

/** The verdict of a press on an emoji for a message's reactions (see reactions.ts). */
export { reactVerdict };
