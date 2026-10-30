// Small pieces world sharing shows in several places (HOST1): the world's mark, "2/8", a row of
// faces, a guest's state, and "Katıl" on a world invite, which says where the join is in place
// ("Hazırlanıyor" → "Oyun açılıyor" → "Bağlanıyor"; host1.md §1.4) without changing its width.
import { Play } from "@phosphor-icons/react";
import { Button, ContentIcon, cn, SwapLabel } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { useOwnLaunch } from "@/features/accounts/use-own-launch";
import { pop, useSocialMotion } from "@/features/social/motion";
import { type FaceSource, SocialFace } from "@/features/social/social-face";
import { api } from "@/lib/api";
import type { GuestState, HostSession, WorldInvite } from "@/lib/api/host-types";
import { pressJoin } from "./actions";
import { type JoinButton, joinButton } from "./rules";

/**
 * The world's mark: its own icon, else our grass block in a chamfered tile (docs/design.md §6,
 * content icons: 2× from 38 px tiles, 1× below; `quiet` one gold step darker, for lists).
 */
export function WorldMark({
  size = 40,
  iconUrl,
  quiet = false,
}: {
  size?: 32 | 40 | 48;
  iconUrl?: string;
  quiet?: boolean;
}) {
  return (
    <span
      className="chamfer flex shrink-0 items-center justify-center overflow-hidden bg-obsidian-800 [--cut:6px]"
      style={{ width: size, height: size }}
    >
      {iconUrl ? (
        <img
          src={iconUrl}
          alt=""
          className="size-full object-cover [image-rendering:pixelated]"
          draggable={false}
        />
      ) : (
        <ContentIcon kind="world" size={size >= 38 ? 32 : 16} quiet={quiet} />
      )}
    </span>
  );
}

/** The session's world's own icon, when the library knows it (its list is cached). */
export function useWorldIcon(
  session: Pick<HostSession, "instanceId" | "folder">,
): string | undefined {
  const worlds = useQuery({
    queryKey: ["worlds", session.instanceId],
    queryFn: () => api.library.worlds(session.instanceId),
    staleTime: 60_000,
  });
  return worlds.data?.find((w) => w.folder === session.folder)?.iconUrl;
}

/**
 * "2/8 kişi" (Tektur, tabular), read out as a sentence. The word says what is counted (LANDING-L: a
 * bare "2/8" beside the picker's "3/7" read as a mistake): the world's people, its host too.
 */
export function PeopleCount({
  count,
  max,
  label,
  className,
}: {
  count: number;
  max: number;
  /** The whole sentence a screen reader hears ("Dünyada 2 kişi, en çok 8"). */
  label: string;
  className?: string;
}) {
  const { t } = useTranslation();
  return (
    <span className={cn("font-display tabular-nums", className)}>
      <span aria-hidden>
        {count}/{max} {t("host.sheet.unit")}
      </span>
      <span className="sr-only">{label}</span>
    </span>
  );
}

/**
 * Up to four faces, the newest at the end: a face that arrives grows in from 85 % where it will
 * stay and the others glide aside (`layout="position"`); reduced motion fades only.
 */
export function Faces({
  sources,
  size = 24,
  surface = "var(--color-obsidian-900)",
  round = false,
}: {
  sources: Array<FaceSource & { id: string }>;
  size?: number;
  /** The ring between overlapping faces: the colour behind them. */
  surface?: string;
  /** Inside a round pill (Home's lines) the faces are round too (SMALL10b). */
  round?: boolean;
}) {
  const { reduce, layout } = useSocialMotion();
  const shown = sources.slice(0, 4);
  const more = sources.length - shown.length;
  return (
    <span className="flex shrink-0 items-center" aria-hidden>
      <AnimatePresence initial={false} mode="popLayout">
        {shown.map((source, index) => (
          <motion.span
            key={source.id}
            layout={layout ? "position" : false}
            transition={{ layout: { type: "spring", stiffness: 420, damping: 36 } }}
            className={cn("relative flex", round ? "rounded-full" : "rounded-[5px]")}
            style={{
              marginLeft: index ? -Math.round(size / 3.5) : 0,
              zIndex: 10 - index,
              boxShadow: `0 0 0 2px ${surface}`,
            }}
            {...pop(reduce, 0.22)}
          >
            <SocialFace
              source={source}
              size={size}
              className={round ? "overflow-hidden rounded-full" : undefined}
            />
          </motion.span>
        ))}
      </AnimatePresence>
      {more > 0 ? (
        <span className="ml-1.5 font-display text-[12px] text-ash-300 tabular-nums">+{more}</span>
      ) : null}
    </span>
  );
}

