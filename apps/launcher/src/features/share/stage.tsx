// The stage (SHARE, SQOL-U1; sqol.md §1.1): what the player watches. In the launcher's window it
// covers the page under the title bar (one share big, two side by side, or one forward with the
// other beside it), sits small in the corner ("Küçült") while the player uses the launcher, or
// fills the screen; in the "Yayın" window it fills the window. Its tiles are stream-tile.tsx; here
// are the layout, the stage's own controls ("Sohbet", "Küçült", "Her Zaman Üstte"), the corner, "Hâlâ
// izliyor musun?", the "Yayın bitti." cards, and full screen's resting controls (they fade after
// three quiet seconds; the pointer, a key or a menu brings them back; Esc leaves full screen).
//
// The room panel docks beside the stage on the page (SQOL-U3): the stage's tiles keep clear of it
// (`useStageDock`) and its chat peek shows top right while the panel is closed or in full screen.
// Motion: the stage fades and rises a little in, tiles fade in and out, the corner and the full
// stage cross-fade; reduced motion keeps the fades. Layout changes snap (a moving video would warp).
import {
  ArrowsInSimple,
  ArrowsOut,
  ChatCircleText,
  CornersIn,
  PushPin,
  PushPinSlash,
  Screencast,
  X,
} from "@phosphor-icons/react";
import { Button, cn, toast, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  type KeyboardEvent as ReactKeyboardEvent,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useFace, useVoiceUi } from "@/features/voice/store";
import { isTauri } from "@/lib/platform";
import { shareApi } from "@/share/api";
import { mockEnded } from "@/share/mock-view";
import { param } from "@/share/param";
import {
  type ShareViewQuality,
  shareKey,
  type VoiceLiveShare,
  type VoiceShareView,
  type VoiceWatch,
} from "@/share/types";
import { unwatchShare } from "./actions";
import { type StageMode, useShare, useShareName, useShareStage, useShareUi } from "./store";
import { keepOnTop, leaveFullscreen } from "./stream-actions";
import { useStreamEnv } from "./stream-env";
import {
  hideOwnTile,
  openRoomChat,
  RoomChatPeek,
  useOwnTile,
  useRoomChatOffered,
  useRoomChatUnread,
  useStageDock,
} from "./stream-seams";
import {
  EndedTile,
  OwnTile,
  SoundButton,
  StageButton,
  StreamBackdrop,
  StreamMenu,
  StreamTile,
  useMenuCount,
} from "./stream-tile";
import { layerFor } from "./stream-view";
import { ShareVideo } from "./video";

/** Full screen's controls rest after this long without the pointer, a key or a press. */
export const IDLE_MS = 3000;
/** "Yayın bitti." stays this long where the share was. */
export const ENDED_MS = 2000;
/** The corner picture's width (CSS pixels). */
const CORNER_PX = 320;

/** The shares the stage shows: the ones the player watches, never their own. */
export function stageWatches(share: VoiceShareView, me: string | null): VoiceWatch[] {
  return share.watching.filter((watch) => watch.person !== me);
}

/** A share that ended while watched: its card for a moment. */
export interface EndedShare {
  key: string;
  person: string;
  n: number;
  at: number;
}

/**
 * Watched shares that ended (their sharer stopped; not the player's "İzlemeyi Bırak", nor watching
 * moving to the other window: the share is still live then), each kept two seconds for its "Yayın
 * bitti." card. Leaving the room shows none (`on` false: no sharing at all).
 */
