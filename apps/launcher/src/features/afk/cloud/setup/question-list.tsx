// The settings as questions (SETUP-UX-L, the site's question-list.tsx in the launcher's looks): one
// quiet panel of rows, each a question with its item, its answer in plain words and "Değiştir". A
// row opens in place into its editor (the setup step's fields, then its own folded "Gelişmiş"): the
// editor fades in where it is (220 ms, 4 px) and the rows under it glide aside (`layout="position"`,
// 300 ms; a height is never animated); under reduced motion and in Performance mode the rows move at
// once and the editor only fades. Edits wait in the one draft and its change bar; a question with
// unsaved edits carries the small gold dot (design.md §5), one with a field to fix the flare mark.
import { CaretDown, WarningCircle } from "@phosphor-icons/react";
import { cn, easeOutExpo, SwapLabel, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { type ReactNode, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { OneGold } from "../../settings/controls";
import { ItemSlot } from "./item-slot";

const GLIDE = { layout: { duration: 0.3, ease: easeOutExpo } };

export function QuestionList({
  label,
  children,
  className,
}: {
  label: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <LayoutGroup>
      <section
        aria-label={label}
        className={cn(
          "rounded-[var(--radius-panel)] bg-white/[0.03] ring-1 ring-white/7 ring-inset",
          className,
        )}
      >
        {/* The screen's gold is its save: switches and quiet links here are bone (design.md §2). */}
        <OneGold>{children}</OneGold>
      </section>
    </LayoutGroup>
  );
}

export function QuestionRow({
  id,
  item,
  question,
  answer,
  extra,
  open,
  onToggle,
  dirty = false,
  alert = false,
  children,
}: {
  /** The row's name in the page (`question-<id>`): "Sınırlara git" scrolls to it. */
  id: string;
  /** The question's item (`minecraft:…`). */
  item: string;
  question: string;
  answer: ReactNode;
  /** Under the answer while it is folded (the spending meter). */
  extra?: ReactNode;
  open: boolean;
  onToggle: () => void;
  /** Unsaved edits here: the small gold dot. */
  dirty?: boolean;
  /** A field here must be fixed before saving: the flare mark. */
  alert?: boolean;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const body = useId();
  return (
    <motion.div
      layout={layout ? "position" : false}
      transition={GLIDE}
      id={`question-${id}`}
      data-question={id}
      className="scroll-mt-3 border-t border-white/6 first:border-t-0"
    >
      <button
        type="button"
        aria-expanded={open}
        aria-controls={body}
        onClick={onToggle}
        className="group flex w-full cursor-pointer items-start gap-3.5 rounded-[var(--radius-panel)] px-[18px] py-[14px] text-left outline-none transition-colors duration-150 hover:bg-white/[0.025] focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-inset"
      >
        <ItemSlot item={item} />
        <span className="flex min-w-0 flex-1 flex-col gap-[3px] pt-[2px]">
          <span className="flex items-center gap-2 text-[14.5px] leading-snug font-semibold text-bone-50">
            {question}
            {alert ? (
              <span className="flex shrink-0">
                <WarningCircle size={15} weight="fill" aria-hidden className="text-flare-400" />
                <span className="sr-only">{t("setup.marked")}</span>
              </span>
            ) : dirty ? (
              <span className="flex shrink-0 items-center">
                <span aria-hidden className="size-1.5 rounded-full bg-gold-400" />
                <span className="sr-only">{t("setup.unsaved")}</span>
              </span>
            ) : null}
          </span>
          <span className="text-[13px] leading-[1.45] text-pretty text-ash-300">{answer}</span>
          {open ? null : extra}
        </span>
        <span
          className={cn(
            "inline-flex shrink-0 items-center gap-1.5 pt-[3px] text-[12.5px] font-medium transition-colors duration-150",
            open ? "text-bone-100" : "text-ash-400 group-hover:text-bone-100",
          )}
        >
          <SwapLabel labels={[t("setup.change"), t("setup.close")]} active={open ? 1 : 0} />
          <CaretDown
            size={14}
            aria-hidden
            className={cn("transition-[rotate] duration-200 ease-out", open && "rotate-180")}
          />
        </span>
      </button>
      <AnimatePresence initial={false}>
        {open ? (
          <motion.div
            key="editor"
            id={body}
            initial={{ opacity: 0, transform: layout ? "translateY(4px)" : "none" }}
            animate={{
              opacity: 1,
              transform: "translateY(0px)",
              transition: { duration: 0.22, ease: easeOutExpo },
            }}
            exit={{ opacity: 0, transition: { duration: 0.15 } }}
            className="flex flex-col pr-[18px] pb-[18px] pl-[72px]"
          >
            {children}
          </motion.div>
        ) : null}
      </AnimatePresence>
    </motion.div>
  );
}

/**
 * A question's rarely needed fields, folded under "Gelişmiş" with what they are (`summary`, read
 * while folded). `open` holds it open from outside ("Sınırlara git" to a field in it).
 */
export function Advanced({
  summary,
  open: forced,
  children,
}: {
  summary: string;
  open?: boolean;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const body = useId();
  const [open, setOpen] = useState(forced === true);
  // Opened from outside it stays open after the landing (the caller clears its target then).
  useEffect(() => {
    if (forced) setOpen(true);
  }, [forced]);
  return (
    <motion.div
      layout={layout ? "position" : false}
      transition={GLIDE}
      className="mt-2 border-t border-white/6 pt-3"
    >
      <button
        type="button"
        aria-expanded={open}
        aria-controls={body}
        onClick={() => setOpen(!open)}
        className="group flex max-w-full cursor-pointer items-center gap-2 rounded-sm py-0.5 text-left text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <span className="shrink-0 font-semibold text-ash-300 transition-colors duration-150 group-hover:text-bone-100">
          {t("setup.advanced")}
        </span>
        <CaretDown
          size={13}
          aria-hidden
          className={cn(
            "shrink-0 text-ash-400 transition-[rotate] duration-200 ease-out",
            open && "rotate-180",
          )}
        />
        {open ? null : <span className="min-w-0 truncate text-ash-400">{summary}</span>}
      </button>
      <AnimatePresence initial={false}>
        {open ? (
          <motion.div
            key="advanced"
            id={body}
            initial={{ opacity: 0, transform: layout ? "translateY(4px)" : "none" }}
            animate={{
              opacity: 1,
              transform: "translateY(0px)",
              transition: { duration: 0.22, ease: easeOutExpo },
            }}
            exit={{ opacity: 0, transition: { duration: 0.15 } }}
            className="flex flex-col pt-1"
          >
            {children}
          </motion.div>
        ) : null}
      </AnimatePresence>
    </motion.div>
  );
}

/** A label over its control (selects, segmented choices, the chat preview). */
export function Labeled({
  label,
  id,
  help,
  children,
  className,
}: {
  label: string;
  id?: string;
  help?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col gap-1.5 py-2.5", className)}>
      <span id={id} className="text-[13px] font-medium text-bone-100">
        {label}
      </span>
      {children}
      {help ? <p className="text-[12px] text-pretty text-ash-400">{help}</p> : null}
    </div>
  );
}
