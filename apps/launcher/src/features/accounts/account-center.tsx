// "Hesap merkezi": every game account in one place. Rendered in a side sheet (title bar, palette)
// and inside Settings → Hesaplar; both are the same component over the same data.
// Order (owner's rule): pinned accounts first, dragged into the player's order (handle in the
// left gutter, its menu, or Alt+↑/↓); the rest sorted by activity, never by hand.
import { ArrowDown, ArrowUp, CaretRight, DotsSixVertical, Robot, X } from "@phosphor-icons/react";
import {
  ContentIcon,
  cn,
  EntranceGate,
  easeOutExpo,
  exitOf,
  focusPanel,
  GatedMotion,
  IconButton,
  Kbd,
  ReducedMotionConfig,
  returnFocusAfterKeys,
  Skeleton,
  scrimMotion,
  scrollBehavior,
  sheetMotion,
  sheetOutside,
  springSnappy,
  swallowDismissClick,
  useInteractionMotion,
} from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import {
  AnimatePresence,
  type DragControls,
  LayoutGroup,
  motion,
  Reorder,
  useDragControls,
} from "motion/react";
import { Dialog, DropdownMenu } from "radix-ui";
import {
  type KeyboardEvent,
  type ReactNode,
  type Ref,
  type RefObject,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { ProjectsLinkRow } from "@/features/projects-skin/link-row";
import { useProjectsSkinOverview } from "@/features/projects-skin/queries";
import { SocialAccountRow } from "@/features/social/account-row";
import { type AccountSummary, api } from "@/lib/api";
import { AccountCard } from "./account-card";
import { movePinned, pinnedIds, withPinnedOrder } from "./account-order";
import { AddAccount } from "./add-account";
import { afkSummary, onlineAfkSession } from "./afk-match";
import { useCalm } from "./calm";
import { lastInputWasKeyboard, useAccountsUi } from "./store";
import {
  ACCOUNTS_KEY,
  announce,
  friendlyError,
  pinAccount,
  removeAccount,
  reorderPinned,
  setAccountAvatar,
  switchAccount,
  undoLatestRemoval,
  useAccountHeads,
  useAccounts,
  useAfkSessions,
  useSessionChecks,
} from "./use-accounts";
import { usePlayingAccountIds } from "./use-own-launch";

const enter = { duration: 0.3, ease: easeOutExpo };
const NO_ACCOUNTS: AccountSummary[] = [];

/**
 * A row that moves in the list can lose keyboard focus when React moves its DOM node (the
 * browser drops focus to the body). `keep()` remembers the focused control before a change that
 * reorders rows; after each commit in the next moments, focus goes back to it, before paint and
 * before the sheet's focus trap would pull it to the container. Focus the player moved
 * elsewhere is left alone.
 */
function useKeepFocus() {
  const kept = useRef<{ element: HTMLElement; until: number } | null>(null);
  useLayoutEffect(() => {
    const entry = kept.current;
    if (!entry) return;
    if (performance.now() > entry.until) {
      kept.current = null;
      return;
    }
    const lost = !document.activeElement || document.activeElement === document.body;
    if (lost && entry.element.isConnected) entry.element.focus({ preventScroll: true });
  });
  return () => {
    const focused = document.activeElement;
    if (focused instanceof HTMLElement && focused !== document.body) {
      kept.current = { element: focused, until: performance.now() + 800 };
    }
  };
}

const menuItemClass =
  "flex h-10 cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-[13.5px] text-bone-100 outline-none select-none data-[disabled]:cursor-default data-[disabled]:opacity-40 data-[highlighted]:bg-white/8";

/**
 * The grip of a pinned account, in the gutter left of its card. Press and drag to move it; a
 * plain click (or Enter) opens "Yukarı taşı / Aşağı taşı", the single-pointer way to reorder.
 */
function DragHandle({
  name,
  controls,
  dragged,
  canMoveUp,
  canMoveDown,
  onMove,
  menuOpen,
  setMenuOpen,
}: {
  name: string;
  controls: DragControls;
  dragged: RefObject<boolean>;
  canMoveUp: boolean;
  canMoveDown: boolean;
  onMove: (step: -1 | 1) => void;
  menuOpen: boolean;
  setMenuOpen: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const calm = useCalm();
  return (
    <DropdownMenu.Root open={menuOpen} onOpenChange={setMenuOpen} modal={false}>
      <DropdownMenu.Trigger asChild>
        <button
          type="button"
          aria-label={t("accounts.center.move", { name })}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            // Keeps the menu shut on press (Radix opens on pointerdown); a click opens it below.
            event.preventDefault();
            dragged.current = false;
            controls.start(event);
          }}
          onClick={(event) => {
            if (dragged.current || event.detail === 0) return;
            setMenuOpen(true);
          }}
          className={cn(
            "absolute top-0 -left-[22px] flex h-[72px] w-[22px] cursor-grab touch-none items-center justify-center rounded-l-md text-ash-400",
            "opacity-0 transition-[opacity,color] duration-150 group-hover/row:opacity-100 hover:text-bone-100 focus-visible:opacity-100 active:cursor-grabbing data-[state=open]:text-gold-300 data-[state=open]:opacity-100",
          )}
        >
          <DotsSixVertical size={16} weight="bold" aria-hidden />
        </button>
      </DropdownMenu.Trigger>
      <AnimatePresence>
        {menuOpen ? (
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
              <motion.div
                className="z-50 w-[232px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-1.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
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
                <DropdownMenu.Item
                  disabled={!canMoveUp}
                  onSelect={() => onMove(-1)}
                  className={menuItemClass}
                >
                  <ArrowUp size={16} className="text-ash-300" aria-hidden />
                  <span className="flex-1">{t("accounts.center.moveUp")}</span>
                  <Kbd>Alt ↑</Kbd>
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  disabled={!canMoveDown}
                  onSelect={() => onMove(1)}
                  className={menuItemClass}
                >
                  <ArrowDown size={16} className="text-ash-300" aria-hidden />
                  <span className="flex-1">{t("accounts.center.moveDown")}</span>
                  <Kbd>Alt ↓</Kbd>
                </DropdownMenu.Item>
              </motion.div>
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        ) : null}
      </AnimatePresence>
    </DropdownMenu.Root>
  );
}