/** A guest's state beside their name: jade in the world, gold on the way, ash otherwise. */
export function GuestStateText({ state }: { state: GuestState }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  return (
    <span className="relative inline-grid justify-items-end text-[12.5px]">
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span
          key={state}
          className={cn(
            "[grid-area:1/1] whitespace-nowrap",
            state === "in"
              ? "text-jade-400"
              : state === "joining"
                ? "text-gold-300"
                : "text-ash-400",
          )}
          initial={{ opacity: 0, transform: reduce ? "none" : "translateY(4px)" }}
          animate={{
            opacity: 1,
            transform: "translateY(0px)",
            transition: { duration: 0.2, ease: [0.16, 1, 0.3, 1], delay: 0.04 },
          }}
          exit={{
            opacity: 0,
            transform: reduce ? "none" : "translateY(-4px)",
            transition: { duration: 0.1 },
          }}
        >
          {t(`host.guest.${state}`)}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

const STEP_INDEX: Record<Extract<JoinButton, { kind: "busy" }>["step"], number> = {
  preparing: 1,
  opening: 2,
  connecting: 3,
};

function labelIndex(state: JoinButton): number {
  switch (state.kind) {
    case "join":
      return 0;
    case "busy":
      return STEP_INDEX[state.step];
    case "in":
      return 4;
    case "retry":
      return 5;
    case "rejoin":
      return 6;
    case "copy":
      return 7;
  }
}

/** Where a join stands for the invite (the launch says when the game opens and connects). */
export function useJoinButton(invite: WorldInvite): JoinButton {
  // The selected account's launch or game (PLAY-AGAIN: other accounts' games run beside it).
  const { phase, instanceId } = useOwnLaunch();
  return joinButton(invite.join, { phase, instanceId });
}

/** A wording with Home's gold play glyph before it (the event card's "Katıl"). */
function Glyphed({ children }: { children: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <Play size={13} weight="fill" className="shrink-0 text-gold-300" aria-hidden />
      {children}
    </span>
  );
}

/**
 * "Katıl" on a world invite. Its words change in place and keep its width (every wording shares
 * one cell); while the join is under way it waits without dimming (it says where it is) and a
 * thin gold edge fills with the download when there is one (transform only). On Home (`glyph`)
 * the wordings that act carry the event card's gold play glyph; the ones that only say where the
 * join is ("Hazırlanıyor", "Açılıyor") go without it, in the same width (SMALL10).
 */
export function WorldJoinButton({
  invite,
  size = "sm",
  variant = "secondary",
  glyph = false,
  className,
}: {
  invite: WorldInvite;
  size?: "sm" | "md";
  variant?: "primary" | "secondary";
  glyph?: boolean;
  className?: string;
}) {
  const { t } = useTranslation();
  const state = useJoinButton(invite);
  const waiting = state.kind === "busy" || state.kind === "in";
  const percent = state.kind === "busy" ? state.percent : undefined;
  const act = (words: string) => (glyph ? <Glyphed>{words}</Glyphed> : words);
  const labels = [
    act(t("social.actions.join")),
    t("host.join.preparing"),
    t("host.join.opening"),
    t("host.join.connecting"),
    t("host.join.in"),
    act(t("host.join.retry")),
    act(t("host.join.rejoin")),
    act(t("host.join.copyAction")),
  ];
  return (
    <Button
      size={size}
      variant={waiting ? "secondary" : variant}
      aria-disabled={waiting || undefined}
      aria-busy={state.kind === "busy" || undefined}
      className={cn(waiting && "cursor-default active:scale-100", className)}
      onClick={(event) => {
        event.stopPropagation();
        if (!waiting) pressJoin(invite);
      }}
    >
      <span
        className={cn(
          "transition-colors duration-150",
          state.kind === "in" ? "text-jade-400" : state.kind === "busy" ? "text-gold-200" : "",
        )}
      >
        <SwapLabel labels={labels} active={labelIndex(state)} />
      </span>
      <span
        aria-hidden
        className={cn(
          "absolute inset-x-[7px] bottom-[3px] h-[2px] overflow-hidden rounded-full bg-white/8 transition-opacity duration-150",
          percent === undefined ? "opacity-0" : "opacity-100",
        )}
      >
        <span
          className="block h-full origin-left rounded-full bg-gold-400 transition-transform duration-300 ease-out"
          style={{ transform: `scaleX(${(percent ?? 0) / 100})` }}
        />
      </span>
    </Button>
  );
}
