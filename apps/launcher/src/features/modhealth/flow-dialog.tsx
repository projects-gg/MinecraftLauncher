import {
  ArrowCounterClockwise,
  ArrowsClockwise,
  CaretDown,
  CheckCircle,
  Copy,
  DownloadSimple,
  FileText,
  FolderSimple,
  Lifebuoy,
  Package,
  SlidersHorizontal,
  Trash,
  Warning,
  WarningCircle,
  WifiSlash,
} from "@phosphor-icons/react";
import { Button, cn, Dialog, IconButton, Progress, Skeleton, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { showError } from "@/features/content/jobs";
import { openHelp } from "@/features/help/store";
import {
  type Direction,
  enter,
  fade,
  leave,
  rise,
  stagger,
  stepVariants,
  swap,
  useMotionPrefs,
} from "@/features/modcenter/motion";
import { Percent } from "@/features/modcenter/parts";
import { setName, setToUpdate } from "@/features/modsets/model";
import {
  api,
  type HealthIssue,
  type ReinstallPhase,
  type RepairScope,
  type RepairStep,
} from "@/lib/api";
import { copyDetails as copySupportText } from "@/lib/clipboard";
import { type CrashInfo, useLaunch } from "@/stores/launch";
import { Checklist, type ChecklistRow } from "./checklist";
import { type FlowStep, useModFlow } from "./flow";
import { IssueList } from "./issue-list";
import { useKeysStep } from "./key-clash";
import {
  crashHint,
  crashTitle,
  currentStep,
  formatMemory,
  joinNames,
  stepNote,
  stepText,
  suspectText,
} from "./texts";
import { reinstallPercent, repairPercent, useReinstallJob, useRepairJob } from "./use-repair";

/** A fact of a summary: an icon and a sentence, cascading in. */
function Fact({
  icon,
  index,
  children,
  action,
}: {
  icon: ReactNode;
  index: number;
  children: ReactNode;
  action?: ReactNode;
}) {
  const prefs = useMotionPrefs();
  const shift = (px: number) => (prefs.reduce ? "none" : `translateY(${px}px)`);
  return (
    <motion.li
      initial={{ opacity: 0, transform: shift(6) }}
      animate={{
        opacity: 1,
        transform: shift(0),
        transition: { ...enter(0.26), delay: stagger(index, 0.045) },
      }}
      className="flex items-start gap-3"
    >
      <span className="mt-px shrink-0 text-gold-300">{icon}</span>
      <div className="min-w-0 flex-1 text-[13.5px] leading-relaxed text-bone-100">{children}</div>
      {action ? <span className="-my-1 shrink-0">{action}</span> : null}
    </motion.li>
  );
}

/** Shaped like the facts of the reinstall summary. */
function FactsSkeleton() {
  return (
    <div aria-busy className="space-y-4">
      {["w-3/5", "w-2/5", "w-1/2", "w-1/3"].map((width) => (
        <div key={width} className="flex gap-3">
          <Skeleton className="size-5 shrink-0 rounded-full" />
          <Skeleton className={`mt-0.5 h-3.5 ${width}`} />
        </div>
      ))}
    </div>
  );
}

/** A progress line for a footer: what happens now, the overall percentage and the bar. */
function RunLine({ label, percent, bar }: { label: string; percent: number; bar: string }) {
  return (
    <div className="min-w-0 flex-1">
      <p
        className="mb-1.5 flex items-baseline justify-between gap-3 text-[13px] text-bone-100"
        aria-live="polite"
      >
        <span className="relative min-w-0 flex-1 truncate">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span key={label} {...swap(0.16)} className="block truncate">
              {label}
            </motion.span>
          </AnimatePresence>
        </span>
        <Percent value={percent} />
      </p>
      <Progress value={percent} label={bar} />
    </div>
  );
}

const shorten = (name: string, max = 26) =>
  name.length > max ? `${name.slice(0, max - 1)}…` : name;

const settingsKinds = new Set(["invalidJvmArgument", "javaTooNew", "javaTooOld"]);

/**
 * How long "Onar" may wait, pressed in, for the repair's plan before its step opens anyway (with
 * rows that hold the checklist's place). The plan usually comes within a tenth of a second.
 */
const REPAIR_HOLD_MS = 250;

/**
 * Mod health in one dialog: the crash summary, mods that stop a launch, "Onar" with its steps
 * ticking through, and "Yeniden yükle" (confirm, progress, summary). Moving between them keeps
 * the dialog open: the panel glides to its new size and steps slide in from the side they come
 * from. While files change it cannot be dismissed; stopping is offered while it is safe.
 */
export function ModFlowDialog() {
  const { t, i18n } = useTranslation();
  const prefs = useMotionPrefs();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const flow = useModFlow();
  const liveCrash = useLaunch((s) => s.crash);
  const play = useLaunch((s) => s.play);
  const dismissCrash = useLaunch((s) => s.dismissCrash);
  const repair = useRepairJob(flow.instanceId);
  const reinstall = useReinstallJob(flow.instanceId);
  const [direction, setDirection] = useState<Direction>(1);
  const [warningsOpen, setWarningsOpen] = useState(false);
  const [turnedOff, setTurnedOff] = useState<string | null>(null);
  const [busy, setBusy] = useState<"turnOff" | "memory" | "args" | null>(null);
  // "Onar" pressed, waiting for the repair's plan (`startRepair`).
  const [holding, setHolding] = useState<{ scope: RepairScope; until: number } | null>(null);

  // What the dialog shows stays in place while it closes.
  const lastStep = useRef<FlowStep | null>(null);
  const lastCrash = useRef<CrashInfo | null>(null);
  if (flow.step) lastStep.current = flow.step;
  if (liveCrash) lastCrash.current = liveCrash;
  const step = flow.step ?? lastStep.current;
  const crash = liveCrash ?? lastCrash.current;
  const instanceId = flow.instanceId;
  const open = flow.step !== null;
  const playAfter = flow.origin === "crash" || flow.origin === "blocked";

  // A new flow starts clean.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs once per flow opened
  useEffect(() => {
    repair.reset();
    reinstall.reset();
    setTurnedOff(null);
    setWarningsOpen(false);
    setBusy(null);
    setHolding(null);
    setDirection(1);
  }, [flow.session]);

  // Entering a step starts its work; going back to a repair shows its result again.
  const ran = useRef<string | null>(null);
  const stepId =
    flow.step?.kind === "repair" ? `repair:${JSON.stringify(flow.step.scope)}` : flow.step?.kind;
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs once per step entered
  useEffect(() => {
    const { step: current, session } = useModFlow.getState();
    if (current?.kind === "repair") {
      const key = `${session}:${JSON.stringify(current.scope)}`;
      if (ran.current !== key) {
        ran.current = key;
        void repair.start(current.scope);
      }
    }
    if (current?.kind === "reinstall" && !reinstall.running && !reinstall.result) {
      reinstall.reset();
    }
  }, [stepId, flow.session]);

  const preview = useQuery({
    queryKey: ["reinstall-preview", instanceId],
    queryFn: () => api.modHealth.reinstallPreview(instanceId ?? ""),
    enabled: open && step?.kind === "reinstall" && Boolean(instanceId),
    staleTime: 0,
    retry: false,
  });

  // A Java crash of an instance on a custom Java: "Kur ve oyna" switches it to a Java that fits
  // (the launch's Java check does it, J1).
  const javaCrash =
    step?.kind === "crash" && (crash?.kind === "javaTooOld" || crash?.kind === "javaTooNew");
  const crashedInstance = useQuery({
    queryKey: ["instance", instanceId],
    queryFn: () => api.instance(instanceId ?? ""),
    enabled: open && javaCrash && Boolean(instanceId),
  });
  const customJava = javaCrash && crashedInstance.data?.settings.javaMode === "custom";

  // A crash in a ready-made set's mod: when the team has changed that set since, its update may
  // fix it, so the dialog points at "Güncelle" in the mod center's sets (FIX2).
  const modCrash = step?.kind === "crash" && Boolean(crash?.suspects.length);
  const instanceSets = useQuery({
    queryKey: ["mod-sets-instance", instanceId],
    queryFn: () => api.modSets.instance(instanceId ?? ""),
    enabled: open && modCrash && Boolean(instanceId),
    staleTime: 15_000,
    retry: false,
  });
  const updateSet =
    modCrash && crash && instanceSets.data ? setToUpdate(crash, instanceSets.data.sets) : null;

  const close = () => {
    flow.close();
    dismissCrash();
    repair.reset();
    reinstall.reset();
    setTurnedOff(null);
    setWarningsOpen(false);
    setBusy(null);
  };
  const go = (next: FlowStep) => {
    setDirection(1);
    flow.go(next);
  };
  const back = () => {
    setDirection(-1);
    if (!flow.back()) close();
  };
  const playNow = (ignoreModIssues = false) => {
    const id = instanceId;
    close();
    if (id) void play(id, ignoreModIssues ? { ignoreModIssues: true } : undefined);
  };
  // "Onar" from the mod check or the crash summary: the repair starts at once, and the button
  // stays pressed in until its plan is known (at most REPAIR_HOLD_MS), so "Modlar onarılıyor"
  // opens with its checklist at its height rather than a placeholder that then grows. Meanwhile
  // the dialog's other actions wait.
  const startRepair = (scope: RepairScope) => {
    if (holding) return;
    ran.current = `${flow.session}:${JSON.stringify(scope)}`;
    setHolding({ scope, until: performance.now() + REPAIR_HOLD_MS });
    void repair.start(scope);
  };
  const planned =
    (repair.progress?.steps.length ?? 0) > 0 || repair.result !== null || repair.error !== null;
  // biome-ignore lint/correctness/useExhaustiveDependencies: `go` is the same action every render
  useEffect(() => {
    if (!holding) return;
    const enter = () => {
      setHolding(null);
      go({ kind: "repair", scope: holding.scope });
    };
    if (planned) {
      enter();
      return;
    }
    const timer = window.setTimeout(enter, Math.max(0, holding.until - performance.now()));
    return () => window.clearTimeout(timer);
  }, [holding, planned]);
  /** An action of the step "Onar" was pressed on: it waits while the plan comes. */
  const unlessHolding = (action: () => void) => () => {
    if (!holding) action();
  };
  // The pack's mod keys clash (KEYS2): asked before the launch, in this dialog.
  const keys = useKeysStep({ step, instanceId, session: flow.session, playNow: () => playNow() });

  let title: ReactNode = "";
  let titleKey = "none";
  let body: ReactNode = null;
  let footer: ReactNode = null;
  let dismissible = true;

  if (step?.kind === "crash" && crash) {
    dismissible = !holding;
    const strong = crash.suspects.slice(0, 5);
    const kind = crash.kind;
    const turnOffLabel = crash.turnOff
      ? strong.length > 1
        ? t("modHealth.crash.turnOffNamed", { name: shorten(crash.turnOff.name) })
        : t("modHealth.crash.turnOff")
      : "";
    const memoryLine = crash.memory
      ? t("modHealth.crash.memory", {
          from: formatMemory(crash.memory.currentMb),
          to: formatMemory(crash.memory.suggestedMb),
        })
      : null;
    const copyDetails = () => {
      const lines = [
        crash.title,
        crash.hint,
        `kind: ${kind ?? "?"}`,
        ...crash.suspects.map((s) => `${s.name}: ${s.reason}${s.related ? ` (${s.related})` : ""}`),
        crash.mods.length ? `mods: ${crash.mods.join(", ")}` : "",
        crash.logFile ?? "",
        crash.crashReport ?? "",
      ];
      void copySupportText(lines.filter(Boolean).join("\n"));
    };
    const turnOff = async () => {
      if (!crash.turnOff || !instanceId) return;
      setBusy("turnOff");
      try {
        await api.setModEnabled(instanceId, crash.turnOff.file, false);
        setTurnedOff(crash.turnOff.name);
      } catch (error) {
        showError(error);
      } finally {
        setBusy(null);
      }
    };
    const applyMemory = async () => {
      if (!crash.memory || !instanceId) return;
      setBusy("memory");
      try {
        const saved = await api.modHealth.setMaxMemory(instanceId, crash.memory.suggestedMb);
        toast.success(t("modHealth.crash.memorySet", { size: formatMemory(saved) }));
        playNow();
      } catch (error) {
        showError(error);
        setBusy(null);
      }
    };
    // The player's own JVM arguments stopped the JVM: clear them and retry; "Geri al" in the
    // notice puts them back for the next launch.
    const resetArgs = async () => {
      if (!crash.resetJvmArgs || !instanceId) return;
      const id = instanceId;
      setBusy("args");
      try {
        const previous = await api.modHealth.setJvmArgs(id, "");
        void queryClient.invalidateQueries({ queryKey: ["instance", id] });
        toast.success(t("modHealth.crash.argsReset"), {
          action: {
            label: t("modHealth.crash.argsUndo"),
            onClick: () => {
              void api.modHealth
                .setJvmArgs(id, previous)
                .then(() => queryClient.invalidateQueries({ queryKey: ["instance", id] }))
                .catch(showError);
            },
          },
        });
        playNow();
      } catch (error) {
        showError(error);
        setBusy(null);
      }
    };
    const shift = (px: number) => (prefs.reduce ? "none" : `translateY(${px}px)`);
    titleKey = `crash:${kind}`;
    title = (
      <span className="flex items-center gap-2.5">
        <WarningCircle size={22} weight="fill" className="shrink-0 text-flare-400" />
        {crashTitle(t, kind, crash.title)}
      </span>
    );
    body = (
      <div className="space-y-4">
        <AnimatePresence initial={false}>
          {turnedOff ? (
            <motion.p
              key="off"
              {...rise(prefs.reduce, 4, 0.24)}
              className="flex items-center gap-2.5 text-[14px] font-medium text-bone-50"
            >
              <CheckCircle size={19} weight="fill" className="shrink-0 text-jade-400" />
              {t("modHealth.crash.turnedOff", { name: turnedOff })}
            </motion.p>
          ) : null}
        </AnimatePresence>
        <p className="text-[14.5px] leading-relaxed text-bone-100">
          {crashHint(t, kind, crash.hint)}
          {memoryLine ? <span className="mt-1 block text-ash-300">{memoryLine}</span> : null}
        </p>
        {strong.length > 0 ? (
          <section>
            <h3 className="mb-2 text-[12.5px] font-medium text-ash-400">
              {t("modHealth.crash.suspects")}
            </h3>
            <ul className="overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950/45 ring-1 ring-white/7">
              {strong.map((suspect, index) => (
                <motion.li
                  key={`${suspect.name}-${suspect.reason}`}
                  initial={{ opacity: 0, transform: shift(4) }}
                  animate={{
                    opacity: 1,
                    transform: shift(0),
                    transition: { ...enter(0.26), delay: 0.06 + stagger(index, 0.05) },
                  }}
                  className="flex items-baseline gap-3 border-white/6 border-t px-3.5 py-2.5 first:border-t-0"
                >
                  <span
                    className={cn(
                      "shrink-0 text-[14px] font-semibold",
                      suspect.strong ? "text-bone-50" : "text-ash-300",
                    )}
                  >
                    {suspect.name}
                  </span>
                  <span className="min-w-0 text-[13px] text-ash-400">
                    {suspectText(t, suspect)}
                  </span>
                </motion.li>
              ))}
            </ul>
          </section>
        ) : null}
        <AnimatePresence initial={false}>
          {updateSet && instanceId ? (
            <motion.div
              key="set-update"
              {...rise(prefs.reduce, 4, 0.24)}
              className="flex items-center gap-3 rounded-[var(--radius-panel)] bg-gold-500/[0.07] py-2 pr-2 pl-3.5 ring-1 ring-gold-400/15"
            >
              <ArrowsClockwise size={18} className="shrink-0 text-gold-300" aria-hidden />
              <p className="min-w-0 flex-1 text-[13.5px] text-bone-100">
                {t("modSets.crash.update", {
                  name: setName(updateSet, i18n.language.startsWith("en")),
                })}
              </p>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => {
                  const id = instanceId;
                  close();
                  void navigate({
                    to: "/library/$instanceId",
                    params: { instanceId: id },
                    search: { tab: "mods", focus: "sets" },
                  });
                }}
              >
                {t("modSets.crash.open")}
              </Button>
            </motion.div>
          ) : null}
        </AnimatePresence>
      </div>
    );
    const settings = kind && settingsKinds.has(kind) && !crash.repairable;
    // "Kur ve oyna" for a Java crash of an instance on a custom Java (J1).
    const javaFix = Boolean(settings && customJava && !crash.turnOff);
    footer = (
      <div className="flex w-full items-center gap-1.5">
        <IconButton
          label={t("modHealth.crash.copy")}
          icon={<Copy size={17} />}
          size="sm"
          tooltipSide="top"
          onClick={copyDetails}
        />
        {crash.logFile && instanceId ? (
          <IconButton
            label={t("modHealth.crash.openLog")}
            icon={<FileText size={17} />}
            size="sm"
            tooltipSide="top"
            onClick={() => {
              const id = instanceId;
              const log = crash.logFile?.split(/[\\/]/).pop();
              close();
              void navigate({
                to: "/library/$instanceId",
                params: { instanceId: id },
                search: { tab: "logs", log },
              });
            }}
          />
        ) : null}
        {/* The troubleshooter (HELP1): the connection, the disk, Java and the mods beside the crash. */}
        <IconButton
          label={t("help.title")}
          icon={<Lifebuoy size={17} />}
          size="sm"
          tooltipSide="top"
          onClick={() => {
            close();
            openHelp();
          }}
        />
        <div className="relative ml-auto flex items-center justify-end">
          <AnimatePresence mode="popLayout" initial={false}>
            {turnedOff ? (
              <motion.div key="after" {...swap(0.2)} className="flex items-center gap-2.5">
                <Button variant="ghost" onClick={close}>
                  {t("common.close")}
                </Button>
                <Button variant="primary" onClick={() => playNow()}>
                  {t("modHealth.crash.play")}
                </Button>
              </motion.div>
            ) : (
              <motion.div key="actions" {...swap(0.2)} className="flex items-center gap-2.5">
                {crash.reinstall ? (
                  <Button variant="ghost" onClick={() => go({ kind: "reinstall" })}>
                    {t("modHealth.crash.reinstall")}
                  </Button>
                ) : null}
                {javaFix ? (
                  <Button variant="ghost" onClick={close}>
                    {t("modHealth.crash.ok")}
                  </Button>
                ) : null}
                {settings && instanceId ? (
                  <Button
                    variant="secondary"
                    onClick={() => {
                      const id = instanceId;
                      close();
                      void navigate({
                        to: "/library/$instanceId",
                        params: { instanceId: id },
                        search: { tab: "settings" },
                      });
                    }}
                  >
                    {t("modHealth.crash.settings")}
                  </Button>
                ) : null}
                {crash.turnOff ? (
                  <Button
                    variant={crash.repairable ? "secondary" : "primary"}
                    disabled={busy !== null}
                    onClick={() => void turnOff()}
                  >
                    {turnOffLabel}
                  </Button>
                ) : null}
                {crash.repairable ? (
                  <Button
                    variant="primary"
                    disabled={busy !== null}
                    className={holding ? "scale-[0.97]" : undefined}
                    onClick={() => startRepair({ kind: "crash" })}
                  >
                    {t("modHealth.crash.repair")}
                  </Button>
                ) : crash.resetJvmArgs ? (
                  <Button
                    variant="primary"
                    disabled={busy !== null}
                    onClick={() => void resetArgs()}
                  >
                    {t("modHealth.crash.resetArgs")}
                  </Button>
                ) : crash.memory ? (
                  <Button
                    variant="primary"
                    disabled={busy !== null}
                    onClick={() => void applyMemory()}
                  >
                    {crash.memory.increase
                      ? t("modHealth.crash.increase")
                      : t("modHealth.crash.decrease")}
                  </Button>
                ) : !crash.turnOff && !javaFix ? (
                  <Button variant="primary" onClick={close}>
                    {t("modHealth.crash.ok")}
                  </Button>
                ) : null}
                {javaFix && instanceId ? (
                  <Button
                    variant="primary"
                    icon={<DownloadSimple size={16} weight="bold" />}
                    onClick={() => {
                      const id = instanceId;
                      close();
                      void play(id, { java: "install" });
                    }}
                  >
                    {t("javaPrompt.install")}
                  </Button>
                ) : null}
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>
    );
  } else if (step?.kind === "blocked") {
    const check = step.check;
    const blocking = check.issues.filter((i) => i.severity === "blocking");
    const warnings = check.issues.filter((i) => i.severity === "warning");
    titleKey = "blocked";
    title = t("modHealth.prelaunch.title");
    body = (
      <div>
        <IssueList
          issues={blocking}
          minecraft={check.minecraft}
          label={t("modHealth.prelaunch.title")}
        />
        {warnings.length > 0 ? (
          <div className="mt-3 border-white/6 border-t pt-1.5">
            <button
              type="button"
              aria-expanded={warningsOpen}
              onClick={() => setWarningsOpen((v) => !v)}
              className="-ml-1 flex h-8 cursor-pointer items-center gap-2 rounded-md px-1 text-[13px] text-ash-300 transition-colors duration-150 hover:text-bone-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              <CaretDown
                size={14}
                className={cn(
                  "transition-transform duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]",
                  warningsOpen && "rotate-180",
                )}
              />
              {t("modHealth.prelaunch.warnings", { count: warnings.length })}
            </button>
            <AnimatePresence initial={false} mode="popLayout">
              {warningsOpen ? (
                <motion.div key="warnings" {...rise(prefs.reduce, 4, 0.2)}>
                  <IssueList
                    issues={warnings}
                    minecraft={check.minecraft}
                    label={t("modHealth.prelaunch.warnings", { count: warnings.length })}
                    dense
                  />
                </motion.div>
              ) : null}
            </AnimatePresence>
          </div>
        ) : null}
      </div>
    );
    dismissible = !holding;
    footer = (
      <>
        <Button variant="ghost" onClick={unlessHolding(close)}>
          {t("modHealth.prelaunch.cancel")}
        </Button>
        <Button variant="secondary" onClick={unlessHolding(() => playNow(true))}>
          {t("modHealth.prelaunch.anyway")}
        </Button>
        {check.repairable ? (
          <Button
            variant="primary"
            className={holding ? "scale-[0.97]" : undefined}
            onClick={() => startRepair({ kind: "all" })}
          >
            {t("modHealth.prelaunch.repair")}
          </Button>
        ) : check.hasMods ? (
          <Button variant="primary" onClick={() => go({ kind: "reinstall" })}>
            {t("modHealth.banner.reinstall")}
          </Button>
        ) : null}
      </>
    );
  } else if (keys) {
    ({ titleKey, title, body, footer } = keys);
  } else if (step?.kind === "repair") {
    const result = repair.result;
    const steps: RepairStep[] = result?.steps ?? repair.progress?.steps ?? [];
    const running = repair.running || (!result && !repair.error);
    const rows: ChecklistRow[] = steps.map((s) => ({
      key: s.key,
      status: s.status,
      text: stepText(t, s),
      note: stepNote(t, s),
      percent: s.status === "running" ? repair.progress?.percent : undefined,
    }));
    // The pack's key clash is no mod problem "Onar" leaves: it keeps its own "Düzelt".
    const left: HealthIssue[] = (result?.check.issues ?? []).filter((i) => i.kind !== "keyClash");
    const blockingLeft = (result?.check.blocking ?? 0) > 0;
    dismissible = !running;
    titleKey = running
      ? "repair:running"
      : result?.cancelled
        ? "repair:stopped"
        : left.length > 0
          ? "repair:partial"
          : steps.length === 0
            ? "repair:nothing"
            : "repair:done";
    title = running
      ? t("modHealth.repair.working")
      : repair.error
        ? t("modHealth.repair.partial")
        : result?.cancelled
          ? t("modHealth.repair.stopped")
          : left.length > 0
            ? t("modHealth.repair.partial")
            : steps.length === 0
              ? t("modHealth.repair.nothing")
              : t("modHealth.repair.done");
    body = (
      <div>
        <AnimatePresence initial={false}>
          {/* Nothing was left to do: say the game is ready (the steps say it otherwise). */}
          {result && !result.cancelled && left.length === 0 && steps.length === 0 ? (
            <motion.p
              key="ready"
              {...rise(prefs.reduce, 4, 0.26)}
              className="mb-3 flex items-center gap-2.5 text-[14px] font-medium text-bone-50"
            >
              <CheckCircle size={19} weight="fill" className="shrink-0 text-jade-400" />
              {t("modHealth.repair.ready")}
            </motion.p>
          ) : null}
        </AnimatePresence>
        {/* Until the plan comes the steps' place is held by rows as tall as the checklist's
            (34 px), so the panel opens at about its final height: the old short skeleton made
            it shrink ~30 px and grow back as the list came (POLISH6). The rows show only after
            150 ms, so a plan that comes at once never flashes grey bars. */}
        <div className="relative">
          <AnimatePresence mode="popLayout">
            {steps.length === 0 && running ? (
              <motion.ul
                key="planning"
                aria-busy
                aria-label={t("modHealth.repair.progress")}
                className="space-y-px"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: { ...enter(0.2), delay: 0.15 } }}
                exit={{ opacity: 0, transition: leave(0.12) }}
              >
                {["w-1/2", "w-2/5", "w-3/5"].map((width) => (
                  <li key={width} className="flex items-center gap-3 py-[7px]">
                    <span className="grid size-5 shrink-0 place-items-center">
                      <Skeleton className="size-4 rounded-full" />
                    </span>
                    <Skeleton className={`h-3.5 ${width}`} />
                  </li>
                ))}
              </motion.ul>
            ) : (
              <motion.div key="steps" className="max-h-[38vh] overflow-y-auto pr-1">
                <Checklist rows={rows} label={t("modHealth.repair.progress")} />
              </motion.div>
            )}
          </AnimatePresence>
        </div>
        {repair.error ? (
          <motion.p {...fade(0.2)} className="mt-3 text-[13.5px] text-ash-300">
            {repair.error instanceof Error ? repair.error.message : String(repair.error)}
          </motion.p>
        ) : null}
        <AnimatePresence initial={false}>
          {result && left.length > 0 ? (
            <motion.section
              key="left"
              {...rise(prefs.reduce, 6, 0.26)}
              className="mt-4 border-white/6 border-t pt-3"
            >
              <p className="mb-1 text-[12.5px] font-medium text-ash-400">
                {t("modHealth.repair.left", { count: left.length })}
              </p>
              <IssueList
                issues={left}
                minecraft={result.check.minecraft}
                label={t("modHealth.repair.left", { count: left.length })}
                dense
              />
            </motion.section>
          ) : null}
        </AnimatePresence>
      </div>
    );
    const current = currentStep(steps);
    const label = repair.progress?.checking
      ? t("modHealth.repair.checking")
      : current
        ? stepText(t, { ...current, status: "running" })
        : t("modHealth.repair.preparing");
    footer = (
      <div className="relative flex min-h-10 w-full items-center justify-end">
        <AnimatePresence initial={false} mode="popLayout">
          {running ? (
            <motion.div key="progress" {...swap(0.2)} className="flex w-full items-center gap-4">
              <RunLine
                label={label}
                percent={repairPercent(repair.progress)}
                bar={t("modHealth.repair.progress")}
              />
              <Button variant="ghost" size="sm" onClick={repair.stop}>
                {t("modHealth.repair.stop")}
              </Button>
            </motion.div>
          ) : (
            <motion.div key="actions" {...swap(0.2)} className="flex items-center gap-2.5">
              {blockingLeft || repair.error ? (
                <>
                  <Button variant="ghost" onClick={close}>
                    {t("modHealth.repair.close")}
                  </Button>
                  {playAfter ? (
                    <Button variant="secondary" onClick={() => playNow(true)}>
                      {t("modHealth.prelaunch.anyway")}
                    </Button>
                  ) : null}
                  {result?.check.hasMods ? (
                    <Button variant="primary" onClick={() => go({ kind: "reinstall" })}>
                      {t("modHealth.banner.reinstall")}
                    </Button>
                  ) : null}
                </>
              ) : playAfter ? (
                <>
                  <Button variant="ghost" onClick={close}>
                    {t("modHealth.repair.close")}
                  </Button>
                  <Button variant="primary" onClick={() => playNow()}>
                    {t("modHealth.repair.play")}
                  </Button>
                </>
              ) : (
                <Button variant="primary" onClick={close}>
                  {t("modHealth.reinstall.close")}
                </Button>
              )}
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    );
  } else if (step?.kind === "reinstall") {
    const result = reinstall.result;
    const progress = reinstall.progress;
    const phase: "confirm" | "running" | "result" = result
      ? "result"
      : reinstall.running || progress
        ? "running"
        : "confirm";
    dismissible = phase !== "running" || Boolean(progress?.cancellable);
    if (phase === "confirm") {
      const p = preview.data;
      titleKey = "reinstall:confirm";
      title = t("modHealth.reinstall.title");
      const kept = p?.kept ?? [];
      const keptNames =
        kept.length > 3
          ? joinNames([
              ...kept.slice(0, 3),
              t("modHealth.reinstall.more", { count: kept.length - 3 }),
            ])
          : joinNames(kept);
      body = preview.isPending ? (
        <FactsSkeleton />
      ) : preview.isError || !p ? (
        <p className="text-[13.5px] text-ash-300">
          {preview.error instanceof Error ? preview.error.message : ""}
        </p>
      ) : (
        <ul className="space-y-3.5">
          <Fact icon={<Package size={20} />} index={0}>
            {t("modHealth.reinstall.intro", { version: p.minecraft })}
          </Fact>
          {p.resetsSettings ? (
            <Fact icon={<Warning size={20} weight="fill" className="text-gold-300" />} index={1}>
              {t("modHealth.reinstall.settings")}
            </Fact>
          ) : null}
          {kept.length > 0 ? (
            <Fact icon={<FolderSimple size={20} />} index={2}>
              {t("modHealth.reinstall.kept", { count: kept.length, names: keptNames })}
            </Fact>
          ) : null}
          <Fact icon={<Trash size={20} />} index={3}>
            {t("modHealth.reinstall.trash")}
          </Fact>
          {p.needsNetwork ? (
            <Fact icon={<WifiSlash size={20} className="text-flare-400" />} index={4}>
              {t("modHealth.reinstall.network")}
            </Fact>
          ) : null}
        </ul>
      );
      footer = (
        <>
          <Button variant="ghost" onClick={back}>
            {t("modHealth.reinstall.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={!p || p.needsNetwork}
            onClick={() => void reinstall.start()}
          >
            {t("modHealth.reinstall.confirm")}
          </Button>
        </>
      );
    } else if (phase === "running") {
      const phases: ReinstallPhase[] = progress?.phases ?? [];
      const at = progress ? phases.indexOf(progress.phase) : -1;
      const rows: ChecklistRow[] = phases.map((name, index) => {
        const status = index < at ? "done" : index === at ? "running" : "pending";
        return {
          key: name,
          status,
          text: t(`modHealth.reinstall.phases.${name}.${status}`),
          percent: status === "running" ? progress?.percent : undefined,
        };
      });
      titleKey = "reinstall:running";
      title = t("modHealth.reinstall.working");
      body =
        rows.length === 0 ? (
          <FactsSkeleton />
        ) : (
          <Checklist rows={rows} label={t("modHealth.reinstall.progress")} />
        );
      const label =
        progress && at >= 0
          ? t(`modHealth.reinstall.phases.${progress.phase}.running`)
          : t("modHealth.repair.preparing");
      footer = (
        <div className="flex w-full items-center gap-4">
          <RunLine
            label={label}
            percent={reinstallPercent(progress)}
            bar={t("modHealth.reinstall.progress")}
          />
          <AnimatePresence initial={false}>
            {progress?.cancellable ? (
              <motion.span key="stop" {...fade(0.16)}>
                <Button variant="ghost" size="sm" onClick={reinstall.stop}>
                  {t("modHealth.reinstall.stop")}
                </Button>
              </motion.span>
            ) : null}
          </AnimatePresence>
        </div>
      );
    } else if (result) {
      const installed =
        result.reinstalled + (result.packSynced ? (preview.data?.packMods ?? 0) : 0);
      const left = result.check.issues.filter((i) => i.kind !== "keyClash");
      titleKey = result.cancelled ? "reinstall:stopped" : "reinstall:done";
      title = result.cancelled ? t("modHealth.reinstall.stopped") : t("modHealth.reinstall.done");
      const keptFolder = result.keptFolder;
      const facts: Array<{ key: string; icon: ReactNode; text: string; action?: ReactNode }> = [];
      if (installed > 0) {
        facts.push({
          key: "installed",
          icon: <CheckCircle size={20} weight="fill" className="text-jade-400" />,
          text: t("modHealth.reinstall.reinstalled", { count: installed }),
        });
      }
      if (result.kept.length > 0) {
        facts.push({
          key: "kept",
          icon: <FolderSimple size={20} />,
          text: t("modHealth.reinstall.keptDone", { count: result.kept.length }),
          action:
            keptFolder && instanceId ? (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void api.openInstanceFolder(instanceId, keptFolder).catch(showError)}
              >
                {t("modHealth.reinstall.openFolder")}
              </Button>
            ) : undefined,
        });
      }
      if (result.restored.length > 0) {
        facts.push({
          key: "restored",
          icon: <ArrowCounterClockwise size={20} />,
          text: t("modHealth.reinstall.restored", { names: joinNames(result.restored) }),
        });
      }
      if (!result.packSynced && (preview.data?.packMods ?? 0) > 0) {
        facts.push({
          key: "pack",
          icon: <Package size={20} />,
          text: t("modHealth.reinstall.packLater"),
        });
      }
      if (result.settingsReset) {
        facts.push({
          key: "settings",
          icon: <SlidersHorizontal size={20} />,
          text: t("modHealth.reinstall.settingsReset"),
        });
      }
      body = result.cancelled ? (
        <p className="text-[13.5px] text-bone-100">{t("modHealth.reinstall.nothingMoved")}</p>
      ) : (
        <div>
          <ul className="space-y-3.5">
            {facts.map((fact, index) => (
              <Fact key={fact.key} icon={fact.icon} index={index} action={fact.action}>
                {fact.text}
              </Fact>
            ))}
          </ul>
          {left.length > 0 ? (
            <motion.section
              {...rise(prefs.reduce, 6, 0.26)}
              className="mt-4 border-white/6 border-t pt-3"
            >
              <p className="mb-1 text-[12.5px] font-medium text-ash-400">
                {t("modHealth.reinstall.left", { count: left.length })}
              </p>
              <IssueList
                issues={left}
                minecraft={result.check.minecraft}
                label={t("modHealth.reinstall.left", { count: left.length })}
                dense
              />
            </motion.section>
          ) : null}
        </div>
      );
      footer = playAfter ? (
        <>
          <Button variant="ghost" onClick={close}>
            {t("modHealth.repair.close")}
          </Button>
          <Button variant="primary" onClick={() => playNow()}>
            {t("modHealth.reinstall.play")}
          </Button>
        </>
      ) : (
        <Button variant="primary" onClick={close}>
          {t("modHealth.reinstall.close")}
        </Button>
      );
    }
    if (reinstall.error) {
      titleKey = "reinstall:error";
      title = t("modHealth.reinstall.stopped");
      body = (
        <p className="text-[13.5px] leading-relaxed text-bone-100">
          {reinstall.error instanceof Error ? reinstall.error.message : String(reinstall.error)}
        </p>
      );
      footer = (
        <Button variant="primary" onClick={close}>
          {t("modHealth.repair.close")}
        </Button>
      );
      dismissible = true;
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && dismissible && close()}
      title={
        <span className="relative block">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span key={titleKey} {...swap(0.18)} className="block">
              {title}
            </motion.span>
          </AnimatePresence>
        </span>
      }
      closeLabel={t("common.close")}
      width={600}
      footer={footer}
      morph={prefs.layout}
      dismissible={dismissible}
    >
      <AnimatePresence mode="popLayout" initial={false} custom={direction}>
        <motion.div
          key={step?.kind ?? "none"}
          custom={direction}
          variants={stepVariants(prefs.reduce)}
          initial="enter"
          animate="center"
          exit="exit"
        >
          {body}
        </motion.div>
      </AnimatePresence>
    </Dialog>
  );
}
