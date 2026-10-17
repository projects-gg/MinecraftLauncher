// An instance card in the library as a drop target. While files an instance takes in are
// dragged over the library, every card on screen takes part and the window zone steps aside:
// the card under the cursor turns gold like a DropZone, lifts a little and its last line says
// "Buraya bırak"; a card that cannot take the files (mods on a vanilla instance) stays neutral
// and says why. The files go into the card they are dropped on; a drop beside the cards goes
// into none and its notice says so. The library's header says how it works (`CardsDropGuide`).
// Only transform and opacity move; reduced motion and Performance mode keep the fades, and
// Performance mode drops the glow and the shadow (docs/design.md §5).
import { Prohibit, TrayArrowDown } from "@phosphor-icons/react";
import { cn, easeOutExpo, PixelBrackets, useMotionPrefs, usePerf } from "@pl/ui";
import { AnimatePresence, motion, type Transition, type Variants } from "motion/react";
import { type ReactNode, useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { InstanceSummary } from "@/lib/api";
import { CARD_REACH, cardsJoin, cardTakes } from "./cards";
import { useDrop, useDropAnnouncement, useZone } from "./store";
import { type ZoneState, zoneState } from "./zones";

const targetOf = (instance: InstanceSummary) => ({
  id: instance.id,
  modded: instance.loader !== "vanilla",
});

/** Makes an instance card a drop zone: `ref` goes on the card, `id` names it for `CardDrop`. */
export function useCardZone(instance: InstanceSummary) {
  const target = targetOf(instance);
  return useZone({
    layer: "page",
    into: instance,
    reach: CARD_REACH,
    joins: (drag) => cardsJoin(drag.paths, drag.files),
    accepts: (drag) => cardTakes(drag.paths, drag.files, target),
  });
}

/** The card's look in the drag; the cursor moving over other cards does not render it. */
function useCardState(zone: string, instance: InstanceSummary): ZoneState {
  const drag = useDrop((s) => (s.drag !== null && s.active.includes(zone) ? s.drag : null));
  const hovered = useDrop((s) => s.hovered === zone);
  const absorbing = useDrop((s) => s.absorbing === zone);
  return zoneState(zone, {
    active: drag !== null,
    accepts: drag ? cardTakes(drag.paths, drag.files, targetOf(instance)) : true,
    hovered: hovered ? zone : null,
    hoveredAccepts: true,
    absorbing: absorbing ? zone : null,
  });
}

const ease = (duration: number, delay = 0): Transition => ({ duration, ease: easeOutExpo, delay });

/** Seconds to reach each state, as the shared DropZone: the cursor's state takes longest. */
const timing: Record<ZoneState, number> = {
  idle: 0.18,
  ready: 0.18,
  hover: 0.26,
  recede: 0.18,
  refuse: 0.2,
  absorb: 0.22,
};
const states = Object.keys(timing) as ZoneState[];

/** A layer's opacity in each state, eased over that state's time. */
function fade(opacity: Record<ZoneState, number>): Variants {
  return Object.fromEntries(
    states.map((s) => [s, { opacity: opacity[s], transition: ease(timing[s]) }]),
  );
}

// At rest a card shows none of this. Files it takes draw a warm dashed frame; under the cursor
// the frame turns gold, the card glows from its last line and lifts; a card that refuses the
// files keeps a calm neutral frame. The other cards do not react to the cursor.
const neutralFrame = fade({ idle: 0, ready: 0.75, hover: 0, recede: 0.75, refuse: 1, absorb: 0 });
const goldFrame = fade({ idle: 0, ready: 0.4, hover: 1, recede: 0.4, refuse: 0, absorb: 1 });
const glow = fade({ idle: 0, ready: 0, hover: 1, recede: 0, refuse: 0, absorb: 1 });
const shadow = fade({ idle: 0, ready: 0, hover: 1, recede: 0, refuse: 0, absorb: 0 });

const corners = [
  { key: "tl", x: -1, y: -1 },
  { key: "tr", x: 1, y: -1 },
  { key: "bl", x: -1, y: 1 },
  { key: "br", x: 1, y: 1 },
] as const;

/** Brackets lock on to the card under the cursor and close in as it takes the files. */
function bracketVariants(x: number, y: number, still: boolean): Variants {
  const shift = (px: number) => (still ? "none" : `translate(${x * px}px, ${y * px}px)`);
  const away = (seconds: number) => ({
    opacity: 0,
    transform: shift(6),
    transition: ease(seconds),
  });
  return {
    idle: away(0.14),
    ready: away(0.14),
    recede: away(0.14),
    refuse: away(0.14),
    hover: { opacity: 1, transform: shift(0), transition: ease(0.28) },
    absorb: { opacity: 0, transform: shift(-5), transition: ease(0.24) },
  };
}

// The last line swaps in place: the line it replaces leaves quickly and the new one comes in a
// beat later (nothing moves: the lines share one grid cell).
const lineIn = ease(0.18, 0.04);
const lineOut = ease(0.12);
const line = (shownIn: ZoneState[]): Variants =>
  Object.fromEntries(
    states.map((s) => {
      const shown = shownIn.includes(s);
      return [s, { opacity: shown ? 1 : 0, transition: shown ? lineIn : lineOut }];
    }),
  );
const restLine = line(["idle", "ready", "recede"]);
const takeLine = line(["hover", "absorb"]);
const refuseLine = line(["refuse"]);
/** A card that takes none of the files steps back (its last line says why). */
const identityFade = fade({ idle: 1, ready: 1, hover: 1, recede: 1, refuse: 0.7, absorb: 1 });

/** The tray's arrow settles in as "Buraya bırak" appears and sinks as the card takes the files. */
function markVariants(still: boolean): Variants {
  const at = (px: number) => (still ? "none" : `translateY(${px}px)`);
  const variants: Variants = Object.fromEntries(
    states.map((s) => [s, { transform: at(-3), transition: ease(0.12) }]),
  );
  variants.hover = { transform: at(0), transition: ease(0.26, 0.04) };
  variants.absorb = { transform: at(2), transition: ease(0.16) };
  return variants;
}

/**
 * Everything an instance card shows of a file drag, around its content: `identity` (icon,
 * name, version) and `lastLine` (play time and actions, which swap for "Buraya bırak" or the
 * reason the card takes none). The frames carry `data-drop`, so the card lifts and settles
 * through CSS (`:has`): a drag never renders the card itself again, and the list never
 * measures it for its layout animation meanwhile. A selected card's pixel brackets step aside
 * during the drag: the files go where the cursor is, not to the selection.
 */
export function CardDrop({
  zone,
  instance,
  selected,
  identity,
  lastLine,
}: {
  zone: string;
  instance: InstanceSummary;
  selected: boolean;
  identity: ReactNode;
  lastLine: ReactNode;
}) {
  const { t } = useTranslation();
  const state = useCardState(zone, instance);
  const still = !useMotionPrefs().layout;
  const { performanceMode } = usePerf();
  const brackets = useMemo(() => corners.map(({ x, y }) => bracketVariants(x, y, still)), [still]);
  const mark = useMemo(() => markVariants(still), [still]);
  return (
    <>
      {/* Under the card's content, over its glass. */}
      <motion.span
        aria-hidden
        data-drop={state}
        initial={false}
        animate={state}
        className="pointer-events-none absolute -inset-px -z-10 rounded-[inherit]"
      >
        {performanceMode ? null : (
          <>
            <motion.span
              variants={shadow}
              className="absolute inset-0 rounded-[inherit] shadow-[0_18px_44px_rgb(34_6_6/0.45)]"
            />
            <motion.span
              variants={glow}
              className="absolute inset-0 rounded-[inherit] bg-[radial-gradient(90%_75%_at_50%_100%,rgb(232_154_44/0.16),transparent_72%)]"
            />
          </>
        )}
        <motion.span
          variants={neutralFrame}
          className="absolute inset-0 rounded-[inherit] border-[1.5px] border-dashed border-white/26"
        />
        <motion.span
          variants={goldFrame}
          className="absolute inset-0 rounded-[inherit] border-[1.5px] border-dashed border-gold-400/90"
        />
        {corners.map((corner, index) => (
          <motion.span
            key={corner.key}
            variants={brackets[index]}
            className="absolute size-[12px]"
            style={{
              [corner.y < 0 ? "top" : "bottom"]: -5,
              [corner.x < 0 ? "left" : "right"]: -5,
            }}
          >
            <span
              className={cn(
                "absolute h-[2px] w-full bg-gold-300",
                corner.y < 0 ? "top-0" : "bottom-0",
              )}
            />
            <span
              className={cn(
                "absolute h-full w-[2px] bg-gold-300",
                corner.x < 0 ? "left-0" : "right-0",
              )}
            />
          </motion.span>
        ))}
      </motion.span>
      {selected ? (
        <motion.span
          aria-hidden
          className="pointer-events-none absolute inset-0"
          initial={false}
          animate={{ opacity: state === "idle" ? 1 : 0, transition: ease(0.16) }}
        >
          <PixelBrackets />
        </motion.span>
      ) : null}
      <motion.div initial={false} animate={state} variants={identityFade}>
        {identity}
      </motion.div>
      <motion.div initial={false} animate={state} className="mt-2.5 grid *:[grid-area:1/1]">
        <motion.div variants={restLine} className="grid *:[grid-area:1/1]">
          {lastLine}
        </motion.div>
        <motion.div
          aria-hidden
          variants={takeLine}
          className="pointer-events-none flex h-8 items-center justify-center gap-1.5 rounded-[6px] bg-gold-500/14 text-[13px] font-semibold text-gold-200 ring-1 ring-gold-400/30 ring-inset"
        >
          <motion.span variants={mark} className="flex text-gold-300">
            <TrayArrowDown size={16} weight="duotone" />
          </motion.span>
          {t("drop.zone.title")}
        </motion.div>
        <motion.div
          aria-hidden
          variants={refuseLine}
          className="pointer-events-none flex h-8 items-center justify-center gap-1.5 rounded-[6px] bg-white/5 text-[12.5px] font-medium text-ash-300"
        >
          <Prohibit size={14} className="shrink-0 text-ash-400" />
          <span className="truncate">{t("libraryDrop.card.needsLoader")}</span>
        </motion.div>
      </motion.div>
    </>
  );
}

/**
 * The library header's line under the title (`rest`: the instance count). While files go onto
 * the cards it says how: "Dosyaları bir kurulumun üstüne bırak". The guide is out of the flow,
 * so nothing in the header moves.
 */
export function CardsDropGuide({ rest, className }: { rest: string; className?: string }) {
  const { t } = useTranslation();
  const count = useDrop((s) => (s.cards && s.drag ? s.drag.paths.length : 0));
  const dragId = useDrop((s) => (s.cards ? s.drag?.id : undefined));
  // Said once the files are read: a modpack in a ".zip" goes to the window zone after all.
  const known = useDrop((s) => s.cards && Boolean(s.drag?.files));
  const guide = t("libraryDrop.guide", { count: Math.max(count, 1) });
  useDropAnnouncement(count > 0 && known ? guide : null, dragId);
  return (
    <p className={cn("relative", className)}>
      <motion.span
        className="block"
        initial={false}
        animate={{ opacity: count > 0 ? 0 : 1, transition: count > 0 ? lineOut : lineIn }}
      >
        {rest}
      </motion.span>
      <AnimatePresence>
        {count > 0 ? (
          <motion.span
            key="guide"
            className="absolute inset-y-0 left-0 flex items-center gap-1.5 whitespace-nowrap text-gold-300"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: lineIn }}
            exit={{ opacity: 0, transition: lineOut }}
          >
            <TrayArrowDown aria-hidden size={14} weight="bold" className="shrink-0" />
            {guide}
          </motion.span>
        ) : null}
      </AnimatePresence>
    </p>
  );
}
