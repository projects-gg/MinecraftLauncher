// Settings → Sorun yardımcısı (HELP1): "the game won't start / I can't connect" in one place. A calm
// verdict on top, then the rows (connection; the game Play starts: disk, Java, memory, mods, last
// crash), each with its state, one line and, where the launcher can do it, one tap to fix; at the
// end the support summary to copy and the support channel. Every check runs once when the section
// opens (or a launch fails), never in the background.
//
// Motion (docs/design.md §5): rows keep their place while they are checked; a row's words cross-
// fade with its skeleton in one grid cell and its mark grows in from 90 % (fades only under reduced
// motion), rows that come or go fade in place and the ones below glide (not in Performance mode),
// the progress line moves with `transform`. Nothing moves once the run is done.
import {
  ArrowClockwise,
  CheckCircle,
  Copy,
  Info,
  Lifebuoy,
  MinusCircle,
  Warning,
  WarningCircle,
  XCircle,
} from "@phosphor-icons/react";
import {
  Button,
  cn,
  easeOutExpo,
  Select,
  SignalBars,
  type SignalState,
  Skeleton,
  toast,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { CommunityMark } from "@/features/home/community-links";
import { sortInstances, useLibraryView } from "@/features/library/library-view";
import { enter, glide, leave, pop, useMotionPrefs } from "@/features/modcenter/motion";
import { useModFlow } from "@/features/modhealth/flow";
import { usePlayInstance } from "@/features/settings/graphics-section";
import { api, type HelpLevel, type InstanceSummary } from "@/lib/api";
import { copyDetails, copyText } from "@/lib/clipboard";
import { openLink } from "@/lib/links";
import { InstanceIcon } from "@/shell/instance-icon";
import { usePrefs } from "@/stores/prefs";
import { runFix } from "./fixes";
import {
  fixLabel,
  type Group,
  layout,
  noticeRow,
  type Row,
  type RowCheck,
  reasonOf,
  shownFix,
  summaryText,
  titleOf,
  type Verdict,
  verdict,
  verdictLine,
} from "./model";
import { type LaunchFailure, recentFailure, useHelp } from "./store";

type Shown = "checking" | HelpLevel;

/** Skeleton widths, so the rows being checked do not read as one block. */
const SKELETON_WIDTHS = ["w-[46%]", "w-[58%]", "w-[38%]", "w-[52%]", "w-[42%]", "w-[60%]"];

/** Text replaced in place: the old words leave fast, the new ones come a beat later, one cell. */
function Swap({ text, className }: { text: string; className?: string }) {
  return (
    <span className={cn("grid", className)}>
      <AnimatePresence initial={false}>
        <motion.span
          key={text}
          className="[grid-area:1/1]"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: { duration: 0.18, ease: easeOutExpo, delay: 0.05 } }}
          exit={{ opacity: 0, transition: { duration: 0.1, ease: easeOutExpo } }}
        >
          {text}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

/**
 * A row's state: a quiet ring with a softly breathing gold core while it is checked (a status
 * light, not a spinner), then a check, a warning sign, a cross, an "i" or a dash. Each state has its
 * own shape, so colour is never the only sign; the box never changes size.
 */
function Glyph({ state }: { state: Shown }) {
  const prefs = useMotionPrefs();
  return (
    <span aria-hidden className="relative grid size-5 shrink-0 place-items-center">
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span
          key={state}
          {...pop(prefs.reduce, state === "checking" ? 0.16 : 0.24)}
          className="absolute inset-0 grid place-items-center"
        >
          {state === "checking" ? (
            <span className="grid size-[15px] place-items-center rounded-full ring-[1.5px] ring-gold-400/55">
              <span className="size-[7px] rounded-full bg-gold-300 animate-pulse motion-reduce:animate-none" />
            </span>
          ) : state === "ok" ? (
            <CheckCircle size={19} weight="fill" className="text-jade-400" />
          ) : state === "warning" ? (
            <Warning size={19} weight="fill" className="text-gold-400" />
          ) : state === "problem" ? (
            <XCircle size={19} weight="fill" className="text-flare-400" />
          ) : state === "note" ? (
            <Info size={19} weight="fill" className="text-ash-300" />
          ) : (
            <MinusCircle size={18} className="text-ash-400" />
          )}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

/** The verdict's mark: the tool's own lifebuoy while checking, then the run's worst state. */
function VerdictMark({ kind }: { kind: Verdict["kind"] }) {
  const prefs = useMotionPrefs();
  const icon =
    kind === "checking" ? (
      <Lifebuoy size={22} weight="duotone" className="text-gold-300" />
    ) : kind === "fine" ? (
      <CheckCircle size={22} weight="duotone" className="text-jade-400" />
    ) : kind === "problems" ? (
      <WarningCircle size={22} weight="duotone" className="text-flare-400" />
    ) : (
      <Warning size={22} weight="duotone" className="text-gold-300" />
    );
  return (
    <span
      aria-hidden
      className="chamfer relative grid size-10 shrink-0 place-items-center bg-obsidian-800 [--cut:7px]"
    >
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span
          key={kind}
          {...pop(prefs.reduce, 0.26)}
          className="absolute inset-0 grid place-items-center"
        >
          {icon}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

function VerdictCard({
  shown,
  run,
  progress,
  running,
  onRecheck,
}: {
  shown: Verdict;
  run: number;
  progress: number;
  running: boolean;
  onRecheck: () => void;
}) {
  const { t, i18n } = useTranslation();
  const headline =
    shown.kind === "checking"
      ? t("help.verdict.checking")
      : shown.kind === "fine"
        ? t("help.verdict.fine")
        : t(`help.verdict.${shown.kind}`, { count: shown.count });
  const line =
    shown.kind === "checking"
      ? t("help.verdict.checkingLine")
      : shown.kind === "fine"
        ? t("help.verdict.fineLine")
        : verdictLine(t, shown.checks, i18n.language);
  // Heard once a run ends (the rows are not announced one by one).
  const announced = shown.kind === "checking" ? "" : `${t("help.verdict.done")} ${headline}.`;
  return (
    <div className="relative overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950/45 px-4 py-3 ring-1 ring-white/7">
      <div className="flex items-center gap-3.5">
        <VerdictMark kind={shown.kind} />
        <div className="min-w-0 flex-1">
          <Swap text={headline} className="font-display text-section font-semibold text-bone-50" />
          <Swap text={line} className="mt-0.5 text-[13px] leading-[18px] text-ash-300" />
        </div>
        <Button
          size="sm"
          variant="secondary"
          icon={<ArrowClockwise size={15} />}
          disabled={running}
          onClick={onRecheck}
        >
          {t("help.recheck")}
        </Button>
      </div>
      {/* How many rows are in: a hairline along the bottom edge (a new run starts a new one, so
          it never runs backwards), gone once the run is done. */}
      <div
        key={run}
        aria-hidden
        className={cn(
          "absolute inset-x-0 bottom-0 h-[2px] bg-white/[0.04] transition-opacity",
          running ? "opacity-100 duration-150" : "opacity-0 delay-300 duration-500",
        )}
      >
        <div
          className="h-full w-full origin-left bg-[linear-gradient(90deg,var(--color-gold-600),var(--color-gold-300))] transition-transform duration-500 ease-[cubic-bezier(0.16,1,0.3,1)]"
          style={{ transform: `scaleX(${Math.max(0.04, progress)})` }}
        />
      </div>
      <p role="status" className="sr-only">
        {announced}
      </p>
    </div>
  );
}

/** The server's bars, as everywhere the launcher shows reachability (DOT1). */
function serverSignal(row: Row | undefined): SignalState {
  if (!row) return "pending";
  if (row.code === "serverOk" || row.code === "serverSlow") return "online";
  return row.level === "skipped" ? "idle" : "offline";
}

function HelpRow({
  check,
  row,
  rows,
  index,
  lead,
  busy,
  onFix,
}: {
  check: RowCheck;
  row: Row | undefined;
  /** Every row (one button per fix, `shownFix`). */
  rows: ReadonlyMap<RowCheck, Row>;
  index: number;
  lead: boolean;
  busy: boolean;
  onFix: (row: Row) => void;
}) {
  const { t, i18n } = useTranslation();
  const prefs = useMotionPrefs();
  const checking = !row;
  const state: Shown = row ? row.level : "checking";
  const reason = row ? reasonOf(t, row, i18n.language) : "";
  // While the row is checked again its last words and fix stay under the skeleton, so the row
  // keeps its height and nothing beside it moves.
  const last = useRef<{ reason: string; row: Row } | null>(null);
  if (row) last.current = { reason, row };
  const kept = { reason: row ? reason : (last.current?.reason ?? "") };
  const shownRow = row ?? last.current?.row;
  const fix = shownRow ? shownFix(shownRow, rows) : undefined;
  const title = titleOf(t, check);
  const tone =
    !row || row.level === "ok"
      ? "text-ash-300"
      : row.level === "skipped"
        ? "text-ash-400"
        : "text-bone-100";
  return (
    <motion.li
      layout={prefs.layout ? "position" : false}
      transition={{ layout: glide }}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: enter(0.2) }}
      exit={{ opacity: 0, transition: leave(0.16) }}
      className="flex min-h-[40px] items-center gap-3 border-white/6 border-t px-3.5 py-1.5 first:border-t-0"
    >
      <Glyph state={state} />
      <span className="w-[150px] shrink-0 text-[13.5px] font-medium text-bone-50">{title}</span>
      <span className="sr-only">{t(`help.states.${state}`)}:</span>
      <span className="flex min-w-0 flex-1 items-center gap-2 text-[13px] leading-[18px]">
        {check === "server" ? (
          <SignalBars state={serverSignal(row)} latencyMs={row?.facts.latencyMs} />
        ) : null}
        <span className="grid min-w-0 flex-1 items-center">
          <Skeleton
            className={cn(
              "[grid-area:1/1] h-3 transition-opacity",
              SKELETON_WIDTHS[index % SKELETON_WIDTHS.length],
              checking ? "opacity-100 duration-150" : "opacity-0 duration-100",
            )}
          />
          <span
            aria-hidden={checking || undefined}
            className={cn(
              "[grid-area:1/1] min-w-0 text-pretty transition-[opacity,color]",
              checking ? "opacity-0 duration-100" : "opacity-100 delay-[60ms] duration-150",
              tone,
            )}
          >
            {kept.reason}
          </span>
        </span>
      </span>
      <span className="flex shrink-0 justify-end">
        <AnimatePresence initial={false}>
          {fix ? (
            <motion.span
              key={fix}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: enter(0.18) }}
              exit={{ opacity: 0, transition: leave(0.14) }}
            >
              <Button
                size="sm"
                variant={lead ? "primary" : "secondary"}
                disabled={checking || busy}
                aria-label={`${fixLabel(t, fix)}: ${title}`}
                onClick={() => shownRow && onFix(shownRow)}
              >
                {fixLabel(t, fix)}
              </Button>
            </motion.span>
          ) : null}
        </AnimatePresence>
      </span>
    </motion.li>
  );
}

/** The game group's subject: the instance Play starts, chosen as in the command bar's picker. */
function InstancePicker({
  instances,
  instance,
  locked,
}: {
  instances: InstanceSummary[];
  instance: InstanceSummary;
  locked: boolean;
}) {
  const { t } = useTranslation();
  const selectInstance = usePrefs((s) => s.selectInstance);
  const sort = useLibraryView((s) => s.sort);
  const ordered = useMemo(() => {
    const sorted = sortInstances(instances, sort);
    return [...sorted.filter((i) => i.managed), ...sorted.filter((i) => !i.managed)];
  }, [instances, sort]);
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <InstanceIcon icon={instance.icon} size={24} />
      {ordered.length > 1 ? (
        <div className="w-52">
          <Select<string>
            value={instance.id}
            onValueChange={selectInstance}
            label={t("fps.instance")}
            disabled={locked}
            options={ordered.map((i) => ({ value: i.id, label: i.name, hint: i.mcVersion }))}
          />
        </div>
      ) : (
        <span className="truncate text-[13px] font-medium text-bone-100">{instance.name}</span>
      )}
    </div>
  );
}

