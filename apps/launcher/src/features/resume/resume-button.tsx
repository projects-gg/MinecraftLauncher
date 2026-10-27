// "Devam et": the world or server the selected instance played last, one click away (1.20+ quick
// play). A quiet button beside OYNA; OYNA stays the loud one. While games run, a world open in one
// of them says so on the button before a press (PLAY-AGAIN, second look): dimmed, no play mark,
// whose game has it.
import { Play } from "@phosphor-icons/react";
import { ContentIcon, cn, easeOutExpo, Tooltip } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type InstanceSummary, type ResumeTarget } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { sameAsAutoJoin } from "./model";

/**
 * Where "Devam et" goes for `instance`: `undefined` while it is being read, `null` when there is
 * nothing to show (before 1.20, never played, or OYNA joins the same server anyway).
 */
export function useResumeTarget(
  instance: InstanceSummary | undefined,
): ResumeTarget | null | undefined {
  const queryClient = useQueryClient();
  const phase = useLaunch((s) => s.phase.kind);
  const autoJoinOn = usePrefs((s) => s.autoJoin);
  const { data, isPending } = useQuery({
    queryKey: ["resume", instance?.id],
    queryFn: () => api.resumeTarget(instance?.id ?? ""),
    enabled: Boolean(instance),
    staleTime: 60_000,
    retry: false,
  });
  // A session that just ended may have moved the target.
  const previous = useRef(phase);
  useEffect(() => {
    if (previous.current === "running" && phase === "idle") {
      void queryClient.invalidateQueries({ queryKey: ["resume"] });
    }
    previous.current = phase;
  }, [phase, queryClient]);

  if (!instance) return null;
  if (isPending) return undefined;
  if (!data || sameAsAutoJoin(data, instance.autoJoin, autoJoinOn)) return null;
  return data;
}

/**
 * Whether the world "Devam et" goes to is open in a game now (PLAY-AGAIN), read while games run:
 * when one starts or ends, and when the pointer or the focus comes to "Devam et" (never on a
 * timer). A world open in a game cannot open in another (its lock).
 */
function useWorldBusy(instance: InstanceSummary, target: ResumeTarget) {
  const games = useLaunch((s) => s.games.length);
  const world = target.quickPlay.kind === "singleplayer" ? target.quickPlay.world : null;
  const enabled = world !== null && games > 0;
  const { data, refetch } = useQuery({
    queryKey: ["world-open", instance.id, world, games],
    queryFn: () => api.gameWorldOpen(instance.id, world ?? ""),
    enabled,
    staleTime: 0,
    retry: false,
  });
  return {
    open: enabled && data?.open === true,
    account: data?.account ?? null,
    recheck: () => {
      if (enabled) void refetch();
    },
  };
}

export function ResumeButton({
  instance,
  target,
}: {
  instance: InstanceSummary;
  target: ResumeTarget;
}) {
  const { t } = useTranslation();
  const play = useLaunch((s) => s.play);
  const [broken, setBroken] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const busy = useWorldBusy(instance, target);
  // Where the world is, in the label's place ("Başka oyunda"; third look: "Kullanımda" told a
  // player nothing); whose game has it under the pointer when that is one account.
  const reason = busy.open ? t("playAgain.worldBusy") : null;
  const whose =
    busy.open && busy.account ? t("playAgain.worldBusyName", { name: busy.account }) : undefined;
  const button = (
    <button
      type="button"
      aria-disabled={reason ? true : undefined}
      onPointerEnter={busy.recheck}
      onFocus={busy.recheck}
      onClick={() => {
        // Open in a game: nothing starts (the press only reads it again).
        if (reason) {
          busy.recheck();
          return;
        }
        void play(instance.id, { quickPlay: target.quickPlay });
      }}
      aria-label={
        reason
          ? t("playAgain.worldBusyAria", { world: target.name, reason: whose ?? reason })
          : t("resume.aria", { name: target.name })
      }
      // The cap counts the hover plate's negative margin: `max-w-full` clamped the button 8 px
      // under its own width, so a name that fits ("Ejderha Adası") lost its last letters at 1280.
      className={cn(
        "group -my-1 -ml-2 flex w-fit max-w-[calc(100%+0.5rem)] min-w-0 items-center gap-3 rounded-[var(--radius-panel)] py-1 pr-3 pl-2 text-left transition-[background-color,transform,scale] duration-150 ease-out focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none",
        reason ? "cursor-default" : "cursor-pointer hover:bg-white/6 active:scale-[0.98]",
      )}
    >
      <span
        className={cn(
          "relative flex size-9 shrink-0 items-center justify-center overflow-hidden rounded-[var(--radius-head)] bg-obsidian-800 ring-1 ring-white/10 transition-opacity duration-200 ease-out",
          reason && "opacity-35",
        )}
      >
        {target.iconUrl && !broken ? (
          <img
            src={target.iconUrl}
            alt=""
            draggable={false}
            onLoad={() => setLoaded(true)}
            onError={() => setBroken(true)}
            className={cn(
              "size-full object-cover transition-opacity duration-200 ease-out",
              loaded ? "opacity-100" : "opacity-0",
              // Server icons are pixel art; world icons are small screenshots.
              target.kind === "server" && "[image-rendering:pixelated]",
            )}
          />
        ) : (
          <ContentIcon kind={target.kind} size={16} />
        )}
        {reason ? null : (
          <span
            aria-hidden
            className="absolute inset-0 flex items-center justify-center bg-obsidian-950/60 opacity-0 transition-opacity duration-150 ease-out group-hover:opacity-100 group-focus-visible:opacity-100"
          >
            <Play size={14} weight="fill" className="text-bone-50" />
          </span>
        )}
      </span>
      <span className="relative min-w-0 leading-tight">
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span
            key={reason ?? "resume"}
            // While the world is open elsewhere the reason is the news and the title steps back.
            className={cn("block truncate text-[12px]", reason ? "text-ash-300" : "text-ash-400")}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.18, ease: easeOutExpo } }}
            exit={{ opacity: 0, transition: { duration: 0.1, ease: easeOutExpo } }}
          >
            {reason ?? t("resume.label")}
          </motion.span>
        </AnimatePresence>
        <span
          className={cn(
            "block truncate text-[13.5px] font-semibold transition-colors duration-200 ease-out",
            reason ? "text-ash-400" : "text-bone-50",
          )}
        >
          {target.name}
        </span>
      </span>
    </button>
  );
  // Whose game has the world, under the pointer: the app's own tooltip (a native one drew the
  // system's grey box), only while the world is open in a game.
  return (
    <Tooltip content={whose} disabled={!whose}>
      {button}
    </Tooltip>
  );
}
