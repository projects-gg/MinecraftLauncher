// A tile on the stage (SQOL-U1; sqol.md §1.1): one watched share, the player's own share's preview,
// or a share that just ended ("Yayın bitti.", two seconds).
//
// A watched share's tile: its picture (zoomable: Ctrl + the wheel under the pointer, drag or
// Shift + the arrows to look around, the "%250" chip resets), its name always, and a bar on a dark
// scrim: what is shared, how many watch, "Düşük Kalite" on a weak connection; "Yayın Sesi", the
// window, full screen, "⋯" and "İzlemeyi Bırak". The bar shows under the pointer or the focus; in
// full screen it fades after three quiet seconds (never while a menu is open, the focus is in it or
// the pointer is on it). A right click, Shift + F10, the menu key and "⋯" open one menu
// (stream-menu.tsx). Keys while the tile has the focus: F, M, ↑ ↓, Ctrl + = - 0, Shift + arrows.
// States: "Bağlanıyor…", "Yayın duraklatıldı." (no last frame: the sharer's face, blurred),
// "Paylaşılan pencere küçültüldü." (the last frame, blurred). Nothing moves on its own.
import {
  ArrowSquareIn,
  ArrowSquareOut,
  CornersIn,
  CornersOut,
  DotsThree,
  Eye,
  SpeakerHigh,
  SpeakerSimpleSlash,
  X,
} from "@phosphor-icons/react";
import { Chip, ContextMenu, cn, lastInputWasPointer, Menu, Tooltip } from "@pl/ui";
import {
  type ComponentPropsWithoutRef,
  forwardRef,
  type ReactElement,
  type FocusEvent as ReactFocusEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { SocialFace } from "@/features/social/social-face";
import { useSocialUi } from "@/features/social/store";
import { useFace } from "@/features/voice/store";
import { formatPercent } from "@/lib/format";
import { shareApi } from "@/share/api";
import { mockZoom } from "@/share/mock-view";
import { param } from "@/share/param";
import { shareKey, type VoiceLiveShare, type VoiceWatch } from "@/share/types";
import { unwatchShare } from "./actions";
import { isPopout, useShareInfo, useShareName, useShareQuality, useShareUi } from "./store";
import { popOut, toggleFullscreen } from "./stream-actions";
import { moveStreamVolume, streamSoundOf, toggleStreamMute, useStreamEnv } from "./stream-env";
import { useSoundItems, useStreamItems, useStreamSound } from "./stream-menu";
import { sayStream } from "./stream-say";
import { useOwnPreviewFrame, useOwnShareInfo, useOwnShareItems } from "./stream-seams";
import {
  clampZoom,
  layerFor,
  NO_WEAK_HOLD,
  NO_ZOOM,
  panBy,
  tileKey,
  tileShortcuts,
  weakBackAt,
  weakStep,
  wheelFactor,
  ZOOM_STEP,
  type Zoom,
  zoomAt,
  zoomPercent,
} from "./stream-view";
import { ShareVideo } from "./video";

/** Layer changes wait for a resize to settle. */
const LAYER_SETTLE_MS = 400;
/** A press that moved this far was a drag, not half of a double click. */
const DRAG_PX = 4;
/** A key's volume change is said once the presses settle. */
const SAY_VOLUME_MS = 700;

// --- small parts --------------------------------------------------------------------------------

/**
 * A control over the picture: an icon on a near-solid dark plate (no blur: a blur over a playing
 * video is drawn again every frame), its words in the tooltip and its accessible name.
 */
export const StageButton = forwardRef<
  HTMLButtonElement,
  ComponentPropsWithoutRef<"button"> & { label: string; pressed?: boolean; off?: boolean }
>(function StageButton({ label, pressed, off, className, children, ...props }, ref) {
  return (
    <Tooltip content={label}>
      <button
        ref={ref}
        type="button"
        aria-label={label}
        aria-pressed={pressed}
        className={cn(
          "flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-[8px] outline-none",
          "bg-obsidian-950/78 ring-1 ring-white/10",
          "transition-[background-color,color,transform,scale] duration-150 ease-out hover:bg-obsidian-800/95 active:scale-[0.94]",
          "focus-visible:ring-2 focus-visible:ring-gold-300 data-[state=open]:bg-obsidian-800/95",
          off ? "text-flare-300" : "text-bone-100",
          pressed && "bg-white/16 hover:bg-white/20",
          className,
        )}
        {...props}
      >
        {children}
      </button>
    </Tooltip>
  );
});

interface BackdropFace {
  id: string;
  name: string;
  avatar?: string;
  avatarUrl?: string;
}

/** A face with a picture of its own (a preset, a skin's head, a photo); else its letter. */
function hasPicture(face: BackdropFace): boolean {
  return (
    Boolean(face.avatarUrl) ||
    (face.avatar !== undefined && face.avatar !== "" && face.avatar !== "discord")
  );
}

/**
 * Behind a tile without its picture: the sharer's face, large and blurred, veiled (SHARE-LIVE's
 * "blurlu" placeholder); a letter face stays a plain plate (it blurs into a glow).
 */
export function StreamBackdrop({ face }: { face: BackdropFace }) {
  return (
    <div aria-hidden className="absolute inset-0 overflow-hidden bg-obsidian-950">
      {hasPicture(face) ? (
        <span className="absolute top-1/2 left-1/2 flex -translate-x-1/2 -translate-y-1/2 scale-[4] blur-[14px]">
          <SocialFace source={face} size={296} />
        </span>
      ) : null}
      <span className="absolute inset-0 bg-obsidian-950/72" />
    </div>
  );
}

/** One quiet line in the middle of a tile (a state). */
function TileNote({ children }: { children: ReactNode }) {
  return (
    <p className="pointer-events-none absolute inset-0 flex items-center justify-center px-6 text-center text-[13.5px] leading-[1.4] font-medium text-bone-100">
      {children}
    </p>
  );
}

/** The name, always there (a small label while the bar rests). */
function NamePlate({ name }: { name: string }) {
  return (
    <span className="flex min-w-0 items-center rounded-[8px] bg-obsidian-950/78 px-2.5 py-1.5 ring-1 ring-white/10">
      <span className="truncate text-[13px] leading-[1.3] font-semibold text-bone-50">{name}</span>
    </span>
  );
}

/**
 * Opens a tile's menu from its "⋯": the very menu a right click opens (one menu, the same rows),
 * at the button.
 */
function openTileMenu(button: HTMLElement) {
  const box = button.getBoundingClientRect();
  button.dispatchEvent(
    new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
      button: 2,
      clientX: box.right,
      clientY: box.top - 6,
    }),
  );
}