function GroupBlock({
  group,
  checks,
  rows,
  running,
  lead,
  busy,
  aside,
  onFix,
}: {
  group: Group;
  checks: RowCheck[];
  rows: ReadonlyMap<RowCheck, Row>;
  running: boolean;
  lead: RowCheck | undefined;
  busy: boolean;
  aside?: ReactNode;
  onFix: (row: Row) => void;
}) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  const id = useId();
  return (
    <motion.section
      layout={prefs.layout ? "position" : false}
      transition={{ layout: glide }}
      aria-labelledby={id}
    >
      <div className="mb-1.5 flex min-h-[24px] items-center justify-between gap-4">
        <h3 id={id} className="text-[12.5px] font-medium text-ash-400">
          {t(`help.groups.${group}`)}
        </h3>
        {aside}
      </div>
      <ul
        aria-labelledby={id}
        aria-busy={running || undefined}
        className="overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950/45 ring-1 ring-white/7"
      >
        <AnimatePresence initial={false}>
          {checks.map((check, index) => (
            <HelpRow
              key={check}
              check={check}
              row={rows.get(check)}
              rows={rows}
              index={index + (group === "game" ? 3 : 0)}
              lead={lead === check}
              busy={busy}
              onFix={onFix}
            />
          ))}
        </AnimatePresence>
      </ul>
    </motion.section>
  );
}

