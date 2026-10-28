// The cloud inventory as the game shows it (CAFK-FEAT §3.1; INV-UI): the window the server opened
// above the player's rows, items with their counts and wear, the game's tooltip, and the game's
// clicks: left takes or places all, right takes half or places one, Shift moves to the other part,
// Q and Ctrl+Q drop, 1–9 trade places with the hotbar on the player's own screen, Esc puts the
// cursor's item back and closes the menu. Touch: a tap is a left click, a long press opens the
// slot's actions under the item's tooltip, a carried stack waits in a hand slot under the window;
// where nothing moves (looking only, a trade lock) a tap shows the tooltip. Keyboard: the slots are grids,
// arrows move, Enter clicks, Shift+F10 opens the actions; every pick and move is spoken. The game's
// words are drawn as whole pixels (pixel-text.tsx).
//
// The frame around the game's screen (status line, close button, menus) is ours: Figtree, ash and
// gold, chamfered buttons, as wide as the game's window. The data comes from the live socket
// (projects-studio services/cloud-afk/README.md, "Inventory"); moves go out through `onOp` and the
// view waits for the frames that follow. Nothing here runs in a loop: the view moves only with the
// player and with frames.
import { ArrowUUpLeft, HandTap, Hourglass, Info, LockSimple } from "@phosphor-icons/react";
import {
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/cn";
import { Button } from "../button";
import { Menu, type MenuItem } from "../menu";
import { type MoveMessage, MoveQueue } from "./controller";
import { tooltipPlace } from "./float";
import "./inventory.css";
import {
  type Direction,
  HOTBAR_FIRST,
  neighbor,
  type Screen,
  type SlotSpot,
  screenFor,
  slotUsable,
} from "./layout";
import { HeldFrame, PanelArt, SlotFrame } from "./panel-art";
import { PixelLine } from "./pixel-text";
import {
  type PlanState,
  planClick,
  planClose,
  planDrop,
  planHold,
  planHotbarSwap,
  planPutBack,
  planQuickMove,
  planUse,
  type Step,
} from "./plan";
import { type InvState, mayPlace, quickMoveTarget, type Stack, sameItem, stateOf } from "./rules";
import { ItemArt, SlotCell } from "./slot-cell";
import { InventoryScale } from "./sprite";
import { cleanText, itemName, itemSpeech, TEXT_COLOR, titleRuns, tooltipLines } from "./text";
import { fill, INVENTORY_TEXTS, type InventoryLang, type InventoryTexts } from "./texts";
import { artBox, layoutTitle } from "./title";
import { ItemTooltip, MenuHead } from "./tooltip";
import type { InventoryMode, InvFrame, InvOp, InvResult, TitleGlyph, WinFrame } from "./types";

export interface InventoryViewProps {
  /** The latest `inv` frame; null until the first one (the slots show empty). */
  inventory: InvFrame | null;
  /** The latest open window, or null when none is open. */
  window?: WinFrame | null;
  /**
   * `full`: moves; `view`: looking only; `pick`: one of the player's own items is chosen (nothing
   * moves; a click, a tap or Enter on a filled slot of the player's inventory calls `onPick`);
   * `static`: a picture (no input, nothing live).
   */
  mode?: InventoryMode;
  /** `pick`: a filled slot of the player's inventory (5-45) was chosen. */
  onPick?: (slot: number) => void;
  /** `pick`: the chosen slot (its highlight stays, the brackets mark it). */
  picked?: number | null;
  /** An AFK+ trade holds the inventory (`invst.lock`): the view stays live, moves wait. */
  locked?: boolean;
  /** Sends a move over the live socket and resolves with the machine's answer (`invres`). */
  onOp?: (op: InvOp) => Promise<InvResult>;
  lang?: InventoryLang;
  /** Words to use instead of the built-in ones. */
  texts?: Partial<InventoryTexts>;
  /**
   * IA-ART's menu glyphs for the account's server: what a window title's private-use character
   * draws (a Projects menu's own picture) or how far it moves the title. Give it only while the
   * account is on a server the item art document names (ITEMART's host check); without it those
   * characters are left out and the title is its text. Items' pictures come from ITEMART's
   * `ItemArtProvider`.
   */
  titleGlyph?: (codePoint: number) => TitleGlyph | null | undefined;
  /** CSS pixels per GUI pixel, at most (default 3) and at least (default 1). */
  maxScale?: number;
  minScale?: number;
  /** Q, Ctrl+Q and the drop rows (default on). */
  allowDrop?: boolean;
  align?: "center" | "start";
  /**
   * `width` (default): the largest whole-pixel scale the component's width holds. `contain`: the
   * largest one its width and its height both hold, the line under the window counted; the
   * component fills its parent's height (a host with a fixed height: a panel, a dialog) and sits
   * in the middle of it (`align="start"`: at its top).
   */
  fit?: "width" | "contain";
  /** What a `static` picture is called for screen readers. */
  label?: string;
  className?: string;
}

const DOUBLE_CLICK_MS = 250;
const MESSAGE_MS = 2600;
const TAP_SLOP = 10;
const TAP_MS = 500;
/** A finger held this long on a slot opens its actions (the platforms' long press is ~500 ms). */
const LONG_PRESS_MS = 520;
/** How far past the component the pointer may go before the carried item waits out of sight. */
const AWAY_PX = 24;
/** The slot menu's own frame around its head, both sides together (its padding and border). */
const MENU_FRAME = 10;

type Input = "mouse" | "keyboard" | "touch";

interface Message {
  key: MoveMessage;
  at: number;
}

/** The line under the window: an answer, or what holds (a lock, looking only, a carried stack). */
type StatusKey = MoveMessage | "carryHint";

const STATUS_ICON: Partial<Record<StatusKey, typeof Info>> = {
  slowDown: Hourglass,
  tradeLocked: LockSimple,
  // Looking only says what it means for the player: nothing here can change (the slot shows the
  // same lock when a move is tried).
  viewOnly: LockSimple,
  carryHint: HandTap,
};

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName);
}

function cellOf(target: EventTarget | null): HTMLElement | null {
  return target instanceof Element ? (target.closest("[data-slot]") as HTMLElement | null) : null;
}

const ARROWS: Record<string, Direction> = {
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  ArrowDown: "down",
};