export function useEndedShares(
  watches: VoiceWatch[],
  live: VoiceLiveShare[],
  on: boolean,
): EndedShare[] {
  const [ended, setEnded] = useState<EndedShare[]>(() => {
    const seeded = mockEnded();
    return seeded
      ? [{ ...seeded, key: shareKey(seeded.person, seeded.n), at: Number.POSITIVE_INFINITY }]
      : [];
  });
  const watchKeys = watches.map((w) => shareKey(w.person, w.n)).join(" ");
  const liveKeys = live.map((l) => shareKey(l.person, l.n)).join(" ");
  const before = useRef<string[]>(watchKeys ? watchKeys.split(" ") : []);
  useEffect(() => {
    const now = watchKeys ? watchKeys.split(" ") : [];
    if (!on) {
      before.current = [];
      setEnded((list) => (list.length ? [] : list));
      return;
    }
    const still = new Set(liveKeys ? liveKeys.split(" ") : []);
    const gone = before.current.filter((key) => !now.includes(key) && !still.has(key));
    before.current = now;
    setEnded((list) => {
      const kept = list.filter((e) => !now.includes(e.key));
      if (!gone.length) return kept.length === list.length ? list : kept;
      const at = Date.now();
      return [
        ...kept.filter((e) => !gone.includes(e.key)),
        ...gone.map((key) => {
          const [person = "", n = "1"] = key.split(":");
          return { key, person, n: Number(n), at };
        }),
      ];
    });
  }, [watchKeys, liveKeys, on]);
  useEffect(() => {
    const next = Math.min(...ended.map((e) => e.at + ENDED_MS));
    if (!Number.isFinite(next)) return;
    const timer = setTimeout(
      () => setEnded((list) => list.filter((e) => e.at + ENDED_MS > Date.now())),
      Math.max(0, next - Date.now()),
    );
    return () => clearTimeout(timer);
  }, [ended]);
  return ended;
}

/** Full screen's resting controls: `idle` after three quiet seconds; `wake` on any input. */
function useIdle(active: boolean) {
  const [idle, setIdle] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const menus = useShareUi((s) => s.menus);
  const wake = useCallback(() => {
    setIdle(false);
    clearTimeout(timer.current);
    if (active) timer.current = setTimeout(() => setIdle(true), IDLE_MS);
  }, [active]);
  // A menu that closes counts as input (its controls do not vanish under the closing menu).
  // biome-ignore lint/correctness/useExhaustiveDependencies: the menus' count says when to wake
  useEffect(() => {
    wake();
    return () => clearTimeout(timer.current);
  }, [wake, menus]);
  return { idle: active && idle && menus === 0, wake };
}

/** The stage's own controls (top right). */
function StageControls({ popout, full, idle }: { popout: boolean; full: boolean; idle: boolean }) {
  const { t } = useTranslation();
  const setSmall = useShareUi((s) => s.setSmall);
  const chat = useRoomChatOffered();
  const unread = useRoomChatUnread();
  // The launcher's word (the "Yayın" window has no settings of its own to read).
  const onTop = useShare().onTop;
  const [pinned, setPinned] = useState(onTop);
  useEffect(() => setPinned(onTop), [onTop]);
  const [focusIn, setFocusIn] = useState(false);
  const shown = !idle || focusIn;
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: the focus inside keeps full screen's controls up; nothing is done here
    <div
      className={cn(
        "flex gap-1.5 transition-opacity duration-200 ease-out",
        shown ? "opacity-100" : "pointer-events-none opacity-0",
      )}
      onFocus={() => setFocusIn(true)}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocusIn(false);
      }}
    >
      {!popout && chat ? (
        <button
          type="button"
          aria-label={
            unread ? t("shareView.chat.unread", { count: unread }) : t("shareView.chat.label")
          }
          onClick={(event) => openRoomChat(event.currentTarget)}
          className={cn(
            "flex h-8 cursor-pointer items-center gap-1.5 rounded-[8px] px-2.5 text-[13px] font-semibold text-bone-100 outline-none",
            "bg-obsidian-950/78 ring-1 ring-white/10",
            "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-obsidian-800/95 active:scale-[0.96]",
            "focus-visible:ring-2 focus-visible:ring-gold-300",
          )}
        >
          <ChatCircleText size={16} weight="bold" aria-hidden />
          {t("shareView.chat.label")}
          {unread ? (
            <span
              aria-hidden
              className="flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-bone-100 px-1 text-[11px] font-bold text-obsidian-950 tabular-nums"
            >
              {unread > 99 ? "99+" : unread}
            </span>
          ) : null}
        </button>
      ) : null}
      {popout ? (
        <StageButton
          label={t("shareView.menu.onTop")}
          pressed={pinned}
          onClick={() => {
            setPinned(!pinned);
            keepOnTop(!pinned);
          }}
        >
          {pinned ? <PushPinSlash size={16} weight="bold" /> : <PushPin size={16} weight="bold" />}
        </StageButton>
      ) : full ? null : (
        <StageButton label={t("share.stage.shrink")} onClick={() => setSmall(true)}>
          <ArrowsInSimple size={16} weight="bold" />
        </StageButton>
      )}
      {full ? (
        <StageButton label={t("shareView.menu.exitFullscreen")} onClick={leaveFullscreen}>
          <CornersIn size={16} weight="bold" />
        </StageButton>
      ) : null}
    </div>
  );
}