/** The launch that failed before the panel opened, with its details for support. */
function LaunchNote({ failure }: { failure: LaunchFailure }) {
  const { t } = useTranslation();
  return (
    <motion.div
      layout="position"
      transition={{ layout: glide }}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: enter(0.2) }}
      exit={{ opacity: 0, transition: leave(0.16) }}
      className="flex items-center gap-3 rounded-[var(--radius-panel)] bg-flare-400/[0.06] py-2 pr-2 pl-3.5 ring-1 ring-flare-400/15"
    >
      <WarningCircle size={19} weight="fill" className="shrink-0 text-flare-400" aria-hidden />
      <p className="min-w-0 flex-1 text-[13px] leading-[18px] text-bone-100">
        <span className="font-medium text-bone-50">{t("help.lastLaunch")}: </span>
        {failure.message}
      </p>
      <Button
        size="sm"
        variant="ghost"
        icon={<Copy size={14} />}
        onClick={() => void copyDetails(failure.details)}
      >
        {t("common.copyDetails")}
      </Button>
    </motion.div>
  );
}

/** Before the run's plan comes (a few milliseconds), the rows every run has. */
function defaultPlan(instance: InstanceSummary | undefined): RowCheck[] {
  const base: RowCheck[] = ["internet", "server", "content", "disk"];
  if (!instance) return base;
  return [...base, "java", ...(instance.loader !== "vanilla" ? (["mods"] as const) : []), "crash"];
}

