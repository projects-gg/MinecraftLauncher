import {
  Copy,
  DotsThreeVertical,
  Export,
  FolderOpen,
  FolderSimplePlus,
  MonitorPlay,
  Play,
  Trash,
} from "@phosphor-icons/react";
import { Chip, cn, easeOutExpo, IconButton, Menu, PixelBrackets, useMotionPrefs } from "@pl/ui";
import { Link } from "@tanstack/react-router";
import { motion } from "motion/react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import type { InstanceSummary } from "@/lib/api";
import { formatDuration, formatRelative } from "@/lib/format";
import { InstanceIcon } from "@/shell/instance-icon";

export interface InstanceActions {
  play: (i: InstanceSummary) => void;
  openFolder: (i: InstanceSummary) => void;
  duplicate: (i: InstanceSummary) => void;
  exportPack: (i: InstanceSummary) => void;
  /** Move to a library group (official instances always stay in their own section). */
  group: (i: InstanceSummary) => void;
  /** A desktop shortcut that starts the instance. */
  shortcut: (i: InstanceSummary) => void;
  remove: (i: InstanceSummary) => void;
}

/** Where a card rests; its entrance and exit move from and to the same function list. */
const REST = "translateY(0px) scale(1)";

export function InstanceCard({
  instance,
  selected,
  actions,
  index,
  meta = "lastPlayed",
  ref,
}: {
  instance: InstanceSummary;
  selected: boolean;
  actions: InstanceActions;
  index: number;
  /**
   * The one figure the card's last line shows, with its words: the play time while the library
   * is sorted by it, else when it was last played ("3 dk  18 dakika önce" read as one number).
   */
  meta?: "playTime" | "lastPlayed";
  /** Set by the list's AnimatePresence (popLayout measures the leaving card). */
  ref?: Ref<HTMLElement>;
}) {
  const { t } = useTranslation();
  const { layout, reduce } = useMotionPrefs();
  const loader = instance.loader === "vanilla" ? "Minecraft" : t(`loaders.${instance.loader}`);

  return (
    <motion.article
      ref={ref}
      // Cards glide into a removed card's place; the size never animates (no stretched text),
      // and Performance mode drops the glide.
      layout={layout ? "position" : false}
      // A new card rises in, a removed one settles to 97% as it fades: transform strings run on
      // the compositor (smooth while the library renders). Reduced motion keeps the fades only.
      initial={{ opacity: 0, transform: reduce ? REST : "translateY(12px) scale(1)" }}
      animate={{ opacity: 1, transform: REST }}
      exit={{
        opacity: 0,
        transform: reduce ? REST : "translateY(0px) scale(0.97)",
        transition: { duration: 0.18, ease: easeOutExpo },
      }}
      transition={{ duration: 0.4, delay: Math.min(index, 8) * 0.035, ease: easeOutExpo }}
      className={cn(
        // The hover lift is the `translate` property (Tailwind 4), apart from motion's transform.
        "group glass relative flex flex-col rounded-[var(--radius-panel)] p-3.5 transition-[translate,box-shadow] duration-200",
        "hover:-translate-y-0.5 hover:shadow-[0_18px_40px_rgb(34_6_6/0.45),inset_0_0_0_1px_rgb(240_184_73/0.25)]",
      )}
    >
      {selected ? <PixelBrackets /> : null}
      <div className="flex items-start justify-between">
        <InstanceIcon icon={instance.icon} size={48} />
        <div className="flex flex-col items-end gap-1.5">
          {instance.managed ? <Chip tone="gold">{t("instances.managed")}</Chip> : null}
          {instance.state === "needsInstall" ? <Chip>{t("instances.needsInstall")}</Chip> : null}
          {instance.state === "needsUpdate" ? (
            <Chip tone="jade">{t("instances.needsUpdate")}</Chip>
          ) : null}
        </div>
      </div>

      <Link
        to="/library/$instanceId"
        params={{ instanceId: instance.id }}
        className="mt-3 block rounded-sm outline-none after:absolute after:inset-0 after:rounded-[var(--radius-panel)] focus-visible:after:ring-2 focus-visible:after:ring-gold-300"
      >
        <h3 className="truncate font-display text-[15px] font-semibold text-bone-50">
          {instance.name}
        </h3>
      </Link>
      <p className="mt-0.5 truncate text-[12.5px] text-ash-300">
        {loader} {instance.mcVersion}
      </p>
      {/* Play time and the actions share one row: pointing at the card (or tabbing into it, or
          an open menu) swaps the line for the actions, so no card reserves empty space. */}
      <div className="mt-2.5 grid *:[grid-area:1/1]">
        <div
          className={cn(
            "flex h-8 items-center gap-3 text-[12px] text-ash-400 tabular-nums transition-opacity duration-150",
            "opacity-0 [@media(hover:hover)]:opacity-100 [@media(hover:hover)]:group-focus-within:opacity-0 [@media(hover:hover)]:group-hover:opacity-0 [@media(hover:hover)]:group-has-[[data-state=open]]:opacity-0",
          )}
        >
          <span className="min-w-0 truncate">
            {meta === "playTime" && instance.playTimeSecs > 0
              ? t("play.playTime", { time: formatDuration(instance.playTimeSecs) })
              : instance.lastPlayedAt
                ? t("play.lastPlayed", { when: formatRelative(instance.lastPlayedAt) })
                : t("play.neverPlayed")}
          </span>
        </div>

        {/* Actions sit above the stretched link. */}
        <div className="relative z-10 flex items-center gap-2 opacity-100 transition-opacity duration-150 [@media(hover:hover)]:opacity-0 [@media(hover:hover)]:group-focus-within:opacity-100 [@media(hover:hover)]:group-hover:opacity-100 [@media(hover:hover)]:group-has-[[data-state=open]]:opacity-100">
          <button
            type="button"
            onClick={() => actions.play(instance)}
            className="chamfer flex h-8 flex-1 cursor-pointer items-center justify-center gap-1.5 bg-gold-500 text-[13px] font-semibold text-gold-ink transition-[filter,transform] duration-150 ease-out [--cut:6px] hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-200 active:scale-[0.97]"
          >
            <Play size={14} weight="fill" /> {t("instance.play")}
          </button>
          <Menu
            trigger={
              <IconButton
                label={t("library.card.more")}
                icon={<DotsThreeVertical size={18} weight="bold" />}
                size="sm"
              />
            }
            items={[
              {
                label: t("library.actions.folder"),
                icon: <FolderOpen size={16} />,
                onSelect: () => actions.openFolder(instance),
              },
              {
                label: t("library.actions.duplicate"),
                icon: <Copy size={16} />,
                onSelect: () => actions.duplicate(instance),
              },
              {
                label: t("library.actions.export"),
                icon: <Export size={16} />,
                onSelect: () => actions.exportPack(instance),
              },
              {
                label: t("shortcut.action"),
                icon: <MonitorPlay size={16} />,
                onSelect: () => actions.shortcut(instance),
              },
              ...(instance.managed
                ? []
                : [
                    {
                      label: t("library.actions.group"),
                      icon: <FolderSimplePlus size={16} />,
                      onSelect: () => actions.group(instance),
                    },
                  ]),
              null,
              {
                label: t("library.actions.delete"),
                icon: <Trash size={16} />,
                danger: true,
                disabled: instance.managed,
                onSelect: () => actions.remove(instance),
              },
            ]}
          />
        </div>
      </div>
    </motion.article>
  );
}