function MoreButton({ open, label }: { open: boolean; label: string }) {
  return (
    <StageButton
      label={label}
      aria-haspopup="menu"
      aria-expanded={open}
      data-state={open ? "open" : "closed"}
      onClick={(event) => openTileMenu(event.currentTarget)}
    >
      <DotsThree size={18} weight="bold" />
    </StageButton>
  );
}

/** "Yayın Sesi": its slider and "Yayını Sessize Al" (the bar's and the corner's). */
export function SoundButton({
  person,
  onOpenChange,
  side = "top",
}: {
  person: string;
  onOpenChange?: (open: boolean) => void;
  side?: "top" | "bottom";
}) {
  const { t } = useTranslation();
  const items = useSoundItems(person);
  const { muted } = useStreamSound(person);
  return (
    <Menu
      items={items}
      align="end"
      side={side}
      onOpenChange={onOpenChange}
      label={t("shareView.menu.volume")}
      trigger={
        <StageButton
          label={muted ? t("shareView.bar.soundOff") : t("shareView.menu.volume")}
          off={muted}
        >
          {muted ? (
            <SpeakerSimpleSlash size={16} weight="bold" />
          ) : (
            <SpeakerHigh size={16} weight="bold" />
          )}
        </StageButton>
      }
    />
  );
}

/** The stream menu around a tile (a right click, Shift + F10, the menu key, its "⋯"). */
export function StreamMenu({
  person,
  n,
  title,
  onOpenChange,
  children,
}: {
  person: string;
  n: number;
  title: string;
  onOpenChange: (open: boolean) => void;
  children: ReactElement;
}) {
  const items = useStreamItems(person, n);
  return (
    <ContextMenu items={items} title={title} onOpenChange={onOpenChange}>
      {children}
    </ContextMenu>
  );
}

