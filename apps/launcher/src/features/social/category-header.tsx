// Friend category pieces of the list: a category's header (folds its friends; a grip on the left
// lifts it to move it; its menu folds, renames, moves, adds and deletes), the row that makes a new
// category, the slot a dragged row or category leaves where it would land, and the lifted ghosts.
import {
  ArrowDown,
  ArrowUp,
  CaretDown,
  CaretRight,
  Check,
  DotsSixVertical,
  DotsThree,
  FolderSimplePlus,
  PencilSimple,
  Trash,
  X,
} from "@phosphor-icons/react";
import {
  ContextMenu,
  cn,
  easeOutExpo,
  IconButton,
  Kbd,
  Menu,
  type MenuItem,
  returnFocusAfterKeys,
  swallowDismissClick,
} from "@pl/ui";
import { AnimatePresence, type MotionValue, motion } from "motion/react";
import { DropdownMenu } from "radix-ui";
import {
  type KeyboardEvent,
  type ReactNode,
  type PointerEvent as ReactPointerEvent,
  useEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { inside, leaveOnEscape } from "./edit-escape";
import { GROUP_NAME_MAX } from "./groups";
import { openMenuFromKey } from "./menu-key";
import { categoryMenu, rowsOf } from "./menus";
import { swap, useSocialMotion } from "./motion";

const menuItemClass =
  "flex h-10 cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-[13.5px] text-bone-100 outline-none select-none data-[disabled]:cursor-default data-[disabled]:opacity-40 data-[highlighted]:bg-white/8";

/** "2/5": friends online (jade while anyone is) over the category's size. */
function Count({ online, count }: { online: number; count: number }) {
  const { reduce } = useSocialMotion();
  return (
    <span className="relative inline-flex font-display text-[12px] tabular-nums">
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span key={`${online}/${count}`} className="flex" {...swap(reduce, 0.18)}>
          <span className={online > 0 ? "text-jade-400" : "text-ash-400"}>{online}</span>
          <span className="text-ash-400">/{count}</span>
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

/** An inline name field: Enter saves, Esc gives up (also when it loses the focus empty). */
function NameField({
  initial,
  label,
  submit,
  onSubmit,
  onCancel,
}: {
  initial: string;
  label: string;
  submit: string;
  onSubmit: (name: string) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [value, setValue] = useState(initial);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);
  // Esc in the field or on its buttons gives up the name, never the panel around it: Radix hears
  // Esc before React does (edit-escape.ts; FIX2: the friends panel closed instead).
  const cancel = useRef(onCancel);
  cancel.current = onCancel;
  useEffect(
    () =>
      leaveOnEscape(
        (target) => inside(input.current?.form, target),
        () => cancel.current(),
      ),
    [],
  );
  const send = () => {
    const name = value.replace(/\s+/g, " ").trim();
    if (name) onSubmit(name);
    else onCancel();
  };
  return (
    <form
      className="flex items-center gap-1.5 py-1 pr-1 pl-2"
      onSubmit={(event) => {
        event.preventDefault();
        send();
      }}
    >
      <input
        ref={input}
        value={value}
        maxLength={GROUP_NAME_MAX}
        aria-label={label}
        placeholder={label}
        onChange={(event) => setValue(event.target.value)}
        className="h-8 min-w-0 flex-1 rounded-md border border-white/10 bg-obsidian-800/80 px-2.5 text-[13px] text-bone-50 transition-[border-color,box-shadow] duration-150 outline-none placeholder:text-ash-500 hover:border-white/16 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-300/25"
      />
      <IconButton label={submit} icon={<Check size={15} weight="bold" />} size="sm" type="submit" />
      <IconButton
        label={t("friendGroups.cancel")}
        icon={<X size={15} />}
        size="sm"
        onClick={onCancel}
      />
    </form>
  );
}

export interface CategoryActions {
  canMoveUp: boolean;
  canMoveDown: boolean;
  onMove: (step: -1 | 1) => void;
  onRename: (name: string) => void;
  onRemove: () => void;
  /** Starts dragging the category from its grip. */
  onGrip: (event: ReactPointerEvent<HTMLElement>) => void;
}

/**
 * A category's header. The name folds and unfolds its friends; the count is friends online over
 * all. Its menu (a right click, Shift+F10 or the menu key, or "…" shown on hover or focus) folds
 * it and makes another category; for the player's own it also renames, moves and deletes, and a
 * grip in the gutter (shown on hover or focus) lifts it (a plain click on it opens "Yukarı taşı /
 * Aşağı taşı"). The implicit "Arkadaşlar" is neither renamed, moved nor deleted.
 */
export function CategoryHeader({
  id,
  name,
  online,
  count,
  folded,
  target,
  actions,
  canAdd,
  onNewCategory,
  onToggle,
  onKeyDown,
}: {
  id: string;
  name: string;
  online: number;
  count: number;
  folded: boolean;
  /** A dragged friend would land here (and it is not where they came from). */
  target: boolean;
  actions: CategoryActions | null;
  /** Another category may be made (fewer than the service keeps). */
  canAdd: boolean;
  onNewCategory: () => void;
  onToggle: () => void;
  onKeyDown?: (event: KeyboardEvent<HTMLButtonElement>) => void;
}) {
  const { t } = useTranslation();
  const calm = useCalm();
  const [grip, setGrip] = useState(false);
  const [renaming, setRenaming] = useState(false);

  const items = rowsOf(
    categoryMenu({
      custom: actions !== null,
      canMoveUp: actions?.canMoveUp ?? false,
      canMoveDown: actions?.canMoveDown ?? false,
      canAdd,
    }),
    (row): MenuItem | null => {
      switch (row.action) {
        case "toggle":
          return {
            label: folded ? t("friendMenus.expand") : t("friendMenus.collapse"),
            icon: folded ? <CaretDown size={16} /> : <CaretRight size={16} />,
            onSelect: onToggle,
          };
        case "rename":
          return {
            label: t("friendGroups.rename"),
            icon: <PencilSimple size={16} />,
            // Once the menu is gone: the name field takes the focus.
            onSelect: () => window.setTimeout(() => setRenaming(true), 0),
          };
        case "moveUp":
          return {
            label: t("friendGroups.moveUp"),
            icon: <ArrowUp size={16} />,
            shortcut: "Alt ↑",
            disabled: row.disabled,
            onSelect: () => actions?.onMove(-1),
          };
        case "moveDown":
          return {
            label: t("friendGroups.moveDown"),
            icon: <ArrowDown size={16} />,
            shortcut: "Alt ↓",
            disabled: row.disabled,
            onSelect: () => actions?.onMove(1),
          };
        case "newCategory":
          return {
            label: t("friendGroups.addMenu"),
            icon: <FolderSimplePlus size={16} />,
            disabled: row.disabled,
            onSelect: () => window.setTimeout(onNewCategory, 0),
          };
        case "remove":
          return {
            label: t("friendGroups.remove"),
            icon: <Trash size={16} />,
            danger: true,
            onSelect: () => actions?.onRemove(),
          };
      }
    },
  );

  if (renaming && actions) {
    return (
      <div className="pt-3">
        <NameField
          initial={name}
          label={t("friendGroups.name")}
          submit={t("friendGroups.save")}
          onSubmit={(next) => {
            setRenaming(false);
            actions.onRename(next);
          }}
          onCancel={() => setRenaming(false)}
        />
      </div>
    );
  }

  const moveItems = actions ? (
    <>
      <DropdownMenu.Item
        disabled={!actions.canMoveUp}
        onSelect={() => actions.onMove(-1)}
        className={menuItemClass}
      >
        <ArrowUp size={16} className="text-ash-300" aria-hidden />
        <span className="flex-1">{t("friendGroups.moveUp")}</span>
        <Kbd>Alt ↑</Kbd>
      </DropdownMenu.Item>
      <DropdownMenu.Item
        disabled={!actions.canMoveDown}
        onSelect={() => actions.onMove(1)}
        className={menuItemClass}
      >
        <ArrowDown size={16} className="text-ash-300" aria-hidden />
        <span className="flex-1">{t("friendGroups.moveDown")}</span>
        <Kbd>Alt ↓</Kbd>
      </DropdownMenu.Item>
    </>
  ) : null;

  const menuPanel = (children: ReactNode) => (
    <motion.div
      className="z-[60] w-[232px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-1.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
      style={{ transformOrigin: "var(--radix-dropdown-menu-content-transform-origin)" }}
      initial={calm({ opacity: 0, scale: 0.96, y: -4 })}
      animate={calm({
        opacity: 1,
        scale: 1,
        y: 0,
        transition: { duration: 0.18, ease: easeOutExpo },
      })}
      exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.12 } })}
    >
      {children}
    </motion.div>
  );

  return (
    <ContextMenu items={items}>
      <div className="group/header relative flex items-center gap-1 pt-3 pr-1 pb-1">
        {actions ? (
          <DropdownMenu.Root open={grip} onOpenChange={setGrip} modal={false}>
            <DropdownMenu.Trigger asChild>
              <button
                type="button"
                aria-label={t("friendGroups.grip", { name })}
                onPointerDown={(event) => {
                  if (event.button !== 0) return;
                  // Keeps the menu shut on press (Radix opens on pointerdown); a click opens it
                  // (the click that ends a drag never arrives).
                  event.preventDefault();
                  actions.onGrip(event);
                }}
                onClick={(event) => {
                  if (event.detail === 0) return;
                  setGrip(true);
                }}
                className="absolute top-3 -left-[15px] flex h-7 w-[15px] cursor-grab touch-none items-center justify-center rounded-l-md text-ash-400 opacity-0 transition-[opacity,color] duration-150 group-hover/header:opacity-100 hover:text-bone-100 focus-visible:opacity-100 active:cursor-grabbing data-[state=open]:text-gold-300 data-[state=open]:opacity-100"
              >
                <DotsSixVertical size={14} weight="bold" aria-hidden />
              </button>
            </DropdownMenu.Trigger>
            <AnimatePresence>
              {grip ? (
                <DropdownMenu.Portal forceMount>
                  <DropdownMenu.Content
                    onPointerDownOutside={swallowDismissClick}
                    onCloseAutoFocus={returnFocusAfterKeys}
                    forceMount
                    asChild
                    side="bottom"
                    align="start"
                    sideOffset={4}
                    collisionPadding={12}
                    loop
                  >
                    {menuPanel(moveItems)}
                  </DropdownMenu.Content>
                </DropdownMenu.Portal>
              ) : null}
            </AnimatePresence>
          </DropdownMenu.Root>
        ) : null}
        <button
          type="button"
          data-category-header={id}
          aria-expanded={!folded}
          aria-label={t("friendGroups.summary", { name, online, count })}
          onClick={onToggle}
          onKeyDown={(event) => {
            if (!openMenuFromKey(event)) onKeyDown?.(event);
          }}
          className="flex h-7 min-w-0 flex-1 cursor-pointer items-center gap-1.5 rounded-md px-1.5 text-left outline-none transition-colors duration-150 hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          <CaretDown
            size={12}
            weight="bold"
            aria-hidden
            className={cn(
              "shrink-0 transition-[rotate,color] duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]",
              folded && "-rotate-90",
              target ? "text-gold-300" : "text-ash-400",
            )}
          />
          <span
            className={cn(
              "min-w-0 truncate text-[12.5px] font-semibold transition-colors duration-150",
              target ? "text-gold-200" : "text-bone-100/85",
            )}
          >
            {name}
          </span>
          <Count online={online} count={count} />
        </button>
        <Menu
          items={items}
          trigger={
            <IconButton
              label={t("friendGroups.options", { name })}
              icon={<DotsThree size={18} weight="bold" />}
              size="sm"
              className="opacity-0 transition-opacity duration-150 group-hover/header:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
            />
          }
        />
      </div>
    </ContextMenu>
  );
}