/**
 * One list row. Every row is a `Reorder.Item` (so pinning never remounts it and its move is a
 * plain layout animation), but only pinned rows, and only while there are two of them, drag.
 * Cards are translucent; while a row moves (or is lifted) a solid layer under it keeps rows
 * that pass each other from showing through, and the row the player acted on rides on top.
 * A lifted row scales up a touch and gets a deeper shadow and a gold hairline.
 */
function AccountRow({
  ref,
  account,
  draggable,
  raised,
  fresh,
  position,
  pinnedCount,
  onMove,
  onLift,
  onDragEnd,
  children,
}: {
  /** AnimatePresence's `popLayout` measures the leaving row through this ref. */
  ref?: Ref<HTMLLIElement>;
  account: AccountSummary;
  draggable: boolean;
  /** The row the player just acted on: drawn above the rows it passes. */
  raised: boolean;
  /** Just added or brought back: it fades in over the row sliding out of its place. */
  fresh: boolean;
  /** Index among the pinned accounts. */
  position: number;
  pinnedCount: number;
  onMove: (step: -1 | 1) => void;
  onLift: () => void;
  onDragEnd: () => void;
  children: ReactNode;
}) {
  const controls = useDragControls();
  const dragged = useRef(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [moving, setMoving] = useState(false);
  const [entered, setEntered] = useState(false);
  const calm = useCalm();
  const solid = moving || (fresh && !entered);
  return (
    <Reorder.Item
      ref={ref}
      as="li"
      value={account.id}
      data-account-row={account.id}
      layout="position"
      drag={draggable ? "y" : false}
      dragListener={false}
      dragControls={controls}
      onDragStart={() => {
        dragged.current = true;
        setMenuOpen(false);
        onLift();
      }}
      onDragEnd={onDragEnd}
      onLayoutAnimationStart={() => setMoving(true)}
      onLayoutAnimationComplete={() => setMoving(false)}
      onAnimationComplete={() => setEntered(true)}
      // Reorder.Item writes its own inline z-index; `!` lets the acted-on row stay on top.
      className={cn("group/row relative", (raised || fresh) && "z-[2]!")}
      // A newcomer fades in once the rows below have made room (the spring clears a slot in
      // about 0.15 s).
      initial={calm({ opacity: 0, y: 10, scale: 0.98 })}
      animate={calm({ opacity: 1, y: 0, scale: 1, transition: { ...enter, delay: 0.18 } })}
      exit={calm({
        opacity: 0,
        scale: 0.97,
        transition: { duration: exitOf(0.26), ease: easeOutExpo },
      })}
      transition={{ layout: springSnappy }}
      variants={{ lifted: calm({ scale: 1.015, transition: springSnappy }) }}
      whileDrag="lifted"
    >
      <span
        aria-hidden
        className="pointer-events-none absolute inset-0 rounded-[var(--radius-panel)] bg-obsidian-900"
        style={{ opacity: solid ? 1 : 0, transition: solid ? "none" : "opacity 220ms ease-out" }}
      />
      <motion.span
        aria-hidden
        className="pointer-events-none absolute inset-0 rounded-[var(--radius-panel)] bg-obsidian-900 shadow-[0_18px_40px_rgb(34_6_6/0.6),0_0_0_1px_rgb(246_213_142/0.28)]"
        style={{ opacity: 0 }}
        variants={{ lifted: { opacity: 1, transition: { duration: 0.12 } } }}
      />
      {draggable ? (
        <DragHandle
          name={account.username}
          controls={controls}
          dragged={dragged}
          canMoveUp={position > 0}
          canMoveDown={position < pinnedCount - 1}
          onMove={onMove}
          menuOpen={menuOpen}
          setMenuOpen={setMenuOpen}
        />
      ) : null}
      {children}
    </Reorder.Item>
  );
}

function ListSkeleton() {
  return (
    <div className="space-y-2" aria-hidden>
      {[0, 1].map((i) => (
        <div
          key={i}
          className="flex items-center gap-3.5 rounded-[var(--radius-panel)] p-3 ring-1 ring-white/6"
        >
          <Skeleton className="size-12 rounded-[var(--radius-head)]" />
          <div className="flex-1 space-y-2">
            <Skeleton className="h-4 w-36" />
            <Skeleton className="h-3.5 w-48" />
          </div>
        </div>
      ))}
    </div>
  );
}

function EmptyAccounts() {
  const { t } = useTranslation();
  const calm = useCalm();
  return (
    <motion.div
      key="empty"
      className="flex flex-col items-center px-4 pt-4 pb-2 text-center"
      initial={calm({ opacity: 0, y: 8 })}
      animate={calm({ opacity: 1, y: 0, transition: enter })}
      exit={{ opacity: 0, transition: { duration: exitOf(0.3) } }}
    >
      <span className="flex size-16 items-center justify-center rounded-[var(--radius-head)] bg-gold-700/25 text-gold-300 ring-1 ring-gold-400/25">
        <ContentIcon kind="account" />
      </span>
      <h3 className="mt-4 font-display text-[19px] font-semibold text-bone-50">
        {t("accounts.center.emptyTitle")}
      </h3>
    </motion.div>
  );
}

function AfkRow({ onOpen }: { onOpen: () => void }) {
  const { t } = useTranslation();
  const sessions = useAfkSessions(true);
  const { total, online } = afkSummary(sessions);
  if (total === 0) return null;
  return (
    <motion.div layout="position" transition={springSnappy}>
      <button
        type="button"
        onClick={onOpen}
        className="group flex min-h-14 w-full cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-3.5 py-2.5 text-left ring-1 ring-white/[0.07] transition-[background-color,box-shadow] duration-200 hover:bg-white/[0.05] hover:ring-white/12 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none"
      >
        <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-white/[0.05] text-ash-300">
          <Robot size={19} aria-hidden />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-[14px] font-medium text-bone-50">
            {t("accounts.center.afkRow")}
          </span>
          <span className="block truncate text-[12.5px] text-ash-400">
            {t("accounts.center.afkTotal", { count: total })},{" "}
            {online > 0 ? (
              <span className="text-jade-400">
                {t("accounts.center.afkOnline", { count: online })}
              </span>
            ) : (
              t("accounts.center.afkNoneOnline")
            )}
          </span>
        </span>
        <CaretRight
          size={16}
          className="shrink-0 text-ash-400 transition-transform duration-150 group-hover:translate-x-0.5"
          aria-hidden
        />
      </button>
    </motion.div>
  );
}

/** The account list with its actions, the AFK link and the add section. */
export function AccountCenter({
  variant,
  onNavigate,
}: {
  variant: "sheet" | "embedded";
  /** Called before leaving for another page (the sheet closes). */
  onNavigate?: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { data: accounts = NO_ACCOUNTS, isPending } = useAccounts();
  const sessions = useAfkSessions(true);
  // The account whose game is on screen says so (PLAY-ACCT): OYNA is that account's alone.
  // Every account with a game on screen (PLAY-AGAIN: several may play at once).
  const playing = usePlayingAccountIds();
  const openLogin = useAccountsUi((s) => s.openLogin);
  const openAppearance = useAccountsUi((s) => s.openAppearance);
  const fresh = useAccountsUi((s) => s.fresh);
  const markFresh = useAccountsUi((s) => s.markFresh);
  // Username accounts get the Projects skin panel while the signed content offers it (SKIN1).
  const projectsSkins = Boolean(useProjectsSkinOverview().data?.shown);
  const addInput = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const listId = useId();
  const bracketsId = `account-brackets-${variant}`;
  useSessionChecks(accounts, true);
  useAccountHeads(accounts);

  // While a pinned row is dragged, the pinned order lives here; it is dropped as soon as the
  // saved list catches up (never mid-drag), so the rows never flash back to the old order.
  const [dragOrder, setDragOrder] = useState<string[] | null>(null);
  const latestOrder = useRef<string[] | null>(null);
  const dragging = useRef(false);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs whenever the saved list changes
  useEffect(() => {
    if (dragging.current) return;
    latestOrder.current = null;
    setDragOrder(null);
  }, [accounts]);
  const onReorder = (order: string[]) => {
    dragging.current = true;
    latestOrder.current = order;
    setDragOrder(order);
  };
  const shown = withPinnedOrder(accounts, dragOrder);
  const pinned = pinnedIds(shown);
  const canDrag = pinned.length > 1;
  // The row the player last moved, pinned or switched to: it passes over the others.
  const [acted, setActed] = useState<string | null>(null);
  const keepFocus = useKeepFocus();

  const movePinnedBy = (account: AccountSummary, step: -1 | 1) => {
    const next = movePinned(pinned, account.id, step);
    if (!next) return;
    setActed(account.id);
    keepFocus();
    void reorderPinned(queryClient, next);
    announce(
      t("accounts.center.moved", {
        name: account.username,
        position: next.indexOf(account.id) + 1,
      }),
    );
  };

  const endDrag = (account: AccountSummary) => {
    const order = dragging.current ? latestOrder.current : null;
    dragging.current = false;
    if (!order) return;
    void reorderPinned(queryClient, order).then((saved) => {
      if (!saved) setDragOrder(null);
    });
    announce(
      t("accounts.center.moved", {
        name: account.username,
        position: order.indexOf(account.id) + 1,
      }),
    );
  };

  const rename = async (account: AccountSummary, name: string) => {
    try {
      const renamed = await api.renameAccount(account.id, name);
      queryClient.setQueryData<AccountSummary[]>(ACCOUNTS_KEY, (list) =>
        list?.map((a) => (a.id === renamed.id ? { ...renamed, active: a.active } : a)),
      );
      void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
      announce(t("accounts.center.renamed", { name: renamed.username }));
      return null;
    } catch (error) {
      return friendlyError(error, t("accounts.center.renameFailed")).message;
    }
  };

  const lastRemoval = useRef(0);
  const remove = (account: AccountSummary, index: number) => {
    // The next card slides under the pointer: a double click must not take it too.
    if (performance.now() - lastRemoval.current < 450) return;
    lastRemoval.current = performance.now();
    // Focus moves to the neighbour (or the name field when the list empties).
    const mains = listRef.current?.querySelectorAll<HTMLElement>("[data-account-main]");
    const neighbour = mains?.[index + 1] ?? mains?.[index - 1];
    void removeAccount(queryClient, account);
    requestAnimationFrame(() => {
      (neighbour ?? addInput.current)?.focus();
      // The list may re-sort under it (a new active account moves up): keep it focused.
      keepFocus();
    });
  };

  const onListKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    if (event.altKey) {
      // Alt+↑/↓ moves a pinned account; focus stays on whatever control of its row had it.
      const row = (document.activeElement as HTMLElement | null)?.closest("[data-account-row]");
      const account = shown.find((a) => a.id === row?.getAttribute("data-account-row"));
      if (!account?.pinned || !listRef.current?.contains(row ?? null)) return;
      event.preventDefault();
      movePinnedBy(account, event.key === "ArrowUp" ? -1 : 1);
      return;
    }
    const mains = [
      ...(listRef.current?.querySelectorAll<HTMLElement>("[data-account-main]") ?? []),
    ];
    const at = mains.indexOf(document.activeElement as HTMLElement);
    if (at < 0) return;
    event.preventDefault();
    const next = mains[(at + (event.key === "ArrowDown" ? 1 : -1) + mains.length) % mains.length];
    next?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    const typing = target.tagName === "INPUT" || target.tagName === "TEXTAREA";
    // Not with Alt: Windows reports AltGr as Ctrl+Alt (Turkish Q types with it).
    const ctrlOnly = (event.ctrlKey || event.metaKey) && !event.altKey;
    if (ctrlOnly && event.key.toLowerCase() === "z" && !typing) {
      event.preventDefault();
      void undoLatestRemoval(queryClient);
    }
  };

  const empty = !isPending && accounts.length === 0;
  const openAfk = () => {
    onNavigate?.();
    void navigate({ to: "/afk" });
  };

  return (
    <ReducedMotionConfig>
      {/* biome-ignore lint/a11y/noStaticElementInteractions: Ctrl+Z from anywhere in the center */}
      <div onKeyDown={onKeyDown} className={cn(variant === "embedded" && "pt-2")}>
        <LayoutGroup id={`account-center-${variant}`}>
          {isPending ? (
            <ListSkeleton />
          ) : (
            <AnimatePresence initial={false} mode="popLayout">
              {empty ? <EmptyAccounts /> : null}
            </AnimatePresence>
          )}
          <h3 id={listId} className="sr-only">
            {t("accounts.center.listLabel")}
          </h3>
          {/* Always mounted, so the last account can still animate out. */}
          <Reorder.Group
            as="ul"
            axis="y"
            values={pinned}
            onReorder={onReorder}
            ref={listRef}
            aria-labelledby={listId}
            onKeyDown={onListKeyDown}
            className="relative flex flex-col gap-2"
          >
            <AnimatePresence initial={false} mode="popLayout">
              {shown.flatMap((account, index) => {
                const row = (
                  <AccountRow
                    key={account.id}
                    account={account}
                    draggable={canDrag && Boolean(account.pinned)}
                    raised={acted === account.id}
                    fresh={fresh === account.id}
                    position={pinned.indexOf(account.id)}
                    pinnedCount={pinned.length}
                    onMove={(step) => movePinnedBy(account, step)}
                    onLift={() => setActed(account.id)}
                    onDragEnd={() => endDrag(account)}
                  >
                    <AccountCard
                      account={account}
                      accounts={accounts}
                      afk={onlineAfkSession(account.username, sessions)}
                      playing={playing.includes(account.id)}
                      bracketsId={bracketsId}
                      fresh={fresh === account.id}
                      onFreshDone={() => markFresh(null)}
                      onSwitch={() => {
                        setActed(account.id);
                        keepFocus();
                        void switchAccount(queryClient, account);
                      }}
                      onRemove={() => remove(account, index)}
                      onRename={(name) => rename(account, name)}
                      onSkins={() => openAppearance(account.id)}
                      onProjectsSkin={
                        account.kind === "offline" && projectsSkins
                          ? () => openAppearance(account.id)
                          : undefined
                      }
                      onRelogin={() => openLogin(account.id)}
                      onPin={() => {
                        setActed(account.id);
                        keepFocus();
                        void pinAccount(queryClient, account, !account.pinned);
                      }}
                      onAvatar={(avatar) => {
                        void setAccountAvatar(queryClient, account, avatar).then((saved) => {
                          if (saved) announce(t("accounts.avatar.changed"));
                        });
                      }}
                    />
                  </AccountRow>
                );
                // A hairline under the pinned group, when something follows it.
                const lastPinned = index === pinned.length - 1 && index < shown.length - 1;
                return lastPinned
                  ? [
                      row,
                      <motion.li
                        key="pinned-divider"
                        aria-hidden
                        role="presentation"
                        layout="position"
                        className="mx-3 h-px bg-white/12"
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1, transition: { duration: 0.24 } }}
                        exit={{ opacity: 0, transition: { duration: 0.12 } }}
                        transition={{ layout: springSnappy }}
                      />,
                    ]
                  : [row];
              })}
            </AnimatePresence>
          </Reorder.Group>

          <div className={cn("space-y-6", accounts.length ? "mt-3" : "mt-6")}>
            {empty ? null : (
              <div className="space-y-2">
                <AfkRow onOpen={openAfk} />
                <SocialAccountRow onOpen={() => onNavigate?.()} />
                <ProjectsLinkRow />
              </div>
            )}
            <motion.div
              layout="position"
              transition={springSnappy}
              className={cn(accounts.length && "pt-1")}
            >
              <AddAccount accounts={accounts} prominent={empty} inputRef={addInput} />
            </motion.div>
            {/* With no game account yet, the AFK link waits below the add section. */}
            {empty ? <AfkRow onOpen={openAfk} /> : null}
            {empty ? <SocialAccountRow onOpen={() => onNavigate?.()} /> : null}
          </div>
        </LayoutGroup>
      </div>
    </ReducedMotionConfig>
  );
}