/**
 * Settings → Sorun yardımcısı. Checks the instance Play starts (the command bar's rule, as
 * "Grafik ve FPS" does); a pick of another instance checks again.
 */
export function HelpSection() {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const { data: instances = [], isPending } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const { instance, busy: gameBusy } = usePlayInstance(instances);
  const instanceId = instance?.id ?? null;
  // Studio's links and the announcement come with the home content the launcher already has.
  const { data: home } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });

  const run = useHelp((s) => s.run);
  const status = useHelp((s) => s.status);
  const planned = useHelp((s) => s.planned);
  const findings = useHelp((s) => s.findings);
  const report = useHelp((s) => s.report);
  const startedAt = useHelp((s) => s.startedAt);
  const failure = recentFailure(useHelp((s) => s.failure));
  const running = status !== "done";

  useEffect(() => {
    if (!isPending) useHelp.getState().ensure(instanceId);
  }, [isPending, instanceId]);

  // The repair or crash dialog opened from a row: what it changed is checked once it closes.
  const flowOpen = useModFlow((s) => s.step !== null);
  const watched = useRef(false);
  useEffect(() => {
    if (flowOpen) {
      watched.current = true;
    } else if (watched.current) {
      watched.current = false;
      useHelp.getState().start(instanceId);
    }
  }, [flowOpen, instanceId]);

  const notice = useMemo(() => noticeRow(home?.maintenance, home?.status), [home]);
  const rows = useMemo(() => {
    const map = new Map<RowCheck, Row>();
    for (const finding of Object.values(findings)) if (finding) map.set(finding.check, finding);
    if (notice) map.set("notice", notice);
    return map;
  }, [findings, notice]);
  const plan: RowCheck[] = planned.length > 0 ? planned : defaultPlan(instance);
  const groups = layout(plan, new Set(rows.keys()));
  const shownRows = groups.flatMap((g) => g.checks.flatMap((check) => rows.get(check) ?? []));
  const shown = verdict(shownRows, running);
  const settled = plan.filter((check) => rows.has(check)).length;
  const progress = plan.length > 0 ? settled / plan.length : 0;
  // The first problem's fix turns gold once the run is done (so the gold never hops from row to
  // row while rows settle); every other fix stays quiet. Fixes work as soon as their row settles.
  const lead = running
    ? undefined
    : shownRows.find((row) => row.level === "problem" && shownFix(row, rows))?.check;

  const [fixing, setFixing] = useState(false);
  const fix = async (row: Row) => {
    if (!row.fix || fixing) return;
    setFixing(true);
    try {
      await runFix(row.fix, { instanceId, url: row.facts.url, queryClient });
    } finally {
      setFixing(false);
    }
  };

  const copy = () => {
    const text = summaryText({
      t,
      language: i18n.language,
      system: report?.system,
      instance: report?.instance,
      rows: shownRows,
      launchCode: failure?.code || undefined,
      now: new Date(startedAt || Date.now()),
    });
    void copyText(text).then(
      () => toast.success(t("help.footer.copied")),
      () => toast.error(t("common.copyFailed")),
    );
  };
  const links = home?.links;
  const supportUrl = links?.support ?? links?.discord ?? "https://projects.gg/discord";
  const discord =
    /(^|\.)discord(app)?\.(gg|com)$/i.test(hostOf(supportUrl)) ||
    supportUrl === "https://projects.gg/discord";
  const prefs = useMotionPrefs();
  const { end, below: stuck } = useContentBelow();

  return (
    <div className="mt-3 space-y-4 pb-1">
      <VerdictCard
        shown={shown}
        run={run}
        progress={running ? progress : 1}
        running={running}
        onRecheck={() => useHelp.getState().start(instanceId)}
      />
      <AnimatePresence initial={false}>
        {failure ? <LaunchNote key="failure" failure={failure} /> : null}
      </AnimatePresence>
      {groups.map(({ group, checks }) => (
        <GroupBlock
          key={group}
          group={group}
          checks={checks}
          rows={rows}
          running={running}
          lead={lead}
          busy={fixing}
          onFix={(row) => void fix(row)}
          aside={
            group === "game" && instance ? (
              <InstancePicker instances={instances} instance={instance} locked={gameBusy} />
            ) : undefined
          }
        />
      ))}
      {/* The way to support stays in view: on a short window the footer holds to the panel's
          bottom edge and the rows pass under a soft fade (no blur, so nothing shows through). */}
      <motion.div
        layout={!stuck && prefs.layout ? "position" : false}
        transition={{ layout: glide }}
        className="sticky bottom-0 z-10 -mx-6 -mb-4 px-6 pt-1.5 pb-4"
      >
        <div
          aria-hidden
          className={cn(
            "pointer-events-none absolute inset-x-0 -top-6 bottom-0 bg-[linear-gradient(to_bottom,transparent,rgb(18_16_22/0.97)_21px)] transition-opacity duration-200",
            stuck ? "opacity-100" : "opacity-0",
          )}
        />
        <div className="relative flex flex-wrap items-center gap-x-4 gap-y-2.5">
          <span className="text-[13px] text-ash-300">{t("help.footer.stuck")}</span>
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              variant="secondary"
              icon={<Copy size={15} />}
              disabled={running}
              onClick={copy}
            >
              {t("help.footer.copy")}
            </Button>
            <Button
              size="sm"
              variant="secondary"
              icon={discord ? <CommunityMark platform="discord" /> : <Lifebuoy size={15} />}
              onClick={() => openLink(supportUrl)}
            >
              {discord ? t("help.footer.discord") : t("help.footer.support")}
            </Button>
          </div>
        </div>
      </motion.div>
      <div ref={end} aria-hidden className="!mt-0 h-px" />
    </div>
  );
}

/**
 * Whether the content goes on below the panel's visible part: a pixel at its very end is out of
 * sight. Observed, never polled.
 */
function useContentBelow() {
  const end = useRef<HTMLDivElement>(null);
  const [below, setBelow] = useState(false);
  useEffect(() => {
    const node = end.current;
    if (!node || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(([entry]) =>
      setBelow(!(entry?.isIntersecting ?? true)),
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return { end, below };
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}
