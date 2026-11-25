// INST-ADD: the library's way to an instance of one's own while there is none yet: a quiet card in
// "Kurulumlarım" with a plus where an instance's icon goes, "Yeni Kurulum" and one line on what it
// makes. The whole card opens "Yeni Kurulum", as an instance card opens its instance (the same
// stretched title). Quiet like the official pack's "Kurulu değil" card (LIB1): no glass, no lift,
// no dashed frame (dashed frames mean "drop files here").
import { Plus } from "@phosphor-icons/react";
import { easeOutExpo, useMotionPrefs } from "@pl/ui";
import { motion } from "motion/react";
import { type Ref, useId } from "react";
import { useTranslation } from "react-i18next";

/** Where a card rests (the instance cards' own entrance and exit move from and to it). */
const REST = "translateY(0px) scale(1)";

export function CreateOwnCard({
  index,
  onCreate,
  ref,
}: {
  /** The card's place in the library, for the cards' staggered entrance. */
  index: number;
  onCreate: () => void;
  /** Set by the list's AnimatePresence (popLayout measures the leaving card). */
  ref?: Ref<HTMLElement>;
}) {
  const { t } = useTranslation();
  const { layout, reduce } = useMotionPrefs();
  const bodyId = useId();
  return (
    <motion.article
      ref={ref}
      layout={layout ? "position" : false}
      initial={{ opacity: 0, transform: reduce ? REST : "translateY(12px) scale(1)" }}
      animate={{ opacity: 1, transform: REST }}
      exit={{
        opacity: 0,
        transform: reduce ? REST : "translateY(0px) scale(0.97)",
        transition: { duration: 0.18, ease: easeOutExpo },
      }}
      transition={{ duration: 0.4, delay: Math.min(index, 8) * 0.035, ease: easeOutExpo }}
      className="relative isolate flex flex-col rounded-[var(--radius-panel)] bg-obsidian-900/35 p-3.5 ring-1 ring-white/8 transition-[box-shadow,background-color,scale] duration-200 ease-out ring-inset hover:bg-obsidian-900/50 hover:ring-white/14 has-[button:active]:scale-[0.985]"
    >
      {/* The instance icon's tile, empty but for the plus: a place for one. */}
      <span
        aria-hidden
        className="chamfer flex size-[48px] shrink-0 items-center justify-center bg-white/10 [--cut:7px]"
      >
        <Plus size={22} weight="bold" className="text-bone-100" />
      </span>
      <button
        type="button"
        onClick={onCreate}
        aria-label={t("instAdd.card.actionLabel")}
        aria-describedby={bodyId}
        className="mt-3 block cursor-pointer rounded-sm text-left outline-none after:absolute after:inset-0 after:rounded-[var(--radius-panel)] focus-visible:after:ring-2 focus-visible:after:ring-gold-300"
      >
        <span className="block truncate font-display text-[15px] font-semibold text-bone-50">
          {t("instAdd.card.title")}
        </span>
      </button>
      <p id={bodyId} className="mt-0.5 text-balance text-[12.5px] text-ash-300">
        {t("instAdd.card.body")}
      </p>
    </motion.article>
  );
}
