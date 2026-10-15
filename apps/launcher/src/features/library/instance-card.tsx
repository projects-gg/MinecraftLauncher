import {
  Copy,
  DotsThreeVertical,
  Export,
  FolderOpen,
  Info,
  Play,
  Trash,
} from "@phosphor-icons/react";
import { Chip, cn, IconButton, Menu, PixelBrackets } from "@pl/ui";
import { Link } from "@tanstack/react-router";
import { motion } from "motion/react";
import { useTranslation } from "react-i18next";
import type { InstanceSummary } from "@/lib/api";
import { formatDuration, formatRelative } from "@/lib/format";
import { InstanceIcon } from "@/shell/instance-icon";

export interface InstanceActions {
  play: (i: InstanceSummary) => void;
  openFolder: (i: InstanceSummary) => void;
  duplicate: (i: InstanceSummary) => void;
  exportPack: (i: InstanceSummary) => void;
  remove: (i: InstanceSummary) => void;
}

export function InstanceCard({
  instance,
  selected,
  actions,
  index,
}: {
  instance: InstanceSummary;
  selected: boolean;
  actions: InstanceActions;
  index: number;
}) {
  const { t } = useTranslation();
  const loader = instance.loader === "vanilla" ? "Minecraft" : t(`loaders.${instance.loader}`);

  return (
    <motion.article
      layout
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.4, delay: Math.min(index, 8) * 0.035, ease: [0.16, 1, 0.3, 1] }}
      className={cn(
        "group glass relative flex flex-col rounded-[var(--radius-panel)] p-4 transition-[transform,box-shadow] duration-200",
        "hover:-translate-y-0.5 hover:shadow-[0_18px_40px_rgb(34_6_6/0.45),inset_0_0_0_1px_rgb(240_184_73/0.25)]",
      )}
    >
      {selected ? <PixelBrackets /> : null}
      <div className="flex items-start justify-between">
        <InstanceIcon icon={instance.icon} size={64} />
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
        className="mt-4 block rounded-sm outline-none after:absolute after:inset-0 after:rounded-[var(--radius-panel)] focus-visible:after:ring-2 focus-visible:after:ring-gold-300"
      >
        <h3 className="truncate font-display text-[17px] font-semibold text-bone-50">
          {instance.name}
        </h3>
      </Link>
      <p className="mt-0.5 truncate text-[13px] text-ash-400">
        {loader} {instance.mcVersion}
      </p>
      <div className="mt-3 flex gap-3 text-[12px] text-ash-500">
        {instance.playTimeSecs > 0 ? <span>{formatDuration(instance.playTimeSecs)}</span> : null}
        <span>
          {instance.lastPlayedAt ? formatRelative(instance.lastPlayedAt) : t("library.card.never")}
        </span>
      </div>

      {/* Actions sit above the stretched link. */}
      <div className="relative z-10 mt-4 flex items-center gap-2 opacity-100 transition-opacity duration-150 [@media(hover:hover)]:opacity-0 [@media(hover:hover)]:group-focus-within:opacity-100 [@media(hover:hover)]:group-hover:opacity-100">
        <button
          type="button"
          onClick={() => actions.play(instance)}
          className="chamfer flex h-8 flex-1 cursor-pointer items-center justify-center gap-1.5 bg-gold-500 text-[13px] font-semibold text-gold-ink transition-[filter] [--cut:6px] hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-200"
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
        <IconButton
          label={t("library.actions.open")}
          icon={<Info size={18} />}
          size="sm"
          className="sr-only"
        />
      </div>
    </motion.article>
  );
}
