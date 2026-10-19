import { CaretUpDown, Check, Plus, SquaresFour } from "@phosphor-icons/react";
import { Chip, cn, easeOutExpo, PlayButton, type PlayState, Progress, Switch, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { DropdownMenu } from "radix-ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AccountFace } from "@/features/accounts/account-face";
import { launchErrorAction } from "@/features/disk/launch-error";
import { sortInstances, useLibraryView } from "@/features/library/library-view";
import { usePrepareSync } from "@/features/prepare/use-prepare-sync";
import { ResumeButton, useResumeTarget } from "@/features/resume/resume-button";
import { api, type InstanceSummary } from "@/lib/api";
import { copyDetails } from "@/lib/clipboard";
import { formatCount, formatDuration, formatRelative, formatSpeed } from "@/lib/format";
import { detectOs } from "@/lib/platform";
import { ariaKeyShortcuts } from "@/lib/shortcuts";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { type PrepareHint, usePrepare } from "@/stores/prepare";
import { InstanceIcon, prewarmInstanceIcons } from "./instance-icon";

function loaderLine(i: InstanceSummary, loaderName: string) {
  return i.loader === "vanilla" ? `Minecraft ${i.mcVersion}` : `${loaderName} ${i.mcVersion}`;
}

/**
 * The picker's second line: the game and loader version, or, while the instance prepares in the
 * background, a quiet "Hazırlanıyor %42" over a thin grey bar and then "Oynamaya hazır" for a
 * moment. The lines cross-fade in place; nothing around them moves.
 */
function PickerLine({
  selected,
  hint,
}: {
  selected: InstanceSummary | undefined;
  hint: PrepareHint | null;
}) {
  const { t } = useTranslation();
  const reduce = useReducedMotion() ?? false;
  return (
    <span className="grid text-[12px] *:[grid-area:1/1]">
      <AnimatePresence initial={false}>
        {hint?.kind === "preparing" ? (
          <motion.span key="preparing" className="relative block min-w-0" {...swapIn(reduce)}>
            <span className="block truncate text-ash-300 tabular-nums">
              {t("prepare.preparing", { percent: Math.floor(hint.percent) })}
            </span>
            <span
              aria-hidden
              className="absolute inset-x-0 -bottom-[5px] h-[2px] overflow-hidden rounded-full bg-white/8"
            >
              <span
                className="block h-full origin-left rounded-full bg-ash-300/60 transition-transform duration-500 ease-out motion-reduce:transition-none"
                style={{ transform: `scaleX(${Math.min(1, Math.max(0, hint.percent / 100))})` }}
              />
            </span>
          </motion.span>
        ) : hint?.kind === "ready" ? (
          <motion.span
            key="ready"
            className="flex min-w-0 items-center gap-1 text-bone-100"
            {...swapIn(reduce)}
          >
            <Check size={12} weight="bold" className="shrink-0 text-gold-400" />
            <span className="truncate">{t("prepare.ready")}</span>
          </motion.span>
        ) : (
          <motion.span key="version" className="block truncate text-ash-300" {...swapIn(reduce)}>
            {selected ? loaderLine(selected, t(`loaders.${selected.loader}`)) : ""}
          </motion.span>
        )}
      </AnimatePresence>
    </span>
  );
}

function InstancePicker({
  instances,
  selected,
  loading,
  locked,
  hint,
  onSelect,
}: {
  instances: InstanceSummary[];
  selected: InstanceSummary | undefined;
  /** Still loading the list (an empty list then reads "Kurulum seç", not "Yükleniyor"). */
  loading: boolean;
  /** A game is being prepared or played: the bar stays on it until it ends. */
  locked: boolean;
  /** Background preparation of the selected instance, when there is something to show. */
  hint: PrepareHint | null;
  onSelect: (id: string) => void;
}) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  // The library's order (official packs first), so both lists read the same way.
  const sort = useLibraryView((s) => s.sort);
  const ordered = useMemo(() => {
    const sorted = sortInstances(instances, sort);
    return [...sorted.filter((i) => i.managed), ...sorted.filter((i) => !i.managed)];
  }, [instances, sort]);
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button
          type="button"
          aria-label={t("instances.pickerLabel")}
          className="group flex h-[52px] w-[clamp(200px,20vw,264px)] shrink-0 cursor-pointer items-center gap-2.5 rounded-[var(--radius-panel)] px-2 text-left transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/6 active:scale-[0.985] data-[state=open]:bg-white/8"
        >
          {selected ? <InstanceIcon icon={selected.icon} size={36} /> : <span className="size-9" />}
          <span className="min-w-0 flex-1">
            <span className="block truncate font-display text-[15px] font-semibold text-bone-50">
              {selected?.name ?? (loading ? t("common.loading") : t("instances.pickerLabel"))}
            </span>
            <PickerLine selected={selected} hint={hint} />
          </span>
          <CaretUpDown
            size={16}
            className="shrink-0 text-ash-400 transition-colors group-hover:text-bone-100"
          />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          side="top"
          align="start"
          sideOffset={10}
          className="glass-strong pop-motion z-50 w-[300px] rounded-[var(--radius-panel)] p-1 shadow-[0_24px_60px_rgb(34_6_6/0.55)]"
        >
          {/* A long library scrolls here; the two actions below always stay in view. */}
          <div className="max-h-[min(392px,calc(var(--radix-dropdown-menu-content-available-height)-88px))] overflow-y-auto">
            {ordered.map((i) => (
              <DropdownMenu.Item
                key={i.id}
                disabled={locked && selected?.id !== i.id}
                onSelect={() => onSelect(i.id)}
                className="flex cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 outline-none data-[disabled]:cursor-default data-[disabled]:opacity-45 data-[highlighted]:bg-white/8"
              >
                <InstanceIcon icon={i.icon} size={32} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[13px] font-semibold text-bone-50">
                    {i.name}
                  </span>
                  <span className="block truncate text-[12px] text-ash-300">
                    {loaderLine(i, t(`loaders.${i.loader}`))}
                  </span>
                </span>
                {i.managed ? <Chip tone="gold">{t("instances.managed")}</Chip> : null}
                {selected?.id === i.id ? (
                  <Check size={16} className="shrink-0 text-gold-400" />
                ) : (
                  <span aria-hidden className="size-4 shrink-0" />
                )}
              </DropdownMenu.Item>
            ))}
          </div>
          <DropdownMenu.Separator className="my-1 h-px bg-white/8" />
          <DropdownMenu.Item
            onSelect={() => void navigate({ to: "/library" })}
            className="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-[7px] text-[13px] text-ash-300 outline-none data-[highlighted]:bg-white/8 data-[highlighted]:text-bone-50"
          >
            {/* The rail's Library mark: a folder would promise the files on disk ("Klasörü aç"). */}
            <SquaresFour size={16} /> {t("instances.openLibrary")}
          </DropdownMenu.Item>
          <DropdownMenu.Item
            onSelect={() => void navigate({ to: "/library", search: { create: true } })}
            className="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-[7px] text-[13px] text-ash-300 outline-none data-[highlighted]:bg-white/8 data-[highlighted]:text-bone-50"
          >
            <Plus size={16} /> {t("instances.create")}
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

/** In-place swap of the status line and "Devam et" (fades only under reduced motion). */
function swapIn(reduce: boolean) {
  return {
    initial: { opacity: 0, transform: reduce ? "none" : "translateY(4px)" },
    animate: {
      opacity: 1,
      transform: reduce ? "none" : "translateY(0px)",
      transition: { duration: 0.24, ease: easeOutExpo, delay: 0.04 },
    },
    exit: { opacity: 0, transition: { duration: 0.12, ease: easeOutExpo } },
  };
}

/**
 * "Oturum: 12 dk" while the game runs. It renders only when the shown text changes: every second
 * in the first minute ("45 sn"), then once a minute.
 */
function SessionTime({ since }: { since: number }) {
  const { t } = useTranslation();
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const elapsed = now - since;
    const wait = elapsed < 60_000 ? 1000 - (elapsed % 1000) : 60_000 - (elapsed % 60_000);
    const timer = setTimeout(() => setNow(Date.now()), Math.max(50, wait));
    return () => clearTimeout(timer);
  }, [now, since]);
  return <>{t("play.session", { time: formatDuration((now - since) / 1000) })}</>;
}