/** "+ Yeni kategori", or its name field. */
export function NewCategoryRow({
  editing,
  disabled,
  onStart,
  onCreate,
  onCancel,
}: {
  editing: boolean;
  disabled: boolean;
  onStart: () => void;
  onCreate: (name: string) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  if (editing) {
    return (
      <div className="pt-2">
        <NameField
          initial=""
          label={t("friendGroups.name")}
          submit={t("friendGroups.create")}
          onSubmit={onCreate}
          onCancel={onCancel}
        />
      </div>
    );
  }
  return (
    <div className="pt-2">
      <button
        type="button"
        disabled={disabled}
        title={disabled ? t("friendGroups.limit") : undefined}
        onClick={onStart}
        className="flex h-9 w-full cursor-pointer items-center gap-2 rounded-[var(--radius-panel)] px-2.5 text-[13px] text-ash-300 outline-none transition-[background-color,color,opacity,transform,scale] duration-150 hover:bg-white/[0.045] hover:text-bone-100 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.98] disabled:cursor-default disabled:opacity-45"
      >
        <FolderSimplePlus size={16} aria-hidden />
        {t("friendGroups.add")}
      </button>
    </div>
  );
}

/**
 * Where a dragged row or category would land: its height, a soft gold outline. `origin`: the place
 * a friend over another category came from, kept open until the drop (a faint outline).
 */
export function DragSlot({
  height,
  folded,
  origin,
}: {
  height: number;
  folded?: boolean;
  origin?: boolean;
}) {
  return (
    <div
      data-drag-slot={origin ? undefined : true}
      data-drag-origin={origin ? true : undefined}
      data-folded={folded ? "1" : undefined}
      aria-hidden
      style={{ height }}
      className="py-0.5"
    >
      <div
        className={cn(
          "h-full rounded-[var(--radius-panel)] border border-dashed",
          origin ? "border-white/10" : "border-gold-400/35 bg-gold-500/[0.06]",
        )}
      />
    </div>
  );
}

/** An empty category's line: a quiet "Boş" that becomes "Buraya bırak" while something is dragged. */
export function EmptyCategory({ dragging }: { dragging: boolean }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  return (
    <div className="px-2 pb-1">
      <div
        className={cn(
          "flex h-9 items-center rounded-[var(--radius-panel)] border border-dashed px-3 text-[12.5px] transition-colors duration-200",
          dragging ? "border-gold-400/30 text-gold-200/90" : "border-white/10 text-ash-400",
        )}
      >
        <span className="relative inline-flex">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span key={String(dragging)} {...swap(reduce, 0.16)}>
              {dragging ? t("friendGroups.dropHere") : t("friendGroups.empty")}
            </motion.span>
          </AnimatePresence>
        </span>
      </div>
    </div>
  );
}

/** The lifted layer that follows the pointer (fixed, above the sheet). */
export function DragGhost({
  left,
  width,
  y,
  scale,
  opacity,
  children,
}: {
  left: number;
  width: number;
  y: MotionValue<number>;
  scale: MotionValue<number>;
  opacity: MotionValue<number>;
  children: ReactNode;
}) {
  return createPortal(
    <motion.div
      aria-hidden
      className="pointer-events-none fixed top-0 z-[70] cursor-grabbing"
      style={{ left, width, y, scale, opacity }}
    >
      <motion.div
        className="rounded-[var(--radius-panel)] bg-obsidian-900 shadow-[0_18px_40px_rgb(34_6_6/0.6),0_0_0_1px_rgb(246_213_142/0.28)]"
        initial={{ opacity: 0.6 }}
        animate={{ opacity: 1, transition: { duration: 0.12 } }}
      >
        {children}
      </motion.div>
    </motion.div>,
    document.body,
  );
}
