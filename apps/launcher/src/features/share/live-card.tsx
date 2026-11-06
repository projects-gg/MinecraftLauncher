// "YAYINDA" beside a person who shares, and the card it opens (SHARE-LIVE; the owner, 6 Nov: "discord
// gibi uçtan uca"). The tag is the one live mark (no second icon beside it): jade, the real state's
// colour, quiet, never a light or a pulse. The mouse resting on it for 350 ms, the keyboard's focus
// on what carries it, or a touch on it opens the card: the stream's last small picture (16:9) with
// the sharer's name, what they share and how many watch, and "İzle" in the room, "Katıl ve izle" in
// a crew room the player may join. Where the player may not go the card shows no picture at all:
// the sharer's face large and blurred, a lock and one line. Until a picture comes the blurred face
// waits alone. One card at a time, with the crew tags' cards (their hover intent, badge-intent.ts).
import { Headset, LockSimple } from "@phosphor-icons/react";
import { Button, Chip, cn, easeOutExpo, focusPanel, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { Popover } from "radix-ui";
import {
  type FocusEvent,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  type ReactNode,
  type RefObject,
  type SyntheticEvent,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import {
  blurChip,
  closeCard,
  closedWithFocus,
  enterCard,
  focusChip,
  hoverChip,
  leaveCard,
  leaveChip,
  pressChip,
  registerCard,
  useBadgeIntent,
} from "@/features/social/badge-intent";
import { SocialFace } from "@/features/social/social-face";
import { useSocialUi } from "@/features/social/store";
import { namesLine, type VoiceFace } from "@/features/voice/rules";
import { useFaces } from "@/features/voice/store";
import { joinAndWatch, watchShare } from "./actions";
import { type LiveTarget, useShareThumb } from "./live";

// --- the tag -------------------------------------------------------------------------------------

/**
 * "YAYINDA" / "LIVE": the chip of a crew tag's size, jade at 15 % under jade words (the "Oyunda"
 * chip's tone), chamfered as every chip is. It never shrinks; the name beside it truncates.
 */
export function LiveTag({ className }: { className?: string }) {
  const { t } = useTranslation();
  return (
    <Chip
      tone="jade"
      aria-hidden
      className={cn("h-[18px] px-1.5 pt-px text-[11px] tracking-[0.06em]", className)}
    >
      {t("share.tag")}
    </Chip>
  );
}

/**
 * The tag comes and goes where it stands: a crew tag's quick fade from 96 % (160 ms in, 110 ms
 * out), a fade alone with less motion.
 */
export function LiveAppear({ show, children }: { show: boolean; children: ReactNode }) {
  const { reduce } = useMotionPrefs();
  return (
    <AnimatePresence initial={false}>
      {show ? (
        <motion.span
          key="live"
          className="flex shrink-0"
          style={{ transformOrigin: "left center" }}
          initial={reduce ? { opacity: 0 } : { opacity: 0, transform: "scale(0.96)" }}
          animate={{
            opacity: 1,
            ...(reduce ? {} : { transform: "scale(1)" }),
            transition: { duration: 0.16, ease: easeOutExpo },
          }}
          exit={{
            opacity: 0,
            ...(reduce ? {} : { transform: "scale(0.96)" }),
            transition: { duration: 0.11, ease: easeOutExpo },
          }}
        >
          {children}
        </motion.span>
      ) : null}
    </AnimatePresence>
  );
}

// --- the hover intent ------------------------------------------------------------------------------

export interface LivePlacement {
  side: "left" | "right" | "top" | "bottom";
  align?: "start" | "center" | "end";
  sideOffset?: number;
  /** The card sits beside this (a row in a side panel); else beside the tag. */
  anchor?: RefObject<HTMLElement | null>;
}

/** A card sits in a portal but in its row's React tree: its presses and keys stay in it. */
const stop = (event: SyntheticEvent) => event.stopPropagation();
const KEEP_INSIDE = {
  onClick: stop,
  onDoubleClick: stop,
  onPointerDown: stop,
  onPointerUp: stop,
  onMouseDown: stop,
  onKeyDown: stop,
  onContextMenu: stop,
};

/** Popovers' motion (docs/design.md §5): 160 ms from 96 % at the tag, 110 ms out; fades only with
 * less motion. A card replaced by another one's leaves at once. */
function cardVariants(reduce: boolean) {
  return {
    hidden: reduce ? { opacity: 0 } : { opacity: 0, transform: "scale(0.96)" },
    shown: {
      opacity: 1,
      ...(reduce ? {} : { transform: "scale(1)" }),
      transition: { duration: 0.16, ease: easeOutExpo },
    },
    gone: (swapped: boolean) =>
      swapped
        ? { opacity: 0, transition: { duration: 0 } }
        : {
            opacity: 0,
            ...(reduce ? {} : { transform: "scale(0.98)" }),
            transition: { duration: 0.11, ease: easeOutExpo },
          },
  };
}

export interface LivePreview {
  /** On the tag: the mouse resting on it opens the card, a touch pins it (not the row's press). */
  tagProps: {
    ref: RefObject<HTMLSpanElement | null>;
    onPointerEnter: (event: PointerEvent<HTMLElement>) => void;
    onPointerLeave: (event: PointerEvent<HTMLElement>) => void;
    onPointerDown: (event: PointerEvent<HTMLElement>) => void;
    onClick: (event: MouseEvent<HTMLElement>) => void;
  };
  /** On what takes the keyboard's focus (the row, or the tag's own button): the card after 350 ms. */
  focusProps: {
    onFocus: (event: FocusEvent<HTMLElement>) => void;
    onBlur: (event: FocusEvent<HTMLElement>) => void;
  };
  /** A press on a tag of its own (a button): pins the card; from the keyboard the focus goes in. */
  press: (event: MouseEvent<HTMLElement>) => void;
  /**
   * On the row that carries the tag: → opens the card with the focus in it (its "İzle" or "Katıl ve izle"); true when
   * it took the key. The row says so (`aria-keyshortcuts`).
   */
  keyIn: (event: KeyboardEvent<HTMLElement>) => boolean;
  /** Who is live, in words ("KorAtesi yayında"). */
  label: string;
  /** The card in its layer: render it anywhere beside the tag. */
  card: ReactNode;
}

/** The sharers' faces; the player's own share shows the player as their own row does. */
function useLiveFaces(targets: LiveTarget[]): VoiceFace[] {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me);
  const faces = useFaces(targets.map((target) => target.person));
  return faces.map((face, i) =>
    targets[i]?.offer === "own"
      ? {
          ...face,
          name: me?.name ?? t("voice.room.you"),
          avatar: me?.avatar,
          avatarUrl: me?.avatarUrl,
        }
      : face,
  );
}

/** The tag's card for `targets` (none: no card, an open one closes). */
export function useLivePreview(targets: LiveTarget[], placement: LivePlacement): LivePreview {
  const { t } = useTranslation();
  const id = useId();
  const { reduce } = useMotionPrefs();
  const status = useBadgeIntent((s) =>
    s.open === id ? (s.instant ? "instant" : "open") : s.open === null ? "closed" : "other",
  );
  const focusAsk = useBadgeIntent((s) => (s.focus?.id === id ? s.focus.n : null));
  const live = targets.length > 0;
  const open = live && (status === "open" || status === "instant");
  const tagRef = useRef<HTMLSpanElement>(null);
  const cardRef = useRef<HTMLDivElement | null>(null);
  const touched = useRef(false);
  const faces = useLiveFaces(targets);

  // The stream ended or the row went: its card goes with it.
  useEffect(() => {
    if (!live) closeCard(id);
  }, [live, id]);
  useEffect(() => () => closeCard(id), [id]);

  const tagProps: LivePreview["tagProps"] = {
    ref: tagRef,
    onPointerEnter: (event) => {
      if (event.pointerType === "mouse") hoverChip(id);
    },
    onPointerLeave: (event) => {
      if (event.pointerType === "mouse") leaveChip(id, { x: event.clientX, y: event.clientY });
    },
    onPointerDown: (event) => {
      touched.current = event.pointerType !== "mouse";
      if (touched.current) event.stopPropagation();
    },
    onClick: (event) => {
      if (!touched.current) return;
      touched.current = false;
      event.preventDefault();
      event.stopPropagation();
      pressChip(id, false);
    },
  };

  const focusProps: LivePreview["focusProps"] = {
    onFocus: (event) => {
      if (live && event.currentTarget.matches(":focus-visible")) focusChip(id);
    },
    onBlur: (event) => {
      blurChip(id);
      // A preview the keyboard opened leaves with the focus, unless the focus went into it.
      const state = useBadgeIntent.getState();
      const to = event.relatedTarget;
      const inside = to instanceof Node && (cardRef.current?.contains(to) ?? false);
      if (state.open === id && !state.pinned && !inside) closeCard(id);
    },
  };

  const keyIn = (event: KeyboardEvent<HTMLElement>) => {
    if (!live || event.key !== "ArrowRight" || event.altKey || event.ctrlKey || event.metaKey)
      return false;
    event.preventDefault();
    pressChip(id, true);
    return true;
  };

  const press = (event: MouseEvent<HTMLElement>) => {
    event.preventDefault();
    event.stopPropagation();
    pressChip(id, event.detail === 0);
  };

  const names = faces.map((face) => face.name);
  const label = t("share.preview.label", { names: namesLine(names, t), count: names.length });

  const card = (
    <Popover.Root
      open={open}
      onOpenChange={(next) => {
        if (!next) closeCard(id);
      }}
    >
      <Popover.Anchor virtualRef={placement.anchor ?? tagRef} />
      <AnimatePresence custom={status === "other"}>
        {open ? (
          <Popover.Portal forceMount>
            <Popover.Content
              forceMount
              asChild
              side={placement.side}
              align={placement.align ?? "center"}
              sideOffset={placement.sideOffset ?? 8}
              collisionPadding={12}
              aria-label={label}
              // The focus moves only after keys (above); a preview under the pointer leaves it.
              onOpenAutoFocus={(event) => event.preventDefault()}
              onCloseAutoFocus={(event) => {
                event.preventDefault();
                if (!closedWithFocus(id)) return;
                const back = tagRef.current?.closest<HTMLElement>("button, [tabindex]");
                if (back?.isConnected) back.focus({ preventScroll: true });
              }}
            >
              <motion.div
                ref={(element: HTMLDivElement | null) => {
                  cardRef.current = element;
                  registerCard(id, element);
                }}
                tabIndex={-1}
                data-live-card
                className="z-50 w-[296px] overflow-hidden rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-[6px] shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:bg-obsidian-900 [html[data-perf=on]_&]:backdrop-blur-none"
                style={{ transformOrigin: "var(--radix-popover-content-transform-origin)" }}
                variants={cardVariants(reduce)}
                custom={false}
                initial={status === "instant" ? false : "hidden"}
                animate="shown"
                exit="gone"
                onPointerEnter={(event) => {
                  if (event.pointerType === "mouse") enterCard(id);
                }}
                onPointerLeave={(event) => {
                  if (event.pointerType === "mouse") leaveCard(id);
                }}
                {...KEEP_INSIDE}
              >
                <LiveCard
                  targets={targets}
                  faces={faces}
                  focusAsk={focusAsk}
                  onDone={() => closeCard(id)}
                />
              </motion.div>
            </Popover.Content>
          </Popover.Portal>
        ) : null}
      </AnimatePresence>
    </Popover.Root>
  );

  return { tagProps, focusProps, press, keyIn, label, card };
}

// --- the tag as a button of its own ------------------------------------------------------------------

/**
 * A tag that is not part of a row's button (the crew page's rooms, the call's stage): a button of
 * its own, so the keyboard reaches it (its focus previews, Enter goes into the card) and a press
 * pins the card.
 */
export function LiveTagButton({
  targets,
  placement,
  className,
}: {
  targets: LiveTarget[];
  placement: LivePlacement;
  className?: string;
}) {
  const preview = useLivePreview(targets, placement);
  if (!targets.length) return null;
  const { ref, onPointerEnter, onPointerLeave } = preview.tagProps;
  return (
    <>
      <button
        type="button"
        data-live-tag
        aria-label={preview.label}
        className={cn(
          "group/live relative inline-flex shrink-0 cursor-pointer outline-none transition-[transform,scale] duration-150 ease-out active:scale-[0.94]",
          className,
        )}
        onClick={preview.press}
        onPointerEnter={onPointerEnter}
        onPointerLeave={onPointerLeave}
        {...preview.focusProps}
      >
        <span
          aria-hidden
          className="chamfer absolute -inset-[2px] bg-gold-300 opacity-0 transition-opacity duration-150 [--cut:6px] group-focus-visible/live:opacity-100"
        />
        {/* The tint is see-through: a solid backing keeps the ring a ring. */}
        <span aria-hidden className="chamfer absolute inset-0 bg-obsidian-900 [--cut:5px]" />
        <span ref={ref} className="relative inline-flex">
          <LiveTag className="transition-[filter] duration-150 group-hover/live:brightness-125" />
        </span>
      </button>
      {preview.card}
    </>
  );
}

// --- the card --------------------------------------------------------------------------------------

/** Under the name: what is shared and how many watch, or the crew room it is in. */
function metaLine(
  target: LiveTarget,
  t: (key: string, options?: Record<string, unknown>) => string,
): string | null {
  if (target.kind === null) return target.roomName;
  const what = t(`share.kind.${target.kind}`);
  if (target.offer === "watching") return `${what}, ${t("share.preview.watching")}`;
  if (target.offer === "own") {
    return `${what}, ${
      target.watchers
        ? t("share.live.watchers", { count: target.watchers })
        : t("share.live.resting")
    }`;
  }
  return target.watchers
    ? `${what}, ${t("share.live.watchers", { count: target.watchers })}`
    : what;
}

/** The blurred face behind the picture: the placeholder, and the locked card's whole backdrop. */
function Backdrop({ face, locked }: { face: VoiceFace | undefined; locked: boolean }) {
  return (
    <div aria-hidden className="absolute inset-0">
      {face ? (
        <span className="absolute top-1/2 left-1/2 flex -translate-x-1/2 -translate-y-1/2 scale-[1.25] blur-[18px] saturate-[1.15]">
          <SocialFace source={face} size={296} />
        </span>
      ) : null}
      <span
        className={cn("absolute inset-0", locked ? "bg-obsidian-950/60" : "bg-obsidian-950/35")}
      />
    </div>
  );
}

function LiveCard({
  targets,
  faces,
  focusAsk,
  onDone,
}: {
  targets: LiveTarget[];
  faces: VoiceFace[];
  /** The keyboard asked for the focus in the card (it changes with each ask). */
  focusAsk: number | null;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const rootRef = useRef<HTMLDivElement>(null);
  const [index, setIndex] = useState(0);
  const at = Math.min(index, targets.length - 1);
  const target = targets[at] ?? null;
  const face = faces[at];
  const { thumb, key } = useShareThumb(target, true);
  // Enter (or a press from the keyboard) takes the focus in: its action, else the card itself. Here
  // and not in the hook: the layer's portal mounts the card a render after it opened.
  useEffect(() => {
    if (focusAsk === null) return;
    const root = rootRef.current;
    const card = root?.closest<HTMLElement>("[data-live-card]");
    if (!root || !card) return;
    focusPanel(root.querySelector<HTMLElement>("button:not([disabled])") ?? card);
  }, [focusAsk]);
  if (!target) return null;
  const name = face?.name ?? "";
  // Where the player may not go: no picture, one line.
  const lockedLine =
    target.offer === "full"
      ? t("voice.refused.roomFull")
      : target.offer === "private"
        ? t("share.preview.private")
        : thumb?.kind === "locked"
          ? t("share.preview.locked")
          : null;
  const meta = metaLine(target, t);
  const action =
    target.offer === "watch" && target.n !== null ? (
      <Button
        size="sm"
        variant="secondary"
        aria-label={t("share.row.watchLabel", { name })}
        onClick={() => {
          if (target.n !== null) void watchShare(target.person, target.n);
          onDone();
        }}
      >
        {t("share.row.watch")}
      </Button>
    ) : target.offer === "join" && lockedLine === null && target.crew && target.room ? (
      <Button
        size="sm"
        variant="secondary"
        onClick={() => {
          if (target.crew && target.room) {
            void joinAndWatch(
              target.crew,
              { id: target.room, general: target.general },
              target.person,
            );
          }
          onDone();
        }}
      >
        {t("share.preview.joinWatch")}
      </Button>
    ) : null;

  return (
    <div ref={rootRef} className="flex flex-col">
      {targets.length > 1 ? (
        <div role="tablist" aria-label={t("share.preview.sharers")} className="mb-[6px] flex gap-1">
          {targets.map((each, i) => {
            const chosen = i === at;
            const who = faces[i];
            return (
              <button
                key={each.person}
                type="button"
                role="tab"
                aria-selected={chosen}
                onClick={() => setIndex(i)}
                className={cn(
                  "chamfer flex h-7 min-w-0 flex-1 cursor-pointer items-center gap-1.5 px-2 text-[12px] font-semibold outline-none transition-[background-color,color] duration-150 [--cut:5px] focus-visible:bg-white/12",
                  chosen ? "bg-white/[0.1] text-bone-50" : "text-ash-300 hover:bg-white/[0.05]",
                )}
              >
                {who ? <SocialFace source={who} size={16} /> : null}
                <span className="truncate">{who?.name}</span>
              </button>
            );
          })}
        </div>
      ) : null}
      <div className="relative aspect-video w-full overflow-hidden rounded-[5px] bg-obsidian-950">
        <Backdrop face={face} locked={lockedLine !== null} />
        <AnimatePresence initial={false}>
          {lockedLine === null && thumb?.kind === "image" ? (
            <motion.img
              key={key}
              src={thumb.dataUrl}
              alt={t("share.preview.picture", { name })}
              draggable={false}
              className="absolute inset-0 size-full object-cover"
              style={{ zIndex: 1 }}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { duration: 0.2, ease: easeOutExpo } }}
              // The next picture fades in over this one; it goes once it is covered.
              exit={{ opacity: 0, zIndex: 0, transition: { opacity: { delay: 0.2, duration: 0 } } }}
            />
          ) : null}
        </AnimatePresence>
        {lockedLine !== null ? (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 px-8 text-center">
            <LockSimple size={22} weight="bold" aria-hidden className="text-bone-100" />
            <p className="text-[12.5px] leading-[1.35] font-medium text-balance text-bone-100">
              {lockedLine}
            </p>
          </div>
        ) : null}
      </div>
      <div className="flex min-h-[44px] items-center gap-2.5 px-1.5 pt-[6px]">
        {face ? (
          <SocialFace
            source={face}
            size={24}
            className="overflow-hidden rounded-[var(--radius-head)]"
          />
        ) : null}
        <div className="min-w-0 flex-1">
          <p className="truncate text-[13.5px] leading-[1.25] font-semibold text-bone-50">{name}</p>
          {meta ? (
            <p className="flex min-w-0 items-center gap-1 text-[12px] leading-[1.35] text-ash-300">
              {/* Outside the room the line is the voice room it is in: the friends list's headset says so. */}
              {target.kind === null ? (
                <Headset size={12} weight="bold" aria-hidden className="shrink-0" />
              ) : null}
              <span className="truncate">{meta}</span>
            </p>
          ) : null}
        </div>
        {action}
      </div>
    </div>
  );
}