/** The preview shows the bars without the pointer (`chrome=1`). */
const CHROME_PREVIEW = param("chrome") === "1";

/**
 * The menus a part of the stage has open now, told to the stage too (full screen's controls never
 * rest meanwhile); a part that goes with a menu open gives its count back.
 */
export function useMenuCount(): { menus: number; onMenu: (open: boolean) => void } {
  const menuOpen = useShareUi((s) => s.menuOpen);
  const held = useRef(0);
  const [menus, setMenus] = useState(0);
  const onMenu = useCallback(
    (open: boolean) => {
      held.current = Math.max(0, held.current + (open ? 1 : -1));
      setMenus(held.current);
      menuOpen(open);
    },
    [menuOpen],
  );
  useEffect(
    () => () => {
      for (; held.current > 0; held.current--) menuOpen(false);
    },
    [menuOpen],
  );
  return { menus, onMenu };
}

/** Whether a tile's controls show, and what keeps them (sqol.md §1.1; WCAG 1.4.13). */
function useChrome(full: boolean, idle: boolean) {
  const stageMenus = useShareUi((s) => s.menus > 0);
  const [hover, setHover] = useState(false);
  const [focusIn, setFocusIn] = useState(false);
  const [barFocus, setBarFocus] = useState(false);
  const [overBar, setOverBar] = useState(false);
  const { menus, onMenu } = useMenuCount();
  const [moreOpen, setMoreOpen] = useState(false);
  const onTileMenu = useCallback(
    (open: boolean) => {
      setMoreOpen(open);
      onMenu(open);
    },
    [onMenu],
  );
  const shown =
    CHROME_PREVIEW ||
    menus > 0 ||
    (full ? !idle || barFocus || overBar || stageMenus : hover || focusIn);
  // The focus keeps the bar only when the keys moved it there (a clicked button keeps the focus,
  // and the bar would stay up after the pointer left).
  const comes = (set: (on: boolean) => void) => () => set(!lastInputWasPointer());
  const leaves = (set: (on: boolean) => void) => (event: ReactFocusEvent<HTMLElement>) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) set(false);
  };
  return {
    shown,
    /** The sound menu and other menus of the tile. */
    onMenu,
    /** The tile's own menu (its "⋯" says it is open). */
    onTileMenu,
    moreOpen,
    tile: {
      onPointerEnter: () => setHover(true),
      onPointerLeave: () => setHover(false),
      onFocus: comes(setFocusIn),
      onBlur: leaves(setFocusIn),
    },
    bar: {
      onPointerEnter: () => setOverBar(true),
      onPointerLeave: () => setOverBar(false),
      onFocus: comes(setBarFocus),
      onBlur: leaves(setBarFocus),
    },
  };
}

/** The fading part of a bar. */
const fade = (shown: boolean) =>
  cn(
    "transition-opacity duration-200 ease-out",
    shown ? "opacity-100" : "pointer-events-none opacity-0",
  );

/** The scrim under a bar (WCAG 1.4.11: controls over a moving picture). */
function Scrim({ shown }: { shown: boolean }) {
  return (
    <span
      aria-hidden
      className={cn(
        "pointer-events-none absolute inset-x-0 bottom-0 h-28 bg-linear-to-t from-obsidian-950/88 via-obsidian-950/45 to-transparent",
        fade(shown),
      )}
    />
  );
}

/** The tile's frame: a rounded panel the keyboard can reach. */
const tileClass =
  "group/tile @container/tile relative min-h-0 min-w-0 overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950 ring-1 ring-white/8 outline-none focus-visible:ring-2 focus-visible:ring-gold-300";

// --- a watched share --------------------------------------------------------------------------------