/** The focusable things inside an element, in order. */
function focusables(root: HTMLElement): HTMLElement[] {
  return Array.from(
    root.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
    ),
  );
}

/**
 * "Hâlâ izliyor musun?" over the stage: the focus stays in it (Tab goes round its two buttons) and
 * goes back where it was after it. Esc counts as "Evet" (any touch is an answer, the launcher's rule).
 */
function StillAsk({ watches }: { watches: VoiceWatch[] }) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const panel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    return () => {
      if (before?.isConnected && document.activeElement === document.body) {
        before.focus({ preventScroll: true });
      }
    };
  }, []);
  const onKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      void shareApi.still();
      return;
    }
    if (event.key !== "Tab" || !panel.current) return;
    const items = focusables(panel.current);
    const first = items[0];
    const last = items.at(-1);
    if (!first || !last) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };
  return (
    <motion.div
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="share-still-ask"
      className="absolute inset-0 z-10 flex items-center justify-center bg-obsidian-950/55"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { duration: 0.2 } }}
      exit={{ opacity: 0, transition: { duration: 0.14 } }}
      onKeyDown={onKeyDown}
    >
      <motion.div
        ref={panel}
        className="glass-strong flex flex-col items-center gap-4 rounded-[var(--radius-panel)] px-7 py-6"
        initial={{ opacity: 0, y: reduce ? 0 : 6 }}
        animate={{ opacity: 1, y: 0, transition: { duration: 0.24, ease: [0.16, 1, 0.3, 1] } }}
      >
        <p
          id="share-still-ask"
          className="font-display text-[17px] leading-[1.25] font-semibold text-bone-50"
        >
          {t("share.stage.stillAsk")}
        </p>
        <div className="flex gap-2">
          <Button
            variant="ghost"
            onClick={() => {
              for (const w of watches) void unwatchShare(w.person, w.n);
            }}
          >
            {t("share.stage.stop")}
          </Button>
          <Button variant="primary" autoFocus onClick={() => void shareApi.still()}>
            {t("share.stage.stillYes")}
          </Button>
        </div>
      </motion.div>
    </motion.div>
  );
}

/** A stage tile: a watched share, a share that ended, or the player's own preview. */
type StageTile =
  | { kind: "watch"; key: string; watch: VoiceWatch }
  | { kind: "ended"; key: string; person: string }
  | { kind: "own"; key: string; n: number };

