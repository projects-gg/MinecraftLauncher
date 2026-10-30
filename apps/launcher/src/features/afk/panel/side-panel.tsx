// The AFK account panel's sheet (AFK-UX; owner, 30 Oct: "AFK paneli sağdan ortaya kadar açılıyor
// … sürükleyerek tam açabilmeli ya da kapatabilmeli"): a right sheet the player sizes by its left
// edge. It rests at today's width or across the whole window; dragging the edge follows the
// pointer (wider: the panel grows; narrower than today's width: it slides away) and lets go onto
// the nearest place, a flick deciding on its own, and far enough right it closes. The edge's keys
// move between the two sizes (arrows, Home, End, Enter), Esc closes, the header's button does the
// same as the keys. The size is remembered for this computer's player.
//
// Motion (docs/design.md §5): while dragging, the width follows the pointer directly; letting go,
// the layout takes its new width at once and the panel's edge travels there with transform only
// (the drawer curve, interruptible: a grab mid-way starts from where the edge is). Reduced motion
// takes the new size at once. Nothing runs while nobody drags (one window resize listener while
// open).
import { ArrowsInSimple, ArrowsOutSimple } from "@phosphor-icons/react";
import {
  cn,
  EntranceGate,
  GatedMotion,
  IconButton,
  keepOpenOnToast,
  type OutsideEvent,
  scrimMotion,
  sheetMotion,
  useInteractionMotion,
  useMotionPrefs,
} from "@pl/ui";
import { AnimatePresence } from "motion/react";
import { Dialog } from "radix-ui";
import {
  createContext,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

/** Today's width (the sheet's resting place before AFK-UX). */
export const NARROW_PX = 560;
const NARROW_CSS = `min(${NARROW_PX}px, 92vw)`;
/** From this panel width the panels lay their parts side by side. */
export const WIDE_MIN_PX = 960;
/** A flick (px per ms) decides by its direction alone. */
const FLICK = 0.45;
/** How long the edge takes to settle (the sheets' entrance is 360 ms; this is a shorter move). */
const SETTLE_MS = 280;
const DRAWER = "cubic-bezier(0.32, 0.72, 0, 1)";

interface PanelSize {
  /** The panel rests across the whole window. */
  full: boolean;
  setFull: (full: boolean) => void;
}

/** The player's size of the AFK panel, kept on this computer (`localStorage`). */
export const useAfkPanelSize = create<PanelSize>()(
  persist(
    (set) => ({
      full: false,
      setFull: (full) => set({ full }),
    }),
    { name: "pl.afkPanel", storage: createJSONStorage(() => localStorage) },
  ),
);

export type PanelPlace = "closed" | "narrow" | "full";

/**
 * Where a drag lets go: a flick by its direction (wider: the whole window; narrower: today's width
 * when it was wider, else closed), otherwise the nearest place by the edge's position, a little
 * ahead in the drag's direction.
 */
export function placeAfterDrag(
  width: number,
  velocity: number,
  narrow: number,
  full: number,
): PanelPlace {
  if (velocity > FLICK) return "full";
  if (velocity < -FLICK) return width > narrow + 24 ? "narrow" : "closed";
  const ahead = width + velocity * 120;
  if (ahead < narrow * 0.6) return "closed";
  return ahead - narrow < (full - narrow) / 2 ? "narrow" : "full";
}

interface PanelControls {
  full: boolean;
  /** The panel is wide enough to lay its parts side by side. */
  wide: boolean;
  /** Moves the panel to a size the way the keys do. */
  setSize: (full: boolean) => void;
  /** The panel element's id (the edge's `aria-controls`). */
  panelId: string;
}

const PanelContext = createContext<PanelControls>({
  full: false,
  wide: false,
  setSize: () => {},
  panelId: "",
});

/** The surrounding AFK panel's size, for its layouts and its header button. */
export function useAfkPanel(): PanelControls {
  return useContext(PanelContext);
}

/**
 * A press or focus outside closes the panel as Esc does, except on a toast and on the title bar:
 * the panel starts under it at every width, so the window still moves and its buttons work.
 */
export function keepOpenOnBar(event: OutsideEvent) {
  keepOpenOnToast(event);
  const target = event.target as Element | null;
  if (target?.closest?.("[data-titlebar]")) event.preventDefault();
}

/** Today's width in this window. */
function narrowWidth(): number {
  return Math.min(NARROW_PX, window.innerWidth * 0.92);
}

/** The panel's edge from the window's right side as it is drawn now (a settle included). */
function edgeOf(surface: HTMLElement | null): number {
  return surface ? window.innerWidth - surface.getBoundingClientRect().left : narrowWidth();
}

/** The window's width while `on`, from its resize events only. */
function useWindowWidth(on: boolean): number {
  const [width, setWidth] = useState(() =>
    typeof window === "undefined" ? 1280 : window.innerWidth,
  );
  useEffect(() => {
    if (!on) return;
    const read = () => setWidth(window.innerWidth);
    read();
    window.addEventListener("resize", read);
    return () => window.removeEventListener("resize", read);
  }, [on]);
  return width;
}

interface Drag {
  pointer: number;
  startX: number;
  startWidth: number;
  /** Live width (the panel's edge from the window's right side). */
  width: number;
  lastX: number;
  lastT: number;
  /** px per ms; positive: the edge moves left (wider). */
  velocity: number;
  frame: number;
}

/**
 * A right sheet of the AFK page that the player sizes (the desktop console, the 7/24 Bulut
 * account panel). `solid`: obsidian with the glass edge (the inventory's pixels stay crisp only
 * on a solid surface, docs/design.md §6), else the strong glass.
 */
export function AfkSidePanel({
  open,
  onClose,
  solid = false,
  onOpenAutoFocus,
  children,
}: {
  open: boolean;
  onClose: () => void;
  solid?: boolean;
  onOpenAutoFocus?: (event: Event) => void;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const animate = useInteractionMotion();
  const { reduce } = useMotionPrefs();
  const full = useAfkPanelSize((s) => s.full);
  const setFull = useAfkPanelSize((s) => s.setFull);
  const windowWidth = useWindowWidth(open);
  const panelId = useId();
  const asideRef = useRef<HTMLElement>(null);
  const surfaceRef = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const settling = useRef<Animation | null>(null);
  const [dragging, setDragging] = useState(false);
  const wide = full && windowWidth >= WIDE_MIN_PX;

  /** Rests the panel at a size, its edge travelling there from `from` with transform only. */
  const settle = useCallback(
    (toFull: boolean, from: number) => {
      const aside = asideRef.current;
      const surface = surfaceRef.current;
      setFull(toFull);
      if (!aside || !surface) return;
      settling.current?.cancel();
      settling.current = null;
      aside.style.width = toFull ? "100vw" : NARROW_CSS;
      surface.style.transform = "";
      const to = toFull ? window.innerWidth : narrowWidth();
      const shift = to - from;
      if (reduce || Math.abs(shift) < 1 || typeof surface.animate !== "function") return;
      // Growing, the panel starts pushed right (its far side off the window); shrinking, it
      // starts pulled left and its own surface fills the gap behind it (`data-settling`).
      surface.dataset.settling = shift < 0 ? "shrink" : "grow";
      const run = surface.animate(
        [{ transform: `translateX(${shift}px)` }, { transform: "translateX(0px)" }],
        { duration: SETTLE_MS, easing: DRAWER },
      );
      // A grab or a close cancels it, which rejects `finished`: nobody waits on it.
      run.finished.catch(() => undefined);
      const done = () => {
        if (settling.current === run) settling.current = null;
        delete surface.dataset.settling;
      };
      run.onfinish = done;
      run.oncancel = done;
      settling.current = run;
    },
    [reduce, setFull],
  );

  const setSize = useCallback(
    (toFull: boolean) => settle(toFull, edgeOf(surfaceRef.current)),
    [settle],
  );

  // Closed: nothing is left running.
  useEffect(() => {
    if (open) return;
    settling.current?.cancel();
    settling.current = null;
    drag.current = null;
    setDragging(false);
  }, [open]);

  // While open the page's shield (styles.css `#layer-shield`) leaves the title bar free too.
  useEffect(() => {
    if (!open) return;
    document.body.dataset.titlebarFree = "";
    return () => {
      delete document.body.dataset.titlebarFree;
    };
  }, [open]);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    // One pointer at a time: a second finger never moves the edge.
    if (event.button !== 0 || drag.current) return;
    const aside = asideRef.current;
    const surface = surfaceRef.current;
    if (!aside || !surface) return;
    event.preventDefault();
    // Grabbed mid-settle: the drag starts where the edge is drawn.
    const width = edgeOf(surface);
    settling.current?.cancel();
    settling.current = null;
    const narrow = narrowWidth();
    aside.style.width = `${Math.max(width, narrow)}px`;
    surface.style.transform = width < narrow ? `translateX(${narrow - width}px)` : "";
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      pointer: event.pointerId,
      startX: event.clientX,
      startWidth: width,
      width,
      lastX: event.clientX,
      lastT: event.timeStamp,
      velocity: 0,
      frame: 0,
    };
    setDragging(true);
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d || event.pointerId !== d.pointer) return;
    const width = Math.min(
      window.innerWidth,
      Math.max(0, d.startWidth + (d.startX - event.clientX)),
    );
    const dt = event.timeStamp - d.lastT;
    if (dt > 0) d.velocity = 0.7 * ((d.lastX - event.clientX) / dt) + 0.3 * d.velocity;
    d.lastX = event.clientX;
    d.lastT = event.timeStamp;
    d.width = width;
    if (d.frame) return;
    // One write per frame: wider than today's width the panel grows, narrower it slides away.
    d.frame = requestAnimationFrame(() => {
      d.frame = 0;
      const aside = asideRef.current;
      const surface = surfaceRef.current;
      if (!aside || !surface || drag.current !== d) return;
      const narrow = narrowWidth();
      aside.style.width = `${Math.max(d.width, narrow)}px`;
      surface.style.transform = d.width < narrow ? `translateX(${narrow - d.width}px)` : "";
    });
  };

  const letGo = (event: PointerEvent<HTMLDivElement>, canClose: boolean) => {
    const d = drag.current;
    if (!d || event.pointerId !== d.pointer) return;
    cancelAnimationFrame(d.frame);
    drag.current = null;
    setDragging(false);
    // A pause before letting go is no flick.
    const velocity = event.timeStamp - d.lastT > 80 ? 0 : d.velocity;
    const place = placeAfterDrag(d.width, velocity, narrowWidth(), window.innerWidth);
    if (place === "closed" && canClose) {
      onClose();
      return;
    }
    settle(place === "full", d.width);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const to =
      event.key === "ArrowLeft" || event.key === "End"
        ? true
        : event.key === "ArrowRight" || event.key === "Home"
          ? false
          : event.key === "Enter"
            ? !full
            : null;
    if (to === null) return;
    event.preventDefault();
    if (to !== full) setSize(to);
  };

  const narrowPercent = Math.round((Math.min(NARROW_PX, windowWidth * 0.92) / windowWidth) * 100);

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <AnimatePresence>
        {open ? (
          <EntranceGate key="sheet">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-x-0 top-titlebar bottom-0 z-40 bg-obsidian-950/50"
                  {...scrimMotion(!animate)}
                />
              </Dialog.Overlay>
              <Dialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                onPointerDownOutside={keepOpenOnBar}
                onFocusOutside={keepOpenOnBar}
                onOpenAutoFocus={onOpenAutoFocus}
              >
                <GatedMotion.aside
                  ref={asideRef}
                  id={panelId}
                  tabIndex={-1}
                  className="fixed top-titlebar right-0 bottom-0 z-50 flex outline-none"
                  style={{ width: full ? "100vw" : NARROW_CSS }}
                  {...sheetMotion(!animate)}
                >
                  <div
                    ref={surfaceRef}
                    className={cn(
                      "group/panel relative flex min-w-0 flex-1 flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)]",
                      solid
                        ? "border-l border-white/7 bg-obsidian-900 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),-30px_0_80px_rgb(34_6_6/0.5)]"
                        : "glass-strong",
                    )}
                  >
                    {/* Shrinking, the panel starts left of where it lands: its own surface fills
                        the gap behind it until it is there. */}
                    <div
                      aria-hidden
                      className={cn(
                        "pointer-events-none absolute inset-y-0 left-full hidden w-screen",
                        "group-data-[settling=shrink]/panel:block",
                        solid ? "bg-obsidian-900" : "glass-strong",
                      )}
                    />
                    {/* WAI-ARIA's window splitter: a focusable separator the arrows move (an <hr>
                        takes no focus and no children). */}
                    {/* biome-ignore lint/a11y/useSemanticElements: a focusable splitter, not a rule */}
                    <div
                      role="separator"
                      aria-orientation="vertical"
                      aria-controls={panelId}
                      aria-label={t("afkPanelSize.edge")}
                      aria-valuemin={narrowPercent}
                      aria-valuemax={100}
                      aria-valuenow={full ? 100 : narrowPercent}
                      aria-valuetext={full ? t("afkPanelSize.full") : t("afkPanelSize.narrow")}
                      tabIndex={0}
                      onPointerDown={onPointerDown}
                      onPointerMove={onPointerMove}
                      onPointerUp={(event) => letGo(event, true)}
                      onPointerCancel={(event) => letGo(event, false)}
                      onDoubleClick={() => setSize(!full)}
                      onKeyDown={onKeyDown}
                      className={cn(
                        "group/edge absolute inset-y-0 z-10 flex w-4 cursor-ew-resize touch-none items-center justify-center outline-none",
                        // Across the whole window the edge is the window's: the grip stays inside.
                        full ? "left-0" : "-left-2",
                      )}
                    >
                      {/* The grip: quiet at rest, brighter under the pointer, while dragging and
                          on the keyboard's focus (with the ring). */}
                      <span
                        aria-hidden
                        className={cn(
                          "h-11 w-1 rounded-full bg-white/16 transition-[background-color,box-shadow] duration-150",
                          "group-hover/edge:bg-white/40 group-focus-visible/edge:bg-white/45 group-focus-visible/edge:ring-2 group-focus-visible/edge:ring-gold-300 group-focus-visible/edge:ring-offset-2 group-focus-visible/edge:ring-offset-obsidian-900",
                          dragging && "bg-white/55",
                        )}
                      />
                    </div>
                    <PanelContext.Provider value={{ full, wide, setSize, panelId }}>
                      {children}
                    </PanelContext.Provider>
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

/** The header's button for the panel's size: the whole window, or back to today's width. */
export function PanelSizeButton() {
  const { t } = useTranslation();
  const { full, setSize } = useAfkPanel();
  return (
    <IconButton
      label={full ? t("afkPanelSize.toNarrow") : t("afkPanelSize.toFull")}
      icon={full ? <ArrowsInSimple size={18} /> : <ArrowsOutSimple size={18} />}
      size="sm"
      onClick={() => setSize(!full)}
    />
  );
}