export function StreamTile({
  watch,
  live,
  full,
  idle,
  two,
  first,
  className,
}: {
  watch: VoiceWatch;
  live: VoiceLiveShare | undefined;
  full: boolean;
  /** Full screen and no input for a while: the bar rests. */
  idle: boolean;
  /** Two shares on the stage: a tile's "Tam Ekran" brings its own forward. */
  two: boolean;
  /** The stage's first tile (the preview's `zoom=` lands there). */
  first: boolean;
  className?: string;
}) {
  const { t } = useTranslation();
  const { person, n } = watch;
  const key = shareKey(person, n);
  const name = useShareName(person);
  const face = useFace(person);
  const quality = useShareQuality(person, n);
  const viewQuality = useStreamEnv((s) => s.viewQuality);
  const infoOn = useShareUi((s) => s.info);
  const info = useShareInfo(person, n, infoOn, watch.layer);
  const chrome = useChrome(full, idle);
  const popout = isPopout();

  const box = useRef<HTMLElement>(null);
  const picture = useRef<HTMLDivElement>(null);
  const size = useRef({ w: 0, h: 0 });
  const zoom = useRef<Zoom>(NO_ZOOM);
  const [percent, setPercent] = useState(100);
  const [width, setWidth] = useState<number | null>(null);
  const firstZoom = useRef(first ? mockZoom() : null);

  /** Zoom and pan follow the input at once (no React render on a drag; the chip's figure only). */
  const applyZoom = useCallback((next: Zoom) => {
    zoom.current = next;
    const element = picture.current;
    if (element) {
      element.style.transform =
        next.s === 1 ? "" : `translate3d(${next.x}px, ${next.y}px, 0) scale(${next.s})`;
    }
    const shown = zoomPercent(next);
    setPercent((before) => (before === shown ? before : shown));
  }, []);

  // Its size: the zoom stays inside it, and the layer follows it once a resize settles.
  useEffect(() => {
    const element = box.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width: w, height: h } = entry.contentRect;
      size.current = { w, h };
      const start = firstZoom.current;
      if (start && w > 0) {
        firstZoom.current = null;
        applyZoom(zoomAt(NO_ZOOM, start, w / 2, h / 2, w, h));
      } else {
        applyZoom(clampZoom(zoom.current, w, h));
      }
      clearTimeout(timer);
      timer = setTimeout(() => setWidth(w), LAYER_SETTLE_MS);
    });
    observer.observe(element);
    return () => {
      clearTimeout(timer);
      observer.disconnect();
    };
  }, [applyZoom]);

  // Ctrl + the wheel zooms under the pointer (the page itself never zooms).
  useEffect(() => {
    const element = box.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      const rect = element.getBoundingClientRect();
      applyZoom(
        zoomAt(
          zoom.current,
          wheelFactor(event.deltaY, event.deltaMode),
          event.clientX - rect.left,
          event.clientY - rect.top,
          rect.width,
          rect.height,
        ),
      );
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [applyZoom]);

  // The layer ("Yayın Kalitesi"): a weak connection holds the small one a while under "Otomatik".
  const [hold, setHold] = useState(NO_WEAK_HOLD);
  const weak = quality === "weak";
  useEffect(() => {
    setHold((before) => weakStep(before, weak, Date.now()));
  }, [weak]);
  useEffect(() => {
    const at = weakBackAt(hold);
    if (at === null) return;
    const timer = setTimeout(
      () => setHold((before) => weakStep(before, false, Date.now())),
      Math.max(0, at - Date.now()),
    );
    return () => clearTimeout(timer);
  }, [hold]);
  const layer = layerFor({
    quality: viewQuality,
    width,
    weakHeld: viewQuality === "auto" && hold.held,
    zoomed: percent > 100,
    hasSmall: true,
  });
  const asked = useRef(watch.layer);
  useEffect(() => {
    if (watch.layer === layer) {
      asked.current = layer;
      return;
    }
    if (watch.phase === "held" || asked.current === layer || width === null) return;
    asked.current = layer;
    void shareApi.layer(person, n, layer).catch(() => {});
  }, [layer, watch.layer, watch.phase, width, person, n]);

  // A drag looks around a zoomed picture; a double click is full screen.
  const drag = useRef<{ id: number; x: number; y: number; moved: number } | null>(null);
  const dragged = useRef(0);

  // Keys while the tile has the focus (never a page shortcut: WCAG 2.1.4).
  const sayTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(sayTimer.current), []);
  const onKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    const root = event.currentTarget;
    // Keys in a menu opened from the tile pass through React's tree, not the page's.
    if (event.defaultPrevented || !root.contains(event.target as Node)) return;
    const action = tileKey(event, {
      onTile: event.target === root,
      zoomed: zoom.current.s > 1,
      sound: true,
    });
    if (!action) return;
    event.preventDefault();
    const { w, h } = size.current;
    switch (action.kind) {
      case "fullscreen":
        toggleFullscreen(two ? key : undefined);
        return;
      case "mute": {
        const muted = streamSoundOf(person).muted;
        toggleStreamMute(person);
        sayStream(muted ? t("shareView.say.soundOn") : t("shareView.say.soundOff"));
        return;
      }
      case "volume": {
        const max = useStreamEnv.getState().volumeMax;
        const next = Math.min(max, Math.max(0, streamSoundOf(person).volume + action.by));
        moveStreamVolume(person, next, false);
        clearTimeout(sayTimer.current);
        sayTimer.current = setTimeout(
          () => sayStream(t("shareView.say.volume", { value: formatPercent(next) })),
          SAY_VOLUME_MS,
        );
        return;
      }
      case "zoom":
        applyZoom(
          action.by === "reset"
            ? NO_ZOOM
            : zoomAt(
                zoom.current,
                action.by === "in" ? ZOOM_STEP : 1 / ZOOM_STEP,
                w / 2,
                h / 2,
                w,
                h,
              ),
        );
        return;
      case "pan":
        applyZoom(panBy(zoom.current, action.dx * w * 0.1, action.dy * h * 0.1, w, h));
        return;
    }
  };

  const st = live?.st ?? "live";
  const phase = watch.phase;
  const paused = phase === "live" && st === "hold";
  const minimized = phase === "live" && st === "min";
  const note =
    phase === "connecting"
      ? t("share.stage.connecting")
      : phase === "held"
        ? t("share.stage.held")
        : paused
          ? t("shareView.state.hold")
          : minimized
            ? t("shareView.state.min")
            : null;
  const zoomed = percent > 100;

  const tile = (
    <section
      ref={box}
      // biome-ignore lint/a11y/noNoninteractiveTabindex: the tile takes its keys (F, M, the arrows, zoom) while it has the focus
      tabIndex={0}
      aria-label={t("share.stage.label", { name })}
      aria-keyshortcuts={tileShortcuts(true)}
      onKeyDown={onKeyDown}
      {...chrome.tile}
      className={cn(tileClass, className)}
    >
      {phase !== "live" || paused ? <StreamBackdrop face={face} /> : null}
      {/* The picture (kept while paused: it comes back without a new connection). */}
      {phase === "held" ? null : (
        // biome-ignore lint/a11y/noStaticElementInteractions: a drag looks around a zoomed picture (Shift + the arrows do the same), a double click is full screen (F too)
        <div
          className={cn(
            "absolute inset-0 overflow-hidden",
            zoomed && "cursor-grab active:cursor-grabbing",
            (paused || phase === "connecting") && "opacity-0",
          )}
          onPointerDown={(event) => {
            if (event.button !== 0 || zoom.current.s === 1) return;
            drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY, moved: 0 };
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            const at = drag.current;
            if (!at || at.id !== event.pointerId) return;
            const dx = event.clientX - at.x;
            const dy = event.clientY - at.y;
            at.x = event.clientX;
            at.y = event.clientY;
            at.moved += Math.abs(dx) + Math.abs(dy);
            applyZoom(panBy(zoom.current, dx, dy, size.current.w, size.current.h));
          }}
          onPointerUp={() => {
            dragged.current = drag.current?.moved ?? 0;
            drag.current = null;
          }}
          onPointerCancel={() => {
            drag.current = null;
          }}
          onDoubleClick={() => {
            if (dragged.current > DRAG_PX) {
              dragged.current = 0;
              return;
            }
            toggleFullscreen(two ? key : undefined);
          }}
        >
          <div
            ref={picture}
            className={cn(
              "absolute inset-0 origin-top-left",
              minimized && "blur-[16px] brightness-75",
            )}
          >
            <ShareVideo person={person} n={n} fit="contain" />
          </div>
        </div>
      )}
      {/* A minimized window's last frame may be bright: a veil keeps its line readable. */}
      {minimized ? <span aria-hidden className="absolute inset-0 bg-obsidian-950/50" /> : null}
      {note ? <TileNote>{note}</TileNote> : null}

      {/* Top left: "Yayın Bilgileri" and the zoom's chip (a press gives the whole picture back). */}
      {info || zoomed ? (
        <span className="absolute top-2 left-2 flex items-center gap-1.5">
          {info ? (
            <span className="rounded-[7px] bg-obsidian-950/78 px-2 py-1 text-[12px] leading-[1.3] font-medium text-bone-100 tabular-nums ring-1 ring-white/10 @max-[22rem]/tile:hidden">
              {t("shareView.info.line", {
                size: `${info.w}×${info.h}`,
                fps: info.fps,
                layer: t(`shareView.info.${watch.layer}`),
              })}
            </span>
          ) : null}
          {zoomed ? (
            <Tooltip content={t("shareView.zoom.reset")}>
              <button
                type="button"
                aria-label={t("shareView.zoom.label", { value: formatPercent(percent) })}
                onClick={() => applyZoom(NO_ZOOM)}
                className="flex h-7 cursor-pointer items-center rounded-[7px] bg-obsidian-950/78 px-2 text-[12.5px] font-semibold text-bone-50 tabular-nums ring-1 ring-white/10 outline-none transition-[background-color,transform,scale] duration-150 hover:bg-obsidian-800/95 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.96]"
              >
                {formatPercent(percent)}
              </button>
            </Tooltip>
          ) : null}
        </span>
      ) : null}

      <Scrim shown={chrome.shown} />
      <div
        className="absolute inset-x-2 bottom-2 flex items-end justify-between gap-2"
        {...chrome.bar}
      >
        <span className="flex min-w-0 items-center gap-1.5">
          <NamePlate name={name} />
          <span
            className={cn(
              "flex min-w-0 items-center gap-2 pl-0.5 text-[12px] leading-[1.3] text-ash-200 @max-[24rem]/tile:hidden",
              fade(chrome.shown),
            )}
          >
            {live ? <span className="shrink-0">{t(`share.kind.${live.kind}`)}</span> : null}
            {live ? (
              <span className="flex shrink-0 items-center gap-1 tabular-nums">
                <Eye size={13} weight="bold" aria-hidden />
                <span aria-hidden>{live.watchers}</span>
                <span className="sr-only">
                  {t("share.live.watchers", { count: live.watchers })}
                </span>
              </span>
            ) : null}
          </span>
          {weak ? (
            <Tooltip content={t("shareView.bar.weakHint")}>
              <Chip
                tone="neutral"
                tabIndex={0}
                className={cn(
                  "outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
                  fade(chrome.shown),
                )}
              >
                {t("shareView.bar.weak")}
              </Chip>
            </Tooltip>
          ) : null}
        </span>
        <span className={cn("flex shrink-0 gap-1.5", fade(chrome.shown))}>
          <SoundButton person={person} onOpenChange={chrome.onMenu} />
          <span className="flex gap-1.5 @max-[22rem]/tile:hidden">
            {popout ? (
              <StageButton label={t("shareView.menu.popin")} onClick={() => popOut(false)}>
                <ArrowSquareIn size={16} weight="bold" />
              </StageButton>
            ) : (
              <StageButton label={t("shareView.menu.popout")} onClick={() => popOut(true)}>
                <ArrowSquareOut size={16} weight="bold" />
              </StageButton>
            )}
            <StageButton
              label={full ? t("shareView.menu.exitFullscreen") : t("shareView.menu.fullscreen")}
              onClick={() => toggleFullscreen(two ? key : undefined)}
            >
              {full ? (
                <CornersIn size={16} weight="bold" />
              ) : (
                <CornersOut size={16} weight="bold" />
              )}
            </StageButton>
          </span>
          <MoreButton open={chrome.moreOpen} label={t("shareView.bar.more")} />
          <StageButton label={t("share.stage.stop")} onClick={() => void unwatchShare(person, n)}>
            <X size={16} weight="bold" />
          </StageButton>
        </span>
      </div>
    </section>
  );

  return (
    <StreamMenu person={person} n={n} title={name} onOpenChange={chrome.onTileMenu}>
      {tile}
    </StreamMenu>
  );
}