export function InventoryView({
  inventory,
  window: win = null,
  mode = "full",
  locked = false,
  onOp,
  onPick,
  picked = null,
  lang = "tr",
  texts: overrides,
  titleGlyph,
  maxScale = 3,
  minScale = 1,
  allowDrop = true,
  align = "center",
  fit = "width",
  label,
  className,
}: InventoryViewProps) {
  const texts = useMemo(() => ({ ...INVENTORY_TEXTS[lang], ...overrides }), [lang, overrides]);
  const live = mode !== "static";
  const canMove = mode === "full" && Boolean(onOp);
  const open = win?.open ?? null;
  const wseq = win?.wseq ?? 0;
  const openType = open?.type;
  const openSize = open?.size;
  const screen = useMemo<Screen>(
    () =>
      screenFor(openType === undefined ? null : { type: openType, size: openSize ?? 0, title: "" }),
    [openType, openSize],
  );
  const title = open ? cleanText(open.title) : "";
  // The title as the game draws it: IA-ART's pictures and moves, then its words in the game's
  // font; without glyphs its visible words; "Menü" when nothing would show.
  const titleLayout = useMemo(() => {
    const place = screen.title;
    if (!place) return null;
    // The player's own screen: "Equipment" where a window's title goes.
    if (!open) return layoutTitle([{ text: texts.equipment }], place.x, place.y);
    const plain = cleanText(open.title);
    let runs = titleGlyph ? titleRuns(open.title) : plain ? [{ text: plain }] : [];
    let layout = layoutTitle(runs, 0, place.y, titleGlyph);
    if (layout.pictures.length === 0 && layout.texts.every((t) => !t.text.trim())) {
      runs = [{ text: plain || texts.menu }];
      layout = layoutTitle(runs, 0, place.y, titleGlyph);
    }
    // The game centres a 3 x 3 window's title on the whole title's width.
    const x = place.align === "center" ? Math.floor((screen.width - layout.advance) / 2) : place.x;
    return layoutTitle(runs, x, place.y, titleGlyph);
  }, [open, screen, texts.equipment, texts.menu, titleGlyph]);

  // The art's box: the window, grown on every side to hold a menu picture that reaches past it (a
  // server's art wider than the window, the owner's "KİTAP" menu): the scale fits all of it, and
  // the window sits where the art puts it.
  const box = useMemo(() => artBox(screen, titleLayout?.pictures ?? []), [screen, titleLayout]);

  const truth = useMemo<InvState | null>(
    () => (inventory ? stateOf(inventory) : null),
    [inventory],
  );
  const winSlots = useMemo(() => {
    const map = new Map<number, Stack>();
    for (const { slot, ...stack } of win?.items ?? []) if (stack.count > 0) map.set(slot, stack);
    return map;
  }, [win]);

  // Moves on their way.
  const [, rerender] = useReducer((n: number) => n + 1, 0);
  const [message, setMessage] = useState<Message | null>(null);
  const [speech, setSpeech] = useState("");
  // A move tried where nothing moves (looking only, a trade's lock): the slot it was tried on shows a
  // lock for a moment, while the line under the window brightens.
  const [nope, setNope] = useState<{ key: string; at: number } | null>(null);
  const originRef = useRef<string | null>(null);
  const truthRef = useRef(truth);
  truthRef.current = truth;
  const onOpRef = useRef(onOp);
  onOpRef.current = onOp;
  const textsRef = useRef(texts);
  textsRef.current = texts;
  const queueRef = useRef<MoveQueue | null>(null);
  if (!queueRef.current) {
    queueRef.current = new MoveQueue({
      send: (op) =>
        onOpRef.current
          ? onOpRef.current(op)
          : Promise.resolve({ error: { code: "inventory_off", reason: "view" } }),
      truth: () => truthRef.current,
      changed: rerender,
      message: (key) => setMessage({ key, at: Date.now() }),
      speak: (key, item) => setSpeech(fill(textsRef.current[key], { item })),
      origin: (key) => {
        originRef.current = key;
      },
    });
  }
  const queue = queueRef.current;
  useEffect(() => {
    queue.revive();
    return () => queue.dispose();
  }, [queue]);
  const frameCount = useRef(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: every new frame object is a frame
  useEffect(() => {
    if (frameCount.current++ > 0) queue.frame();
  }, [inventory, win, queue]);
  useEffect(() => {
    if (!message) return;
    const timer = setTimeout(() => setMessage(null), MESSAGE_MS);
    return () => clearTimeout(timer);
  }, [message]);

  const shown = queue.shown(truth);
  const inv = shown.state;
  const pendingKeys = shown.keys;
  const cursor = inv?.cursor ?? null;
  // Where the carried item came from is kept while its pickup is on the way (a window's pickup
  // shows no cursor until its frame) and forgotten once nothing is carried or waiting.
  if (!truth?.cursor && !cursor && pendingKeys.size === 0) originRef.current = null;

  const planState: PlanState = {
    screen,
    inv: inv ?? { held: 0, cursor: null, slots: new Map() },
    win: winSlots,
    wseq,
    cursorOrigin: originRef.current,
    lang,
  };
  const planRef = useRef(planState);
  planRef.current = planState;

  const itemAt = useCallback(
    (spot: SlotSpot): Stack | null =>
      (spot.area === "inv" ? inv?.slots.get(spot.slot) : winSlots.get(spot.slot)) ?? null,
    [inv, winSlots],
  );

  const spotByKey = useMemo(() => new Map(screen.slots.map((s) => [s.key, s])), [screen]);

  const act = useCallback(
    (steps: Step[]) => {
      if (steps.length === 0) return;
      if (!canMove || locked) {
        const held: MoveMessage | null = canMove
          ? "tradeLocked"
          : mode === "view"
            ? "viewOnly"
            : null;
        if (held) {
          const at = Date.now();
          setMessage({ key: held, at });
          const key = steps[0]?.keys.find((k) => k !== "cursor");
          if (key) setNope({ key, at });
        }
        return;
      }
      queueRef.current?.run(steps);
    },
    [canMove, locked, mode],
  );

  // Focus: one tab stop, the arrows move it (a roving tab index across the grids).
  const firstKey = screen.slots.find((s) => s.section === "hotbar")?.key ?? screen.slots[0]?.key;
  const [activeKey, setActiveKey] = useState<string | undefined>(firstKey);
  const active =
    (activeKey && spotByKey.get(activeKey)) || (firstKey ? spotByKey.get(firstKey) : undefined);
  const slotsRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<Input>("mouse");
  const focusCell = useCallback((key: string, ring: boolean) => {
    setActiveKey(key);
    const el = slotsRef.current?.querySelector<HTMLElement>(`[data-slot="${key}"]`);
    el?.focus(
      ring
        ? { preventScroll: true }
        : ({ preventScroll: true, focusVisible: false } as FocusOptions),
    );
  }, []);

  // Hover, menu and touch.
  const [hoverKey, setHoverKey] = useState<string | null>(null);
  const [menuKey, setMenuKey] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  /** The open menu came from the keys: its rows show their keys. */
  const [menuKeys, setMenuKeys] = useState(false);
  /** The open menu came from a finger: its rows are a finger's height. */
  const [menuTouch, setMenuTouch] = useState(false);
  const [touchTip, setTouchTip] = useState<string | null>(null);
  const [focusInside, setFocusInside] = useState(false);
  const lastPress = useRef<{ key: string; button: 0 | 1; at: number; picked: boolean } | null>(
    null,
  );
  const touch = useRef<{
    key: string;
    x: number;
    y: number;
    at: number;
    id: number;
    timer: ReturnType<typeof setTimeout> | null;
    long: boolean;
  } | null>(null);
  const endTouch = useCallback(() => {
    if (touch.current?.timer) clearTimeout(touch.current.timer);
    touch.current = null;
  }, []);
  useEffect(() => endTouch, [endTouch]);
  const pointer = useRef<{ x: number; y: number } | null>(null);

  // The slot's actions (long press, Shift+F10, the menu key). On the player's own screen they are
  // the game's clicks by what they do: take, half, the shift-click named by where the stack goes,
  // hold, use. With a window open the server decides what a click does (a shop buys, a menu opens
  // another), so the rows name the clicks themselves. Dropping comes apart, only from the player's
  // own slots (Q still drops from a window). Opened with the keys, each row shows its key.
  const menuItems = useCallback(
    (spot: SlotSpot | undefined, keys: boolean): Array<MenuItem | null> => {
      if (!spot || !canMove || locked || !slotUsable(screen, spot)) return [];
      const state = planRef.current;
      const here = spot.area === "inv" ? state.inv.slots.get(spot.slot) : state.win.get(spot.slot);
      const carried = state.inv.cursor;
      const run = (plan: (s: PlanState) => Step[]) => () => act(plan(planRef.current));
      const key = (shortcut: string) => (keys ? shortcut : undefined);
      const items: Array<MenuItem | null> = [];
      if (screen.kind !== "player") {
        if (here || carried) {
          items.push(
            {
              label: texts.click,
              shortcut: key("Enter"),
              onSelect: run((s) => planClick(s, spot, 0)),
            },
            { label: texts.rightClick, onSelect: run((s) => planClick(s, spot, 1)) },
          );
        }
        if (here && !carried) {
          items.push({
            label: texts.shiftClick,
            shortcut: key("Shift+Enter"),
            onSelect: run((s) => planQuickMove(s, spot)),
          });
        }
        if (carried) {
          items.push({ label: texts.putBack, shortcut: key("Esc"), onSelect: run(planPutBack) });
        }
      } else if (carried) {
        // An armour slot takes only its own armour.
        if (mayPlace(spot.slot, carried)) {
          const joins = !here || sameItem(here, carried);
          items.push({
            label: joins ? texts.place : texts.swap,
            shortcut: key("Enter"),
            onSelect: run((s) => planClick(s, spot, 0)),
          });
          if (joins) {
            items.push({ label: texts.placeOne, onSelect: run((s) => planClick(s, spot, 1)) });
          }
        }
        items.push({ label: texts.putBack, shortcut: key("Esc"), onSelect: run(planPutBack) });
      } else {
        if (here) {
          const moveLabel = {
            inventory: texts.toInventory,
            hotbar: texts.toHotbar,
            equipment: texts.putOn,
            offhand: texts.toOffhand,
          }[quickMoveTarget(state.inv, spot.slot) ?? "inventory"];
          items.push({
            label: texts.take,
            shortcut: key("Enter"),
            onSelect: run((s) => planClick(s, spot, 0)),
          });
          if (here.count > 1) {
            items.push({ label: texts.takeHalf, onSelect: run((s) => planClick(s, spot, 1)) });
          }
          items.push({
            label: moveLabel,
            shortcut: key("Shift+Enter"),
            onSelect: run((s) => planQuickMove(s, spot)),
          });
        }
        const hotbar = spot.kind === "hotbar" ? spot.slot - HOTBAR_FIRST : -1;
        if (hotbar >= 0) {
          if (state.inv.held !== hotbar) {
            items.push({ label: texts.hold, onSelect: run((s) => planHold(s, hotbar)) });
          }
          if (here) items.push({ label: texts.use, onSelect: run((s) => planUse(s, hotbar)) });
        }
      }
      if (here && !carried && allowDrop && spot.area === "inv") {
        if (items.length > 0) items.push(null);
        if (here.count > 1) {
          items.push({
            label: texts.dropOne,
            danger: true,
            shortcut: key("Q"),
            onSelect: run((s) => planDrop(s, spot, false)),
          });
          items.push({
            label: texts.dropStack,
            danger: true,
            shortcut: key("Ctrl+Q"),
            onSelect: run((s) => planDrop(s, spot, true)),
          });
        } else {
          items.push({
            label: texts.drop,
            danger: true,
            shortcut: key("Q"),
            onSelect: run((s) => planDrop(s, spot, false)),
          });
        }
      }
      return items;
    },
    [act, allowDrop, canMove, locked, screen, texts],
  );

  /**
   * Opens the slot's actions under it; a slot with nothing to offer opens nothing. The menu keeps
   * within the game's window sideways (8 px from the screen's edge at least) and 16 px from the
   * screen's top and bottom.
   */
  const [menuPad, setMenuPad] = useState<
    number | Record<"top" | "right" | "bottom" | "left", number>
  >(16);
  const openMenu = useCallback(
    (spot: SlotSpot) => {
      if (menuItems(spot, false).length === 0) return false;
      const panel = panelRef.current?.getBoundingClientRect();
      if (panel) {
        const width = document.documentElement.clientWidth;
        setMenuPad({
          top: 16,
          bottom: 16,
          left: Math.max(8, Math.round(panel.left)),
          right: Math.max(8, Math.round(width - panel.right)),
        });
      }
      setMenuKey(spot.key);
      setMenuKeys(inputRef.current === "keyboard");
      setMenuTouch(inputRef.current === "touch");
      setTouchTip(null);
      setMenuOpen(true);
      return true;
    },
    [menuItems],
  );

  const press = useCallback(
    (spot: SlotSpot, button: 0 | 1, shift: boolean) => {
      const state = planRef.current;
      if (mode === "pick") {
        // A pick takes one of the player's own items; nothing moves, a window's slots are not theirs.
        if (spot.area === "inv" && state.inv.slots.get(spot.slot)) onPick?.(spot.slot);
        return;
      }
      if (!slotUsable(screen, spot)) return;
      const now = performance.now();
      const last = lastPress.current;
      // A double click collects in the game; the contract has no such move, so the second press
      // of a double click does nothing (the stack stays on the cursor, as after a single click).
      if (
        last &&
        !shift &&
        last.key === spot.key &&
        last.button === button &&
        last.picked &&
        now - last.at < DOUBLE_CLICK_MS
      ) {
        lastPress.current = null;
        return;
      }
      const here = spot.area === "inv" ? state.inv.slots.get(spot.slot) : state.win.get(spot.slot);
      lastPress.current = {
        key: spot.key,
        button,
        at: now,
        picked: !shift && !state.inv.cursor && Boolean(here),
      };
      act(shift ? planQuickMove(state, spot) : planClick(state, spot, button));
    },
    [act, mode, onPick, screen],
  );

  const keyAction = useCallback(
    (
      spot: SlotSpot,
      event: { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean },
    ) => {
      const state = planRef.current;
      const key = event.key;
      if ((key === "q" || key === "Q") && !event.altKey) {
        if (!allowDrop) return false;
        act(planDrop(state, spot, event.ctrlKey || event.metaKey));
        return true;
      }
      if (/^[1-9]$/.test(key) && !event.ctrlKey && !event.metaKey && !event.altKey) {
        act(planHotbarSwap(state, spot, Number(key) - 1));
        return true;
      }
      return false;
    },
    [act, allowDrop],
  );

  const onKeyDown = useCallback(
    (event: ReactKeyboardEvent<HTMLDivElement>) => {
      // Q and the number keys act on the slot under the mouse when the mouse moved last (the game).
      const hovered =
        inputRef.current === "mouse" && hoverKey ? spotByKey.get(hoverKey) : undefined;
      inputRef.current = "keyboard";
      const cell = cellOf(event.target);
      const spot = cell ? spotByKey.get(cell.dataset.slot ?? "") : undefined;
      if (!spot) return;
      if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
        event.preventDefault();
        openMenu(spot);
        return;
      }
      const direction =
        ARROWS[event.key] ??
        (event.key === "Home" ? (event.ctrlKey ? "first" : "home") : undefined) ??
        (event.key === "End" ? (event.ctrlKey ? "last" : "end") : undefined);
      if (direction) {
        event.preventDefault();
        focusCell(neighbor(screen.slots, spot, direction).key, true);
        return;
      }
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        if (!event.repeat) press(spot, 0, event.shiftKey);
        return;
      }
      if (event.key === "Escape") {
        const state = planRef.current;
        if (state.inv.cursor) {
          event.preventDefault();
          event.stopPropagation();
          act(planPutBack(state));
        } else if (state.wseq !== 0 && canMove) {
          event.preventDefault();
          event.stopPropagation();
          act(planClose(state));
        }
        return;
      }
      if (event.repeat) return;
      if (keyAction(hovered ?? spot, event)) event.preventDefault();
    },
    [act, canMove, focusCell, hoverKey, keyAction, openMenu, press, screen, spotByKey],
  );

  // Q and the number keys over a hovered slot while the focus is elsewhere on the page.
  useEffect(() => {
    if (!live || !hoverKey) return;
    const onDocKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.repeat || isEditable(event.target)) return;
      if (slotsRef.current?.contains(event.target as Node)) return;
      if (document.querySelector("[role=dialog] [role=menu], [data-radix-popper-content-wrapper]"))
        return;
      const spot = spotByKey.get(hoverKey);
      if (spot && keyAction(spot, event)) event.preventDefault();
    };
    document.addEventListener("keydown", onDocKey);
    return () => document.removeEventListener("keydown", onDocKey);
  }, [hoverKey, keyAction, live, spotByKey]);

  const onPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const cell = cellOf(event.target);
      const spot = cell ? spotByKey.get(cell.dataset.slot ?? "") : undefined;
      if (!spot) return;
      if (event.pointerType === "mouse") {
        inputRef.current = "mouse";
        if (event.button !== 0 && event.button !== 2) return;
        event.preventDefault();
        pointer.current = { x: event.clientX, y: event.clientY };
        focusCell(spot.key, false);
        press(spot, event.button === 2 ? 1 : 0, event.shiftKey);
        return;
      }
      inputRef.current = "touch";
      endTouch();
      const start = {
        key: spot.key,
        x: event.clientX,
        y: event.clientY,
        at: performance.now(),
        id: event.pointerId,
        timer: null as ReturnType<typeof setTimeout> | null,
        long: false,
      };
      touch.current = start;
      // A finger that stays opens the slot's actions under it (nothing for an empty slot).
      if (menuItems(spot, false).length > 0) {
        start.timer = setTimeout(() => {
          start.timer = null;
          if (touch.current !== start) return;
          start.long = true;
          focusCell(spot.key, false);
          openMenu(spot);
        }, LONG_PRESS_MS);
      }
    },
    [endTouch, focusCell, menuItems, openMenu, press, spotByKey],
  );

  const onPointerMove = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const start = touch.current;
      if (!start || start.id !== event.pointerId) return;
      // A finger that travels is scrolling the page: neither a tap nor a long press.
      if (Math.hypot(event.clientX - start.x, event.clientY - start.y) > TAP_SLOP) endTouch();
    },
    [endTouch],
  );

  const onPointerUp = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const start = touch.current;
      if (!start || start.id !== event.pointerId) return;
      endTouch();
      if (start.long || menuOpen) return;
      const moved = Math.hypot(event.clientX - start.x, event.clientY - start.y);
      if (moved > TAP_SLOP || performance.now() - start.at > TAP_MS) return;
      const spot = spotByKey.get(start.key);
      if (!spot) return;
      focusCell(spot.key, false);
      if (mode === "pick") {
        press(spot, 0, false);
        return;
      }
      // Where nothing moves, a tap is the touch screen's hover: the slot's tooltip, on and off. In a
      // trade, a tap that would move something brightens the lock's line too.
      if (!canMove || locked) {
        setTouchTip((current) => (current === spot.key ? null : spot.key));
        if (canMove && planClick(planRef.current, spot, 0).length > 0) {
          const at = Date.now();
          setMessage({ key: "tradeLocked", at });
          setNope({ key: spot.key, at });
        }
        return;
      }
      press(spot, 0, false);
    },
    [canMove, endTouch, focusCell, locked, menuOpen, mode, press, spotByKey],
  );

  // Never the browser's menu: a mouse right click is the game's right click (done on press), a
  // long press is ours (above), and the keys open ours (Shift+F10, the menu key) on their own.
  const onContextMenu = useCallback(
    (event: ReactMouseEvent<HTMLDivElement>) => {
      event.preventDefault();
      if (inputRef.current !== "keyboard") return;
      const cell = cellOf(event.target);
      const spot = cell ? spotByKey.get(cell.dataset.slot ?? "") : undefined;
      if (spot && !menuOpen) openMenu(spot);
    },
    [menuOpen, openMenu, spotByKey],
  );

  // Hover: the slot under a mouse (for the tooltip; the highlight is CSS).
  const onPointerOver = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.pointerType !== "mouse") return;
    pointer.current = { x: event.clientX, y: event.clientY };
    inputRef.current = "mouse";
    const key = cellOf(event.target)?.dataset.slot ?? null;
    setHoverKey((current) => (current === key ? current : key));
  }, []);
  const onPointerLeave = useCallback(() => setHoverKey(null), []);

  // A press anywhere outside the game's window with an item on the cursor puts it back (the game
  // would drop it); a press outside the slots hides a tapped tooltip.
  const panelRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!live || (!cursor && !touchTip)) return;
    const onDocDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (touchTip && !slotsRef.current?.contains(target)) setTouchTip(null);
      if (!planRef.current.inv.cursor || event.button !== 0) return;
      if (panelRef.current?.contains(target)) return;
      const element = target instanceof Element ? target : null;
      if (element?.closest("[role=menu]")) return;
      if (element && rootRef.current?.contains(element) && element.closest("button, a")) return;
      act(planPutBack(planRef.current));
    };
    document.addEventListener("pointerdown", onDocDown, true);
    return () => document.removeEventListener("pointerdown", onDocDown, true);
  }, [act, cursor, live, touchTip]);

  // Window opened and closed, for screen readers.
  const lastWindow = useRef<{ wseq: number; open: boolean } | null>(null);
  useEffect(() => {
    const previous = lastWindow.current;
    lastWindow.current = { wseq, open: Boolean(open) };
    if (!previous || !live) return;
    if (open && previous.wseq !== wseq) {
      setSpeech(fill(textsRef.current.menuOpened, { title: title || textsRef.current.menu }));
    } else if (!open && previous.open) {
      setSpeech(textsRef.current.menuClosed);
    }
  }, [wseq, open, title, live]);

  // The focused slot went away with the window: the first slot of the new screen takes the stop.
  useEffect(() => {
    if (activeKey && spotByKey.has(activeKey)) return;
    setActiveKey(firstKey);
    if (focusInside && firstKey) focusCell(firstKey, inputRef.current === "keyboard");
  }, [activeKey, firstKey, focusCell, focusInside, spotByKey]);

  // Pixels: the exact screen ratio once the script runs. The browser paints the panel's box on whole
  // device pixels itself; a translate to correct a half pixel left the art half a pixel off its
  // edges instead (measured at 1x and 1.5x), so none is added.
  const probeRef = useRef<HTMLSpanElement>(null);
  const [dpr, setDpr] = useState<number | null>(null);
  const [px, setPx] = useState("2px");
  const [mounted, setMounted] = useState(false);
  useEffect(() => {
    setMounted(true);
    if (typeof window === "undefined") return;
    let query: MediaQueryList | null = null;
    const update = () => {
      setDpr(window.devicePixelRatio || 1);
      query?.removeEventListener("change", update);
      query = window.matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
      query.addEventListener("change", update);
    };
    update();
    return () => query?.removeEventListener("change", update);
  }, []);
  const measure = useCallback(() => {
    const probe = probeRef.current;
    if (!probe) return;
    const unit = probe.getBoundingClientRect().width / 100;
    if (unit > 0) setPx((current) => (current === `${unit}px` ? current : `${unit}px`));
  }, []);
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measure when the size inputs change
  useLayoutEffect(() => {
    measure();
  }, [measure, dpr, box.width, box.height, maxScale, minScale]);
  useEffect(() => {
    const root = rootRef.current;
    if (!root || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => measure());
    observer.observe(root);
    window.addEventListener("resize", measure);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [measure]);

  // The tooltip: under the mouse; beside the focused slot for keys; a tapped slot's where nothing
  // moves; never while the slot's menu is open (the menu heads with it) or an item is carried.
  const carrying = Boolean(cursor);
  // Under a finger nothing follows the carried stack: it waits in a hand slot under the window, with
  // what a tap does now, and the grid shows only what its slots hold.
  const touchCarry = carrying && live && inputRef.current === "touch";
  const tipKey = menuOpen
    ? null
    : inputRef.current === "mouse"
      ? hoverKey
      : inputRef.current === "touch"
        ? touchTip
        : focusInside
          ? (activeKey ?? null)
          : null;
  const tipSpot = tipKey ? spotByKey.get(tipKey) : undefined;
  const tipItem = live && !carrying && tipSpot ? itemAt(tipSpot) : null;
  const tipLines = useMemo(
    () => (tipItem ? tooltipLines(tipItem, lang, texts) : []),
    [tipItem, lang, texts],
  );
  const [tipShownOnce, setTipShownOnce] = useState(false);
  useEffect(() => {
    if (!tipItem) {
      const timer = setTimeout(() => setTipShownOnce(false), 300);
      return () => clearTimeout(timer);
    }
    const timer = setTimeout(() => setTipShownOnce(true), 100);
    return () => clearTimeout(timer);
  }, [tipItem]);

  // The cursor's item and the tooltip follow the mouse, or sit by the slot for keys and touch: the
  // game's offsets (the tooltip's words 12 right of and 12 above the pointer, to its left when they
  // would leave the screen; the item centred on it).
  const cursorEl = useRef<HTMLDivElement>(null);
  const tipEl = useRef<HTMLDivElement>(null);
  const frame = useRef(0);
  const placeFloating = useCallback(() => {
    frame.current = 0;
    const unit = Number.parseFloat(px) || 2;
    const ratio = window.devicePixelRatio || 1;
    const snap = (v: number) => Math.round(v * ratio) / ratio;
    const viaMouse = inputRef.current === "mouse" && pointer.current !== null;
    let anchor: { x: number; y: number } | null = viaMouse ? pointer.current : null;
    let slot: DOMRect | null = null;
    if (!anchor) {
      const cell = activeKey
        ? slotsRef.current?.querySelector<HTMLElement>(`[data-slot="${activeKey}"]`)
        : null;
      slot = cell?.getBoundingClientRect() ?? null;
      if (slot) anchor = { x: slot.left + slot.width / 2, y: slot.top + slot.height / 2 };
    }
    if (!anchor) return;
    const cursorNode = cursorEl.current;
    if (cursorNode) {
      // Under the keys the carried stack rests on the focused slot, raised a little: its count
      // stays clear of the neighbours'. Under a finger it rests in its slot.
      const half = 9 * unit;
      const lift = viaMouse || inputRef.current === "touch" ? 0 : 3 * unit;
      cursorNode.style.transform = `translate(${snap(anchor.x - half)}px, ${snap(anchor.y - half - lift)}px)`;
      const root = rootRef.current?.getBoundingClientRect();
      const away =
        viaMouse &&
        root !== undefined &&
        (anchor.x < root.left - AWAY_PX ||
          anchor.x > root.right + AWAY_PX ||
          anchor.y < root.top - AWAY_PX ||
          anchor.y > root.bottom + AWAY_PX);
      cursorNode.toggleAttribute("data-away", away);
    }
    const tipNode = tipEl.current;
    if (tipNode) {
      // The words' box: the node less its padding and border (3 GUI pixels a side).
      const size = tipNode.getBoundingClientRect();
      const { x, y } = tooltipPlace(
        slot
          ? { slot, by: inputRef.current === "touch" ? "touch" : "keyboard" }
          : { pointer: anchor },
        { width: size.width - 6 * unit, height: size.height - 6 * unit },
        {
          width: document.documentElement.clientWidth,
          height: document.documentElement.clientHeight,
        },
        unit,
      );
      tipNode.style.transform = `translate(${snap(x - 3 * unit)}px, ${snap(y - 3 * unit)}px)`;
    }
  }, [activeKey, px]);
  const schedule = useCallback(() => {
    if (!frame.current) frame.current = requestAnimationFrame(placeFloating);
  }, [placeFloating]);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  useEffect(() => {
    if (!live || (!carrying && !tipItem)) return;
    const onMove = (event: PointerEvent) => {
      if (event.pointerType !== "mouse") return;
      pointer.current = { x: event.clientX, y: event.clientY };
      inputRef.current = "mouse";
      schedule();
    };
    document.addEventListener("pointermove", onMove, { passive: true });
    window.addEventListener("scroll", schedule, { capture: true, passive: true });
    return () => {
      document.removeEventListener("pointermove", onMove);
      window.removeEventListener("scroll", schedule, { capture: true });
    };
  }, [carrying, live, schedule, tipItem]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: place again when what floats changes
  useLayoutEffect(() => {
    if (carrying || tipItem) placeFloating();
  }, [carrying, tipItem, tipLines, activeKey, px, placeFloating]);

  // Labels.
  const descId = useId();
  const statusId = useId();
  const labelOf = (spot: SlotSpot, item: Stack | null): string => {
    const parts: string[] = [];
    if (item) parts.push(itemSpeech(item, lang, texts));
    else {
      const name =
        spot.kind === "head" ||
        spot.kind === "chest" ||
        spot.kind === "legs" ||
        spot.kind === "feet" ||
        spot.kind === "offhand"
          ? texts[spot.kind]
          : "";
      parts.push(name ? `${name}, ${texts.empty.toLocaleLowerCase(lang)}` : texts.empty);
    }
    if (
      spot.area === "inv" &&
      spot.kind === "hotbar" &&
      inv?.held === spot.slot - HOTBAR_FIRST &&
      item
    ) {
      parts.push(texts.inHand);
    }
    if (pendingKeys.has(spot.key)) parts.push(texts.pending);
    return parts.join(", ");
  };
  const activeItem = active ? itemAt(active) : null;
  const description = activeItem
    ? tooltipLines(activeItem, lang, texts)
        .slice(1)
        .map((l) => l.text)
        .join(". ")
    : "";

  const filled = useMemo(() => {
    const set = new Set<string>();
    for (const spot of screen.slots) if (itemAt(spot)) set.add(spot.key);
    return set;
  }, [screen, itemAt]);

  const sectionLabel = (id: string) =>
    id === "window"
      ? title || texts.menu
      : id === "equipment"
        ? texts.equipment
        : id === "hotbar"
          ? texts.hotbar
          : texts.inventory;

  const heldSpot = inv ? spotByKey.get(`inv:${HOTBAR_FIRST + inv.held}`) : undefined;

  // The line under the window: the last answer for a moment, else what holds while it holds (a
  // trade's lock, looking only, a stack under a finger). A move tried under a line that holds
  // brightens that line once.
  const holdKey: StatusKey | null = !live
    ? null
    : locked
      ? "tradeLocked"
      : mode === "view"
        ? "viewOnly"
        : touchCarry && canMove
          ? "carryHint"
          : null;
  const statusKey: StatusKey | null = message?.key ?? holdKey;
  const flashAt = message && message.key === holdKey ? message.at : null;
  const StatusIcon = statusKey ? (STATUS_ICON[statusKey] ?? Info) : Info;

  // The slot's menu: what it acts on heads it, at twice the game's pixel on whole device pixels.
  const menuSpot = menuKey ? spotByKey.get(menuKey) : undefined;
  const items = useMemo(() => menuItems(menuSpot, menuKeys), [menuItems, menuSpot, menuKeys]);
  // The slot's last action went away under the menu (a frame emptied it, a trade locked it).
  useEffect(() => {
    if (menuOpen && items.length === 0) setMenuOpen(false);
  }, [items, menuOpen]);
  const menuItem = menuSpot && !cursor ? itemAt(menuSpot) : null;
  const menuLines = useMemo(
    () => (menuItem ? tooltipLines(menuItem, lang, texts) : []),
    [menuItem, lang, texts],
  );
  const headRatio = dpr ?? 1;
  const headUnit = Math.max(1, Math.round(2 * headRatio)) / headRatio;
  const headPx = `${headUnit}px`;
  const menuLabel = menuItem
    ? itemName(menuItem, lang)
    : cursor
      ? fill(texts.onCursor, { item: itemName(cursor, lang) })
      : undefined;

  const rootStyle = {
    "--pl-inv-w": box.width,
    // For `fit="contain"`: the art's height, and the line under the window (its gap and its
    // height, as tall as the hand slot) in CSS pixels.
    "--pl-inv-h": box.height,
    "--pl-inv-under": live ? `${8 + Math.max(32, 18 * headUnit)}px` : "0px",
    "--pl-inv-min": minScale,
    "--pl-inv-max": maxScale,
    ...(dpr ? { "--pl-inv-dpr": dpr } : {}),
  } as CSSProperties;
  const stageStyle = { "--pl-inv-gw": box.width, "--pl-inv-gh": box.height } as CSSProperties;
  const at = (x: number) => `calc(${x} * var(--pl-inv-px))`;

  const slotsLayer = (
    // The grids' cells are the focusable controls; the layer takes their pointer and key events
    // in one place (one listener for up to 90 slots).
    // biome-ignore lint/a11y/noStaticElementInteractions: events of the gridcells, delegated
    <div
      ref={slotsRef}
      className="pl-inv-slots"
      style={{
        left: at(-box.left),
        top: at(-box.top),
        width: at(screen.width),
        height: at(screen.height),
      }}
      onPointerDown={live ? onPointerDown : undefined}
      onPointerMove={live ? onPointerMove : undefined}
      onPointerUp={live ? onPointerUp : undefined}
      onPointerCancel={live ? endTouch : undefined}
      onPointerOver={live ? onPointerOver : undefined}
      onPointerLeave={live ? onPointerLeave : undefined}
      onContextMenu={live ? onContextMenu : undefined}
      onKeyDown={live ? onKeyDown : undefined}
      onFocus={live ? () => setFocusInside(true) : undefined}
      onBlur={
        live
          ? (event) => {
              if (!event.currentTarget.contains(event.relatedTarget as Node)) setFocusInside(false);
            }
          : undefined
      }
    >
      {screen.sections.map((section) => (
        // biome-ignore lint/a11y/useSemanticElements: an ARIA grid of positioned slots; no native element is one
        <div key={section.id} role="grid" aria-label={sectionLabel(section.id)}>
          {section.rows.map((row) => {
            const rowX = (row[0]?.x ?? 0) - 1;
            const rowY = (row[0]?.y ?? 0) - 1;
            return (
              // biome-ignore lint/a11y/useSemanticElements: a row of the ARIA grid above
              // biome-ignore lint/a11y/useFocusableInteractive: its cells hold the focus (a roving tab stop)
              <div
                key={`${section.id}-${rowY}`}
                role="row"
                className="pl-inv-row"
                style={{
                  left: at(rowX),
                  top: at(rowY),
                  width: at((row[row.length - 1]?.x ?? 0) + 17 - rowX),
                }}
              >
                {row.map((spot) => {
                  const item = itemAt(spot);
                  const isActive = live && active?.key === spot.key;
                  return (
                    <SlotCell
                      key={spot.key}
                      spot={spot}
                      left={spot.x - 1 - rowX}
                      item={item}
                      label={labelOf(spot, item)}
                      describedBy={isActive && description ? descId : undefined}
                      tabbable={isActive}
                      pending={pendingKeys.has(spot.key)}
                      held={Boolean(heldSpot && heldSpot.key === spot.key)}
                      lit={(menuOpen && menuKey === spot.key) || touchTip === spot.key}
                      nopeAt={nope?.key === spot.key ? nope.at : undefined}
                      live={live}
                      selectable={mode === "pick" && spot.area === "inv"}
                      picked={mode === "pick" && spot.area === "inv" && picked === spot.slot}
                    />
                  );
                })}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );

  return (
    <InventoryScale.Provider value={Number.parseFloat(px) || 2}>
      {/* biome-ignore lint/a11y/useSemanticElements: a named group of grids, not a form's fieldset */}
      <div
        ref={rootRef}
        className={cn("pl-inv", className)}
        data-align={align}
        data-fit={fit === "contain" ? "contain" : undefined}
        data-live={live ? "" : undefined}
        style={rootStyle}
        role="group"
        aria-label={
          live
            ? open
              ? title || texts.menu
              : texts.inventory
            : (label ?? (title || texts.inventory))
        }
      >
        <div className="pl-inv-stage" style={stageStyle} aria-hidden={live ? undefined : true}>
          <div className="pl-inv-column">
            <div ref={panelRef} className="pl-inv-panel">
              <span
                ref={probeRef}
                aria-hidden
                style={{ position: "absolute", width: at(100), height: 0, visibility: "hidden" }}
              />
              <div
                style={{
                  position: "absolute",
                  left: at(-box.left),
                  top: at(-box.top),
                  width: at(screen.width),
                  height: at(screen.height),
                }}
              >
                <PanelArt screen={screen} filled={filled} />
              </div>
              {titleLayout?.pictures.map((picture, i) => (
                <span
                  // biome-ignore lint/suspicious/noArrayIndexKey: a title's pictures are its order
                  key={i}
                  className="pl-inv-glyph"
                  aria-hidden
                  style={{
                    left: at(picture.x - box.left),
                    top: at(picture.y - box.top),
                    width: at(picture.width),
                    height: at(picture.height),
                    backgroundImage: `url("${picture.src.replace(/"/g, "%22")}")`,
                  }}
                />
              ))}
              {titleLayout?.texts.map((part, i) => (
                <PixelLine
                  // biome-ignore lint/suspicious/noArrayIndexKey: a title's words are its order
                  key={i}
                  className="pl-inv-title"
                  text={part.text}
                  color={TEXT_COLOR.title}
                  // A plain title stops at the window's edge; a menu's own words go where the
                  // server put them.
                  maxWidth={
                    titleLayout.pictures.length === 0
                      ? Math.max(0, screen.width - 8 - part.x)
                      : undefined
                  }
                  style={{
                    left: at(part.x - box.left),
                    top: at((screen.title?.y ?? 0) - box.top),
                  }}
                />
              ))}
              {screen.inventoryLabel ? (
                <PixelLine
                  className="pl-inv-title"
                  text={texts.inventory}
                  color={TEXT_COLOR.title}
                  style={{
                    left: at(screen.inventoryLabel.x - box.left),
                    top: at(screen.inventoryLabel.y - box.top),
                  }}
                />
              ) : null}
              {slotsLayer}
              {live && canMove ? (
                <Menu
                  open={menuOpen && items.length > 0}
                  onOpenChange={setMenuOpen}
                  // A hidden trigger over the slot: the menu opens under it (or above, near the
                  // window's bottom) and slides sideways to stay on screen.
                  trigger={
                    <span
                      className="pl-inv-menu-anchor"
                      aria-hidden
                      style={
                        menuSpot
                          ? {
                              left: at(menuSpot.x - 1 - box.left),
                              top: at(menuSpot.y - 1 - box.top),
                              width: at(18),
                              height: at(18),
                            }
                          : undefined
                      }
                    />
                  }
                  items={items}
                  header={
                    menuLines.length > 0 ? (
                      <MenuHead
                        lines={menuLines}
                        px={headPx}
                        // The window's width less the menu's own frame (its padding and border).
                        room={Math.floor(screen.width * (Number.parseFloat(px) || 2)) - MENU_FRAME}
                      />
                    ) : undefined
                  }
                  label={menuLabel}
                  // Under a finger its rows are a finger's height (44 px).
                  className={menuTouch ? "pl-inv-menu pl-inv-menu-touch" : "pl-inv-menu"}
                  side="bottom"
                  align="start"
                  sideOffset={4}
                  collisionPadding={menuPad}
                  onCloseAutoFocus={(event) => {
                    event.preventDefault();
                    if (inputRef.current === "keyboard" && menuKey) focusCell(menuKey, true);
                  }}
                />
              ) : null}
              {heldSpot ? <HeldFrame x={heldSpot.x - box.left} y={heldSpot.y - box.top} /> : null}
            </div>

            {live ? (
              <div
                className="pl-inv-bar"
                // As tall as the hand slot, so carrying a stack never moves what is below; under
                // the window itself when a server's art is wider.
                style={{
                  minHeight: `max(32px, calc(18 * ${headPx}))`,
                  marginLeft: at(-box.left),
                  width: at(screen.width),
                }}
              >
                <div className="flex min-w-0 items-center gap-2">
                  {touchCarry && cursor ? (
                    <InventoryScale.Provider value={headUnit}>
                      {/* What the finger carries; a tap on it puts it back where it came from. */}
                      <button
                        type="button"
                        className="pl-inv-hand"
                        data-pending={pendingKeys.has("cursor") ? "" : undefined}
                        style={{ "--pl-inv-px": headPx } as CSSProperties}
                        aria-label={`${texts.putBack}: ${itemName(cursor, lang)}`}
                        onClick={() => act(planPutBack(planRef.current))}
                      >
                        <SlotFrame />
                        <ItemArt item={cursor} />
                      </button>
                    </InventoryScale.Provider>
                  ) : null}
                  <div className="relative min-w-0">
                    <p id={statusId} role="status" className="sr-only">
                      {statusKey ? texts[statusKey] : ""}
                    </p>
                    <p
                      aria-hidden
                      className={cn(
                        "flex min-w-0 items-center text-meta text-ash-300 transition-opacity duration-150",
                        statusKey ? "opacity-100" : "opacity-0",
                      )}
                    >
                      {statusKey ? (
                        <span
                          key={flashAt ?? "rest"}
                          className={cn(
                            "flex min-w-0 items-center gap-1.5",
                            flashAt !== null && "pl-inv-dim",
                          )}
                        >
                          {touchCarry ? null : (
                            <StatusIcon size={14} weight="bold" className="shrink-0 text-ash-400" />
                          )}
                          <span className="truncate">{texts[statusKey]}</span>
                        </span>
                      ) : null}
                    </p>
                    {statusKey && flashAt !== null ? (
                      // The same line in bone, crossing back to ash: the move was tried and held.
                      <p
                        key={flashAt}
                        aria-hidden
                        className="pl-inv-flash flex min-w-0 items-center gap-1.5 text-meta text-bone-100"
                      >
                        <StatusIcon size={14} weight="bold" className="shrink-0" />
                        <span className="truncate">{texts[statusKey]}</span>
                      </p>
                    ) : null}
                  </div>
                </div>
                {touchCarry && canMove ? (
                  // Under a finger the stack in the hand has its way back in words too (the hand
                  // slot itself puts it back as well); the window's close button returns after.
                  // Both stay in place, unavailable, while a slot's menu is open.
                  <Button
                    variant="ghost"
                    size="sm"
                    className="-mr-3 shrink-0"
                    disabled={menuOpen}
                    icon={<ArrowUUpLeft size={14} weight="bold" aria-hidden />}
                    onClick={() => act(planPutBack(planRef.current))}
                  >
                    {texts.putBack}
                  </Button>
                ) : open && canMove ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    // The label's edge on the window's edge; the hover fill reaches past it. No
                    // cross: beside a dialog's own one it read as closing the dialog (SITE-W2).
                    className="-mr-3 shrink-0"
                    disabled={locked || menuOpen}
                    onClick={() => {
                      const state = planRef.current;
                      if (state.inv.cursor) {
                        const back = planPutBack(state);
                        const close: Step = { op: { action: "close", wseq: state.wseq }, keys: [] };
                        act(back.length ? [{ ...back[0], expectCursor: null } as Step, close] : []);
                      } else {
                        act(planClose(state));
                      }
                    }}
                  >
                    {texts.closeMenu}
                  </Button>
                ) : null}
              </div>
            ) : null}
          </div>
        </div>

        {live ? (
          <>
            <span className="sr-only" aria-live="polite">
              {speech}
            </span>
            <span id={descId} className="sr-only">
              {description}
            </span>
          </>
        ) : null}

        {live && mounted
          ? createPortal(
              <>
                {cursor && !touchCarry ? (
                  <div
                    ref={cursorEl}
                    className="pl-inv-cursor"
                    data-pending={pendingKeys.has("cursor") ? "" : undefined}
                    style={{ "--pl-inv-px": px } as CSSProperties}
                    aria-hidden
                  >
                    <ItemArt item={cursor} />
                  </div>
                ) : null}
                <ItemTooltip
                  ref={tipEl}
                  lines={tipLines}
                  shown={Boolean(tipItem)}
                  instant={tipShownOnce}
                  px={px}
                />
              </>,
              document.body,
            )
          : null}
      </div>
    </InventoryScale.Provider>
  );
}

export type { Stack as InventoryStack };