/** The center as a side sheet over the current page. */
export function AccountCenterSheet() {
  const { t } = useTranslation();
  const open = useAccountsUi((s) => s.centerOpen);
  const close = useAccountsUi((s) => s.closeCenter);
  const returnFocus = useAccountsUi((s) => s.returnFocus);
  const animate = useInteractionMotion();

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && close()}>
      <AnimatePresence>
        {open ? (
          <EntranceGate key="sheet">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-30 bg-obsidian-950/55"
                  {...scrimMotion(!animate)}
                />
              </Dialog.Overlay>
              <Dialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                // Start where the player is going: the name field for "Hesap ekle", otherwise the
                // active account (arrow keys move between accounts from there).
                onOpenAutoFocus={(event) => {
                  const sheet = event.currentTarget as HTMLElement;
                  const add = useAccountsUi.getState().centerFocus === "add";
                  const target = add
                    ? sheet.querySelector<HTMLElement>("[data-add-offline]")
                    : lastInputWasKeyboard()
                      ? (sheet.querySelector<HTMLElement>(
                          "[data-account-main][aria-current=true]",
                        ) ?? sheet.querySelector<HTMLElement>("[data-account-main]"))
                      : sheet;
                  if (!target) return;
                  event.preventDefault();
                  focusPanel(target);
                  if (add) {
                    setTimeout(
                      () => target.scrollIntoView({ block: "center", behavior: scrollBehavior() }),
                      320,
                    );
                  }
                }}
                onCloseAutoFocus={(event) => {
                  if (returnFocus?.isConnected) {
                    event.preventDefault();
                    returnFocus.focus();
                  }
                }}
                // Escape inside a rename cancels the rename, not the whole sheet; inside the
                // open "Projects eşlemesi" row it closes the row.
                onEscapeKeyDown={(event) => {
                  if (
                    (event.target as Element | null)?.closest?.(
                      "[data-rename-form], [data-link-form]",
                    )
                  ) {
                    event.preventDefault();
                  }
                }}
                // A press outside closes it; the undo toast does not.
                {...sheetOutside}
              >
                <GatedMotion.aside
                  className="glass-strong fixed top-0 right-0 bottom-0 z-40 flex w-[min(480px,94vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  // Slides in on the drawer curve; reduced motion: a short fade.
                  {...sheetMotion(!animate)}
                >
                  <header className="flex items-center gap-3 border-b border-white/8 py-3 pr-4 pl-6">
                    <Dialog.Title className="min-w-0 flex-1 font-display text-[22px] leading-tight font-semibold text-bone-50">
                      {t("accounts.center.title")}
                    </Dialog.Title>
                    <Dialog.Close asChild>
                      <IconButton
                        label={t("common.close")}
                        icon={<X size={18} />}
                        className="size-11"
                      />
                    </Dialog.Close>
                  </header>
                  <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-6 pt-5 pb-8">
                    <AccountCenter variant="sheet" onNavigate={close} />
                  </div>
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}