// --- the player's own share -------------------------------------------------------------------------

/**
 * The player's own share as the stage shows it when its card's preview is pressed (SQOL-U2 asks):
 * the launcher's preview frames (no sound, no watching) and its own menu ("Ekranı Değiştir…", …).
 */
export function OwnTile({
  n,
  full,
  idle,
  onClose,
  className,
}: {
  n: number;
  full: boolean;
  idle: boolean;
  onClose: () => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me);
  const name = me?.name ?? t("voice.room.you");
  const frame = useOwnPreviewFrame(n);
  const items = useOwnShareItems(n);
  const infoOn = useShareUi((s) => s.info);
  const sends = useOwnShareInfo(n);
  const chrome = useChrome(full, idle);
  const face: BackdropFace = {
    id: me?.id ?? "",
    name,
    avatar: me?.avatar,
    avatarUrl: me?.avatarUrl,
  };
  const tile = (
    <section
      // biome-ignore lint/a11y/noNoninteractiveTabindex: the tile takes its key (F) while it has the focus
      tabIndex={0}
      aria-label={t("shareView.own.label")}
      aria-keyshortcuts="F Escape Shift+F10"
      onKeyDown={(event) => {
        const root = event.currentTarget;
        if (event.defaultPrevented || !root.contains(event.target as Node)) return;
        const action = tileKey(event, {
          onTile: event.target === root,
          zoomed: false,
          sound: false,
        });
        if (action?.kind !== "fullscreen") return;
        event.preventDefault();
        toggleFullscreen();
      }}
      {...chrome.tile}
      className={cn(tileClass, className)}
    >
      {frame ? (
        <img
          src={frame}
          alt=""
          draggable={false}
          className="absolute inset-0 size-full object-contain"
        />
      ) : (
        <StreamBackdrop face={face} />
      )}
      {/* "Yayın Bilgileri": what this computer really sends. */}
      {infoOn && sends ? (
        <span className="absolute top-2 left-2 rounded-[7px] bg-obsidian-950/78 px-2 py-1 text-[12px] leading-[1.3] font-medium text-bone-100 tabular-nums ring-1 ring-white/10">
          {sends}
        </span>
      ) : null}
      <Scrim shown={chrome.shown} />
      <div
        className="absolute inset-x-2 bottom-2 flex items-end justify-between gap-2"
        {...chrome.bar}
      >
        <span className="flex min-w-0 items-center gap-1.5">
          <NamePlate name={name} />
          <span
            className={cn(
              "shrink-0 pl-0.5 text-[12px] leading-[1.3] text-ash-200",
              fade(chrome.shown),
            )}
          >
            {t("shareView.own.preview")}
          </span>
        </span>
        <span className={cn("flex shrink-0 gap-1.5", fade(chrome.shown))}>
          <StageButton
            label={full ? t("shareView.menu.exitFullscreen") : t("shareView.menu.fullscreen")}
            onClick={() => toggleFullscreen()}
          >
            {full ? <CornersIn size={16} weight="bold" /> : <CornersOut size={16} weight="bold" />}
          </StageButton>
          {items.length ? (
            <MoreButton open={chrome.moreOpen} label={t("shareView.bar.more")} />
          ) : null}
          <StageButton label={t("shareView.own.close")} onClick={onClose}>
            <X size={16} weight="bold" />
          </StageButton>
        </span>
      </div>
    </section>
  );
  return (
    <ContextMenu
      items={items}
      title={t("shareView.own.label")}
      disabled={!items.length}
      onOpenChange={chrome.onTileMenu}
    >
      {tile}
    </ContextMenu>
  );
}

// --- a share that ended ------------------------------------------------------------------------------

/** "Yayın bitti." for two seconds where the share was. */
export function EndedTile({ person, className }: { person: string; className?: string }) {
  const { t } = useTranslation();
  const name = useShareName(person);
  const face = useFace(person);
  return (
    <section aria-label={t("share.stage.label", { name })} className={cn(tileClass, className)}>
      <StreamBackdrop face={face} />
      <TileNote>{t("shareView.state.ended")}</TileNote>
      <span className="absolute bottom-2 left-2 flex max-w-[calc(100%-1rem)]">
        <NamePlate name={name} />
      </span>
    </section>
  );
}
