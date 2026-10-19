// Small building blocks shared by the instance page tabs.
import {
  Archive,
  ArrowCounterClockwise,
  DotsThreeVertical,
  Globe,
  ImageSquare,
  Package,
  Trash,
} from "@phosphor-icons/react";
import {
  Button,
  cn,
  Dialog,
  EmptyState,
  easeOutExpo,
  IconButton,
  Menu,
  useMotionPrefs,
} from "@pl/ui";
import { motion } from "motion/react";
import type { ReactNode, Ref } from "react";
import { useTranslation } from "react-i18next";
import type { TrashItem } from "@/lib/api";
import { formatBytes, formatRelative } from "@/lib/format";
import { trashLabel } from "./trash-label";

export const trashIcons = {
  world: Globe,
  backup: Archive,
  screenshot: ImageSquare,
  modSet: Package,
  versionSet: Package,
  instance: Package,
} as const;

/**
 * A launcher trash item with "put back" and "delete for good". Inside `AnimatePresence
 * mode="popLayout"` it fades in and out in place while the rows below glide (it forwards `ref`).
 */
export function TrashRow({
  ref,
  item,
  disabled,
  onRestore,
  onPurge,
}: {
  ref?: Ref<HTMLLIElement>;
  item: TrashItem;
  /** Putting it back is not possible right now (worlds while the game runs). */
  disabled?: boolean;
  onRestore: () => void;
  onPurge: () => void;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const Icon = trashIcons[item.kind];
  return (
    <motion.li
      ref={ref}
      layout={layout ? "position" : false}
      transition={{ layout: { duration: 0.3, ease: easeOutExpo } }}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { duration: 0.2 } }}
      exit={{ opacity: 0, transition: { duration: 0.12 } }}
      className="flex items-center gap-3 rounded-md px-3 py-2 transition-colors hover:bg-white/4"
    >
      <Icon size={18} className="shrink-0 text-ash-400" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-[13px] font-medium text-bone-100">{trashLabel(item)}</p>
        <p className="flex gap-3 text-[12px] text-ash-300 tabular-nums">
          <span>{t("instance.trash.deletedAt", { when: formatRelative(item.deletedAt) })}</span>
          <span>{formatBytes(item.sizeBytes)}</span>
        </p>
      </div>
      <Button
        size="sm"
        variant="ghost"
        icon={<ArrowCounterClockwise size={15} />}
        disabled={disabled}
        onClick={onRestore}
      >
        {t("instance.trash.restore")}
      </Button>
      <Menu
        trigger={
          <IconButton
            label={t("instance.trash.more", { name: trashLabel(item) })}
            icon={<DotsThreeVertical size={18} weight="bold" />}
            size="sm"
          />
        }
        items={[
          {
            label: t("instance.trash.purge"),
            icon: <Trash size={16} />,
            danger: true,
            onSelect: onPurge,
          },
        ]}
      />
    </motion.li>
  );
}

/** "Are you sure?" dialog with a cancel and a (usually destructive) confirm button. */
export function ConfirmDialog({
  open,
  title,
  body,
  confirmLabel,
  danger = true,
  onConfirm,
  onClose,
  children,
}: {
  open: boolean;
  title: string;
  body?: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => void;
  onClose: () => void;
  children?: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && onClose()}
      title={title}
      description={body}
      closeLabel={t("common.close")}
      width={460}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("instance.cancel")}
          </Button>
          <Button
            variant={danger ? "danger" : "primary"}
            onClick={() => {
              onConfirm();
              onClose();
            }}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      {children}
    </Dialog>
  );
}

/** One-line notice inside a tab (game running, truncated log). */
export function InlineNotice({
  icon,
  children,
  tone = "neutral",
  action,
  className,
}: {
  icon: ReactNode;
  children: ReactNode;
  tone?: "neutral" | "warn";
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div
      role="status"
      className={cn(
        "flex items-center gap-3 rounded-[var(--radius-panel)] px-4 py-2.5 text-[13px]",
        tone === "warn"
          ? "bg-gold-500/12 text-gold-200 ring-1 ring-inset ring-gold-400/20"
          : "glass text-ash-300",
        className,
      )}
    >
      <span className="flex shrink-0">{icon}</span>
      <p className="min-w-0 flex-1">{children}</p>
      {action}
    </div>
  );
}

const toggleTones = {
  error: "bg-flare-400/16 text-flare-300",
  warn: "bg-gold-500/22 text-gold-200",
  info: "bg-white/10 text-bone-100",
  neutral: "bg-white/7 text-ash-300",
} as const;

/** Chamfered on/off chip (log level filters). The focus ring is a chamfered layer. */
export function ToggleChip({
  pressed,
  onPressedChange,
  tone,
  disabled,
  children,
  label,
}: {
  pressed: boolean;
  onPressedChange: (pressed: boolean) => void;
  tone: keyof typeof toggleTones;
  disabled?: boolean;
  children: ReactNode;
  label?: string;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      aria-label={label}
      disabled={disabled}
      onClick={() => onPressedChange(!pressed)}
      // Presses and dims like a Button; a chip that is on also lights under the pointer.
      className="group relative isolate inline-flex h-7 shrink-0 cursor-pointer items-center gap-1.5 px-2.5 text-[12px] font-semibold whitespace-nowrap transition-[transform,scale,opacity] duration-150 ease-out [--cut:5px] focus-visible:outline-none active:scale-[0.97] disabled:cursor-default disabled:opacity-45"
    >
      <span
        aria-hidden
        className="chamfer absolute -inset-[2px] -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible:opacity-100"
      />
      <span
        aria-hidden
        className={cn(
          "chamfer absolute inset-0 -z-10 transition-[background-color,filter] duration-150",
          pressed
            ? `${toggleTones[tone]} group-enabled:group-hover:brightness-125`
            : "bg-white/[0.035] group-enabled:group-hover:bg-white/7",
        )}
      />
      <span
        className={cn(
          "flex items-center gap-1.5 transition-colors",
          pressed ? (toggleTones[tone].split(" ")[1] ?? "") : "text-ash-400",
        )}
      >
        {children}
      </span>
    </button>
  );
}

/**
 * A tab's empty or failed state: the shared `EmptyState` with the tab's mark, the same in every
 * instance tab; several actions sit in one row.
 */
export function TabEmpty({
  icon,
  title,
  body,
  action,
  className,
}: {
  icon: ReactNode;
  title: string;
  body?: string;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <EmptyState
      icon={icon}
      title={title}
      body={body}
      action={action ? <div className="flex flex-wrap justify-center gap-2">{action}</div> : null}
      className={cn("pt-10", className)}
    />
  );
}

/** Section heading inside a tab, with an optional caption on the right. */
export function TabSection({
  title,
  caption,
  children,
  className,
}: {
  title: string;
  caption?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={className}>
      <div className="mb-2 flex items-baseline justify-between gap-4 px-1">
        <h3 className="font-display text-[15px] font-semibold text-bone-50">{title}</h3>
        {caption ? <p className="text-[12px] text-ash-300">{caption}</p> : null}
      </div>
      {children}
    </section>
  );
}
