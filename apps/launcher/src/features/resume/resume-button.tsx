// "Devam et": the world or server the selected instance played last, one click away (1.20+ quick
// play). A quiet button beside OYNA; OYNA stays the loud one.
import { Play } from "@phosphor-icons/react";
import { cn } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type InstanceSummary, type ResumeTarget } from "@/lib/api";
import { ContentIcon } from "@/shell/content-icon";
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
  return (
    <button
      type="button"
      onClick={() => void play(instance.id, { quickPlay: target.quickPlay })}
      aria-label={t("resume.aria", { name: target.name })}
      className="group -my-1 -ml-2 flex w-fit max-w-full min-w-0 cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] py-1 pr-3 pl-2 text-left transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none active:scale-[0.98]"
    >
      <span className="relative flex size-9 shrink-0 items-center justify-center overflow-hidden rounded-[var(--radius-head)] bg-obsidian-800 ring-1 ring-white/10">
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
        <span
          aria-hidden
          className="absolute inset-0 flex items-center justify-center bg-obsidian-950/60 opacity-0 transition-opacity duration-150 ease-out group-hover:opacity-100 group-focus-visible:opacity-100"
        >
          <Play size={14} weight="fill" className="text-bone-50" />
        </span>
      </span>
      <span className="min-w-0 leading-tight">
        <span className="block text-[12px] text-ash-400">{t("resume.label")}</span>
        <span className="block truncate text-[13.5px] font-semibold text-bone-50">
          {target.name}
        </span>
      </span>
    </button>
  );
}