/** Esc in full screen leaves it, unless a menu, a dialog or a field has the key. */
function useEscLeavesFull(full: boolean) {
  useEffect(() => {
    if (!full) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      const target = event.target as Element | null;
      if (
        target?.closest?.(
          '[role="menu"], [role="dialog"], [role="alertdialog"], [data-radix-popper-content-wrapper], input, textarea',
        )
      ) {
        return;
      }
      leaveFullscreen();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [full]);
}

/** The stage's content: the tiles, the controls, the chat peek, the question. */
export function StageBody({
  watches,
  ended = [],
  own = null,
  popout,
  dock = 0,
}: {
  watches: VoiceWatch[];
  ended?: EndedShare[];
  own?: number | null;
  popout: boolean;
  /** The docked room panel's width (the page stage keeps clear of it). */
  dock?: number;
}) {
  const share = useShare();
  const full = useShareUi((s) => s.full);
  const forward = useShareUi((s) => s.forward);
  const { idle, wake } = useIdle(full);
  useEscLeavesFull(full);

  const tiles: StageTile[] = [
    ...watches.map(
      (watch): StageTile => ({ kind: "watch", key: shareKey(watch.person, watch.n), watch }),
    ),
    ...ended
      .filter((e) => !watches.some((w) => shareKey(w.person, w.n) === e.key))
      .map((e): StageTile => ({ kind: "ended", key: e.key, person: e.person })),
    ...(own !== null ? [{ kind: "own", key: "own", n: own } satisfies StageTile] : []),
  ];
  const two = watches.length > 1;
  const lead = tiles.length > 1 ? tiles.find((tile) => tile.key === forward) : undefined;
  const rest = lead ? tiles.filter((tile) => tile !== lead) : tiles;

  const render = (tile: StageTile, index: number, className?: string) => {
    switch (tile.kind) {
      case "watch":
        return (
          <StreamTile
            watch={tile.watch}
            live={share.live.find((l) => l.person === tile.watch.person && l.n === tile.watch.n)}
            full={full}
            idle={idle}
            two={two}
            first={index === 0}
            className={className}
          />
        );
      case "ended":
        return <EndedTile person={tile.person} className={className} />;
      case "own":
        return (
          <OwnTile
            n={tile.n}
            full={full}
            idle={idle}
            onClose={hideOwnTile}
            className={className}
          />
        );
    }
  };

  const tileMotion = {
    initial: { opacity: 0 },
    animate: { opacity: 1, transition: { duration: 0.2 } },
    exit: { opacity: 0, transition: { duration: 0.16 } },
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: any input wakes full screen's resting controls; nothing is done here
    <div
      data-idle={idle ? "" : undefined}
      className={cn(
        "group/stage relative flex size-full min-h-0 flex-col p-3",
        idle && "cursor-none",
      )}
      style={dock ? { paddingRight: 12 + dock } : undefined}
      onPointerMove={wake}
      onPointerDown={wake}
      onWheel={wake}
      onKeyDown={wake}
    >
      {lead ? (
        <div className="flex min-h-0 flex-1 gap-3">
          {/* The forward one changes at once: two big tiles crossing would push each other aside. */}
          <div key={lead.key} className="flex min-h-0 min-w-0 flex-1">
            {render(lead, 0, "flex-1")}
          </div>
          <div className="flex w-[clamp(200px,24%,340px)] shrink-0 flex-col gap-3">
            <AnimatePresence initial={false}>
              {rest.map((tile, i) => (
                <motion.div key={tile.key} className="flex aspect-video w-full" {...tileMotion}>
                  {render(tile, i + 1, "flex-1")}
                </motion.div>
              ))}
            </AnimatePresence>
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 gap-3">
          <AnimatePresence initial={false}>
            {rest.map((tile, i) => (
              <motion.div key={tile.key} className="flex min-h-0 min-w-0 flex-1" {...tileMotion}>
                {render(tile, i, "flex-1")}
              </motion.div>
            ))}
          </AnimatePresence>
        </div>
      )}
      <div
        className="absolute top-5 right-5 flex flex-col items-end gap-2"
        style={dock ? { right: 20 + dock } : undefined}
      >
        <StageControls popout={popout} full={full} idle={idle} />
        {popout || (dock && !full) ? null : <RoomChatPeek />}
      </div>
      <AnimatePresence>{share.stillAsking ? <StillAsk watches={watches} /> : null}</AnimatePresence>
    </div>
  );
}

/** The layer the corner's picture asks for ("Yayın Kalitesi" at its width). */
function useCornerLayer(watch: VoiceWatch) {
  const quality: ShareViewQuality = useStreamEnv((s) => s.viewQuality);
  const layer = layerFor({
    quality,
    width: CORNER_PX,
    weakHeld: false,
    zoomed: false,
    hasSmall: true,
  });
  useEffect(() => {
    if (watch.phase === "held" || watch.layer === layer) return;
    void shareApi.layer(watch.person, watch.n, layer).catch(() => {});
  }, [watch.person, watch.n, watch.layer, watch.phase, layer]);
}

/** The corner picture while the stage is small: its sound, back to big, or stop. */
function CornerStage({ watch, live }: { watch: VoiceWatch; live: VoiceLiveShare | undefined }) {
  const { t } = useTranslation();
  useCornerLayer(watch);
  const name = useShareName(watch.person);
  const face = useFace(watch.person);
  const setSmall = useShareUi((s) => s.setSmall);
  const asking = useShare().stillAsking;
  const { onMenu } = useMenuCount();
  const st = live?.st ?? "live";
  const paused = watch.phase === "live" && st === "hold";
  const note =
    watch.phase === "connecting"
      ? t("share.stage.connecting")
      : paused
        ? t("shareView.state.hold")
        : watch.phase === "live" && st === "min"
          ? t("shareView.state.min")
          : null;
  return (
    <StreamMenu person={watch.person} n={watch.n} title={name} onOpenChange={onMenu}>
      <div className="group/tile relative size-full overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950 ring-1 ring-white/12">
        {watch.phase !== "live" || paused ? <StreamBackdrop face={face} /> : null}
        <button
          type="button"
          aria-label={t("shareView.corner.grow", { name })}
          onClick={() => setSmall(false)}
          className="absolute inset-0 cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-inset"
        >
          {/* Whole, never cropped: a shared window's edges matter; the box's obsidian letterboxes it. */}
          {watch.phase === "held" || paused ? null : (
            <ShareVideo
              person={watch.person}
              n={watch.n}
              fit="contain"
              className={st === "min" ? "blur-[12px] brightness-75" : undefined}
            />
          )}
        </button>
        {note ? (
          <p className="pointer-events-none absolute inset-x-0 top-1/2 -translate-y-1/2 px-4 text-center text-[12.5px] font-medium text-bone-100">
            {note}
          </p>
        ) : null}
        <span className="pointer-events-none absolute bottom-1.5 left-1.5 max-w-[60%] truncate rounded-[6px] bg-obsidian-950/78 px-2 py-1 text-[12px] leading-[1.3] font-semibold text-bone-50">
          {name}
        </span>
        <span className="absolute top-1.5 right-1.5 flex gap-1">
          <SoundButton person={watch.person} side="bottom" onOpenChange={onMenu} />
          <StageButton label={t("share.stage.grow")} onClick={() => setSmall(false)}>
            <ArrowsOut size={15} weight="bold" />
          </StageButton>
          <StageButton
            label={t("share.stage.stop")}
            onClick={() => void unwatchShare(watch.person, watch.n)}
          >
            <X size={15} weight="bold" />
          </StageButton>
        </span>
        {/* "Hâlâ izliyor musun?" in the corner too: the player may have it there for half an hour. */}
        {asking ? (
          <div
            role="alertdialog"
            aria-labelledby="share-still-corner"
            className="absolute inset-0 flex flex-col items-center justify-center gap-2.5 bg-obsidian-950/82 px-4"
          >
            <p
              id="share-still-corner"
              className="text-center text-[13.5px] leading-[1.3] font-semibold text-bone-50"
            >
              {t("share.stage.stillAsk")}
            </p>
            <Button size="sm" variant="primary" onClick={() => void shareApi.still()}>
              {t("share.stage.stillYes")}
            </Button>
          </div>
        ) : null}
      </div>
    </StreamMenu>
  );
}

/** The stage in the launcher's window (nothing while nothing is watched or shown here). */
export function ShareStage() {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const share = useShare();
  const me = useVoiceUi((s) => s.state.room?.me.id ?? null);
  const small = useShareUi((s) => s.small);
  const full = useShareUi((s) => s.full);
  const own = useOwnTile();
  const dock = useStageDock();
  const main = share.surface === "main";
  const watches = main ? stageWatches(share, me) : [];
  const ended = useEndedShares(watches, share.live, share.on);
  // The player's own preview, while that share lives.
  const ownShown = own !== null && share.mine.some((m) => m.n === own) ? own : null;
  useEffect(() => {
    if (own !== null && ownShown === null) hideOwnTile();
  }, [own, ownShown]);
  const shown = watches.length > 0 || ended.length > 0 || ownShown !== null;
  const first = watches[0];
  const corner = shown && small && !full && first !== undefined;
  const mode: StageMode = !shown ? null : full ? "full" : corner ? "corner" : "page";
  const cornerName = useShareName(first?.person ?? "");

  // SQOL-U3 docks the room panel beside the page stage.
  useLayoutEffect(() => {
    useShareStage.setState({ mode });
  }, [mode]);
  // The preview's toast over the stage (`toast=1`): toasts stay above full screen.
  useEffect(() => {
    if (!isTauri() && param("toast") === "1") toast(t("share.notice.watchFailed"));
  }, [t]);
  useEffect(() => () => useShareStage.setState({ mode: null }), []);

  // Full screen, the corner and a share brought forward end with the stage (not before it came).
  const was = useRef(shown);
  useEffect(() => {
    const closed = was.current && !shown;
    was.current = shown;
    if (!closed) return;
    const ui = useShareUi.getState();
    if (ui.full) leaveFullscreen();
    useShareUi.setState({ small: false, forward: null, forwardBefore: undefined });
  }, [shown]);

  return (
    <AnimatePresence>
      {corner && first ? (
        <motion.section
          key="corner"
          aria-label={t("share.stage.label", { name: cornerName })}
          className="fixed right-4 bottom-[calc(var(--spacing-commandbar)+12px)] z-20 aspect-video w-[320px] shadow-[0_18px_50px_rgb(63_10_10/0.45)]"
          initial={{ opacity: 0, scale: reduce ? 1 : 0.96 }}
          animate={{
            opacity: 1,
            scale: 1,
            transition: { duration: 0.22, ease: [0.16, 1, 0.3, 1] },
          }}
          exit={{ opacity: 0, scale: reduce ? 1 : 0.97, transition: { duration: 0.14 } }}
        >
          <CornerStage
            watch={first}
            live={share.live.find((l) => l.person === first.person && l.n === first.n)}
          />
        </motion.section>
      ) : shown ? (
        <motion.section
          key="stage"
          aria-label={t("shareView.stage.label")}
          className={cn(
            "fixed flex flex-col bg-obsidian-950/92 backdrop-blur-md",
            full ? "inset-0 z-[35]" : "top-titlebar right-0 bottom-0 left-rail z-20",
          )}
          initial={{ opacity: 0, y: reduce ? 0 : 8 }}
          animate={{ opacity: 1, y: 0, transition: { duration: 0.26, ease: [0.16, 1, 0.3, 1] } }}
          exit={{ opacity: 0, transition: { duration: 0.16 } }}
        >
          <h2 className="sr-only">
            <Screencast aria-hidden />
            {t("shareView.stage.label")}
          </h2>
          <StageBody
            watches={watches}
            ended={ended}
            own={ownShown}
            popout={false}
            dock={full ? 0 : dock}
          />
        </motion.section>
      ) : null}
    </AnimatePresence>
  );
}