function StatusArea({ selected }: { selected: InstanceSummary | undefined }) {
  const { t } = useTranslation();
  const reduce = useReducedMotion() ?? false;
  const phase = useLaunch((s) => s.phase);
  const autoJoin = usePrefs((s) => s.autoJoin);
  const setAutoJoin = usePrefs((s) => s.setAutoJoin);
  // "Devam et" only while nothing runs (a click would start nothing otherwise).
  const target = useResumeTarget(selected);
  const resume = phase.kind === "idle" ? target : target === undefined ? undefined : null;

  if (phase.kind === "working") {
    const filesLabel = (done: number) =>
      t("play.files", { done: formatCount(done), total: formatCount(phase.totalFiles) });
    return (
      <div className="min-w-0 flex-1" aria-live="polite">
        <div className="flex items-baseline justify-between gap-4 text-[12px]">
          <span className="truncate font-medium text-bone-100">
            {t(`play.stages.${phase.stage}`)}
          </span>
          {/* Fixed, right-aligned slots: the figures change every tick and come and go between
              stages, so the line never slides sideways. */}
          <span className="flex shrink-0 gap-3 text-ash-300 tabular-nums *:text-right">
            {/* As wide as the count gets in this stage ("402 / 402 dosya"). */}
            <span style={{ minWidth: `${filesLabel(phase.totalFiles).length}ch` }}>
              {phase.totalFiles > 0 ? filesLabel(phase.doneFiles) : null}
            </span>
            <span className="min-w-[10.5ch]">
              {phase.bytesPerSec > 0 ? formatSpeed(phase.bytesPerSec) : null}
            </span>
            <span className="min-w-[11ch]">
              {phase.etaSecs ? t("play.eta", { time: formatDuration(phase.etaSecs) }) : null}
            </span>
          </span>
        </div>
        <Progress value={phase.percent} label={t(`play.stages.${phase.stage}`)} className="mt-2" />
      </div>
    );
  }

  return (
    <div className="flex min-w-0 flex-1 items-center justify-between gap-6">
      {/* "Devam et" and the last-played line share one cell and cross-fade in place. */}
      <div className="grid min-w-0 flex-1 *:[grid-area:1/1]">
        <AnimatePresence initial={false}>
          {selected && resume ? (
            <motion.div key="resume" className="min-w-0" {...swapIn(reduce)}>
              <ResumeButton instance={selected} target={resume} />
            </motion.div>
          ) : selected && resume === null ? (
            <motion.div
              key="status"
              className="min-w-0 text-[12px] text-ash-300"
              {...swapIn(reduce)}
            >
              <span className="block truncate tabular-nums">
                {phase.kind === "running" ? (
                  <SessionTime since={phase.since} />
                ) : selected.lastPlayedAt ? (
                  t("play.lastPlayed", { when: formatRelative(selected.lastPlayedAt) })
                ) : // The session is recorded when the game closes: no "Henüz oynanmadı" while
                // the first one starts.
                phase.kind === "idle" ? (
                  t("play.neverPlayed")
                ) : null}
              </span>
              {selected.playTimeSecs > 0 ? (
                <span className="block truncate">
                  {t("play.playTime", { time: formatDuration(selected.playTimeSecs) })}
                </span>
              ) : null}
            </motion.div>
          ) : null}
        </AnimatePresence>
      </div>
      {selected?.autoJoin ? (
        <Switch
          checked={autoJoin}
          onCheckedChange={setAutoJoin}
          label={t("play.autoJoin")}
          description={selected.autoJoin}
          className="shrink-0"
        />
      ) : null}
    </div>
  );
}

