// The Modlar tab before the first download: the pack's mods that come when the game starts,
// under the overview's words, in the order the tab lists files (incoming-mods.ts). They are not
// files yet, so they have no switch or remove button; "Önerilen" opens the mod center, where
// they are chosen.
import { DownloadSimple } from "@phosphor-icons/react";
import { Chip, easeOutExpo, Skeleton, useMotionPrefs } from "@pl/ui";
import { motion } from "motion/react";
import { useId, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { ProjectIcon } from "@/features/content/parts";
import type { ModCenterItem } from "@/lib/api";
import { formatBytes } from "@/lib/format";

/** "Önerilen": a recommended mod of the pack; opens the mod center, where it is chosen. */
export function PackItemChip({ onClick }: { onClick: () => void }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={t("modCenter.fromMods.open")}
      className="group/pack shrink-0 cursor-pointer transition-transform duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97]"
    >
      <Chip className="transition-colors duration-150 group-hover/pack:bg-white/15">
        {t("modCenter.fromMods.chip")}
      </Chip>
    </button>
  );
}

/** Shaped like the list below: its heading, then rows (icon, two lines). */
export function IncomingModsSkeleton() {
  return (
    <div aria-busy className="pt-0.5">
      <Skeleton className="mx-3 mb-2.5 h-4 w-52" />
      {[0, 1, 2].map((row) => (
        <div key={row} className="flex items-center gap-3.5 px-3 py-2.5">
          <Skeleton className="size-10 shrink-0" />
          <div className="flex-1 space-y-2">
            <Skeleton className="h-3.5 w-36" />
            <Skeleton className="h-3 w-60" />
          </div>
        </div>
      ))}
    </div>
  );
}

export function IncomingModsList({
  items,
  fadeIn,
  onOpenModCenter,
  className,
}: {
  items: readonly ModCenterItem[];
  /** It comes after the mod center answered: fade in where it stands (opacity only). */
  fadeIn: boolean;
  onOpenModCenter?: () => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const headingId = useId();
  const sorted = useMemo(
    () => [...items].sort((a, b) => a.title.localeCompare(b.title, "tr")),
    [items],
  );
  return (
    // Under the player's own mods it glides when rows above it come and go (Performance mode:
    // it moves at once).
    <motion.section
      aria-labelledby={headingId}
      className={className}
      layout={layout ? "position" : false}
      transition={{ layout: { duration: 0.3, ease: easeOutExpo } }}
      initial={fadeIn ? { opacity: 0 } : false}
      animate={{ opacity: 1, transition: { duration: 0.2, ease: "easeOut" } }}
    >
      <h3
        id={headingId}
        className="mb-1 flex items-center gap-2 px-3 text-[13.5px] font-semibold text-bone-50"
      >
        <DownloadSimple size={16} weight="duotone" className="shrink-0 text-gold-300" aria-hidden />
        {t("overview.incoming.title")}
      </h3>
      <ul className="space-y-0.5">
        {sorted.map((item) => (
          <li
            key={item.id}
            className="flex items-center gap-3.5 rounded-[var(--radius-panel)] px-3 py-2.5"
          >
            <ProjectIcon src={item.iconUrl} size={40} className="[--cut:6px]" />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="truncate text-[14px] font-semibold text-bone-50">
                  {item.title}
                </span>
                {!item.library && onOpenModCenter ? (
                  <PackItemChip onClick={onOpenModCenter} />
                ) : null}
              </div>
              <p className="truncate text-[12.5px] text-ash-400">{item.summary}</p>
            </div>
            <span className="hidden w-20 shrink-0 text-right text-[12px] text-ash-300 tabular-nums lg:block">
              {formatBytes(item.sizeBytes)}
            </span>
            {/* The remove button's and the switch's room, so the sizes line up with the
                player's own mods above and with these rows once they are files. */}
            <span aria-hidden className="size-8 shrink-0" />
            <span aria-hidden className="w-[34px] shrink-0" />
          </li>
        ))}
      </ul>
    </motion.section>
  );
}