function AccountChip() {
  const { t } = useTranslation();
  const { data: accounts } = useQuery({ queryKey: ["accounts"], queryFn: () => api.accounts() });
  const active = accounts?.find((a) => a.active);
  if (!active) return null;
  return (
    // The title bar shows the account too: below 1180 px the bar keeps its room for play status.
    <div className="hidden shrink-0 items-center gap-2 border-l border-white/8 pl-4 min-[1180px]:flex">
      <AccountFace account={active} size={32} />
      <div className="leading-tight">
        <span className="block max-w-[140px] truncate text-[13px] font-semibold text-bone-50">
          {active.username}
        </span>
        <span className="block text-[11.5px] text-ash-300">
          {active.kind === "microsoft" ? t("accounts.microsoft") : t("accounts.offline")}
        </span>
      </div>
    </div>
  );
}

/** Bottom command bar: what to play, as whom, progress, and the OYNA button. */
export function CommandBar() {
  const { t, i18n } = useTranslation();
  const { data: instances = [], isPending: instancesLoading } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const selectedId = usePrefs((s) => s.selectedInstanceId);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const navigate = useNavigate();
  // Only what the bar shows: the rest of the launch store (crash, notes, screenshots) changes
  // without it.
  const phase = useLaunch((s) => s.phase);
  const launchId = useLaunch((s) => s.instanceId);
  const play = useLaunch((s) => s.play);
  const cancel = useLaunch((s) => s.cancel);
  const reset = useLaunch((s) => s.reset);
  const queryClient = useQueryClient();
  // While a game is prepared or played the bar shows that one: its progress, its session.
  const busy = phase.kind === "working" || phase.kind === "launching" || phase.kind === "running";
  // The picker and the Ctrl+K palette show every instance icon at once on their first opening.
  useEffect(() => prewarmInstanceIcons(), []);

  const selected = useMemo(
    () =>
      (busy ? instances.find((i) => i.id === launchId) : undefined) ??
      instances.find((i) => i.id === selectedId) ??
      instances[0],
    [instances, selectedId, busy, launchId],
  );
  // The instance OYNA plays is the one the launcher prepares in the background; its hint gives
  // way to the launch's own progress.
  usePrepareSync(selected, !instancesLoading);
  const prepareHint = usePrepare((s) => s.hint);
  const hint = !busy && prepareHint?.instanceId === selected?.id ? prepareHint : null;

  // A launch installs the instance and records when and how long it was played: once the game
  // starts and once the launch or the game ends, the bar, the library and "Devam et" read it
  // again instead of keeping "Henüz oynanmadı" until their next refresh.
  const lastKind = useRef(phase.kind);
  useEffect(() => {
    const was = lastKind.current;
    lastKind.current = phase.kind;
    if (was === phase.kind || was === "idle") return;
    if (phase.kind !== "running" && phase.kind !== "idle") return;
    for (const queryKey of [["instances"], ["instance"], ["resume"]]) {
      void queryClient.invalidateQueries({ queryKey });
    }
  }, [phase.kind, queryClient]);

  useEffect(() => {
    if (phase.kind !== "error") return;
    if (launchErrorAction(phase.code) === "makeSpace") {
      // The disk cannot hold the download: nothing was fetched yet (or the half-done file went).
      // Settings → Veri shows what can go and how much room it makes; OYNA tries again after.
      toast.error(phase.message, {
        id: "launch-disk",
        duration: 10_000,
        action: {
          label: t("disk.makeSpace"),
          onClick: () => void navigate({ to: "/settings", search: { section: "data" } }),
        },
      });
    } else {
      toast.error(phase.message, {
        action: {
          label: t("common.copyDetails"),
          onClick: () => void copyDetails(phase.details),
        },
      });
    }
    reset();
  }, [phase, reset, t, navigate]);

  const state: PlayState = (() => {
    switch (phase.kind) {
      case "working":
        return { kind: "working", percent: phase.percent, label: t(`play.stages.${phase.stage}`) };
      case "launching":
        return { kind: "launching", label: t("play.launching") };
      case "running":
        return { kind: "running" };
      default:
        // "Güncelle" only for an installed game whose version or loader changed; a first
        // install is still "Oyna" (it downloads, then plays), as the welcome flow promises.
        return selected?.state === "needsUpdate" ? { kind: "update" } : { kind: "ready" };
    }
  })();

  return (
    <footer className="glass-strong relative z-20 flex h-commandbar shrink-0 items-center gap-4 border-x-0 border-b-0 pr-4 pl-3">
      <InstancePicker
        instances={instances}
        selected={selected}
        loading={instancesLoading}
        locked={busy}
        hint={hint}
        onSelect={selectInstance}
      />
      <AccountChip />
      <div className={cn("flex min-w-0 flex-1 items-center border-l border-white/8 pl-4")}>
        <StatusArea selected={selected} />
      </div>
      <PlayButton
        state={state}
        onPlay={() => {
          if (selected) void play(selected.id);
          // Nothing to play yet: OYNA leads to the one thing that helps.
          else if (!instancesLoading) void navigate({ to: "/library", search: { create: true } });
        }}
        onCancel={() => void cancel()}
        // Ctrl+Enter presses it (app-shell.tsx), only when a press starts a game.
        keyShortcuts={ariaKeyShortcuts("play", detectOs())}
        labels={{
          play: t("play.play"),
          update: t("play.update"),
          cancel: t("play.cancel"),
          running: t("play.running"),
          percentFirst: i18n.language !== "en",
        }}
      />
    </footer>
  );
}
