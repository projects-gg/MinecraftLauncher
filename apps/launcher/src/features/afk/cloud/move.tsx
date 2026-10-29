// "Buluta taşı": a desktop AFK account's settings written over one of the player's cloud accounts
// (its secret lines sealed by the backend, their words kept in the keychain), then stopped here,
// its start with the launcher turned off, and started in the cloud. Steps: pick the cloud account
// (skipped when the choice is clear), see what moves and what stays on the desktop, done. Buying
// a new account always happens on the site.
import {
  ArrowRight,
  ArrowSquareOut,
  CheckCircle,
  Terminal,
  WarningCircle,
} from "@phosphor-icons/react";
import {
  Button,
  ContentIcon,
  cn,
  Dialog,
  EmptyState,
  easeOutExpo,
  PixelBrackets,
  Skeleton,
  Switch,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AfkAccount } from "@/lib/api";
import { AfkHead } from "../afk-head";
import { afkFailed, isFriendly } from "../errors";
import { type CloudAccount, type CloudMoveData, type CloudMovePlan, cloudApi } from "./api";
import { serverLabel, timeLeft } from "./status";
import { CLOUD_KEY, storeAccount } from "./use-cloud";

type Step = "pick" | "confirm" | "done";

/** The candidate a desktop account should go to: its twin, else the only account. */
function preselect(desktop: AfkAccount, accounts: CloudAccount[]): CloudAccount | null {
  const twin = accounts.find(
    (a) =>
      a.name.toLowerCase() === desktop.nickname.toLowerCase() &&
      serverLabel(a.server) === desktop.server,
  );
  return twin ?? (accounts.length === 1 ? (accounts[0] ?? null) : null);
}

function stepMotion(reduce: boolean, direction: 1 | -1) {
  const from = reduce ? "none" : `translateX(${direction * 24}px)`;
  const to = reduce ? "none" : `translateX(${direction * -24}px)`;
  return {
    initial: { opacity: 0, transform: from },
    animate: {
      opacity: 1,
      transform: reduce ? "none" : "translateX(0px)",
      transition: { duration: 0.28, ease: easeOutExpo },
    },
    exit: { opacity: 0, transform: to, transition: { duration: 0.18, ease: easeOutExpo } },
  };
}

function Pick({
  accounts,
  chosen,
  onChoose,
  desktop,
  skewMs,
}: {
  accounts: CloudAccount[];
  chosen: string | null;
  onChoose: (id: string) => void;
  desktop: AfkAccount;
  skewMs: number;
}) {
  const { t } = useTranslation();
  const now = Date.now() + skewMs;
  const twinId = preselect(desktop, accounts)?.id;
  return (
    <div className="flex flex-col gap-2.5">
      <p className="text-[13px] text-ash-300">{t("cloudAfk.move.pick")}</p>
      <div role="radiogroup" aria-label={t("cloudAfk.move.pick")} className="flex flex-col gap-2">
        {accounts.map((account) => {
          const on = account.id === chosen;
          return (
            // biome-ignore lint/a11y/useSemanticElements: a styled radio row (radiogroup above)
            <button
              key={account.id}
              type="button"
              role="radio"
              aria-checked={on}
              onClick={() => onChoose(account.id)}
              className={cn(
                "relative flex w-full cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-3 py-2.5 text-left outline-none",
                "bg-white/[0.03] ring-1 transition-[background-color,box-shadow,transform,scale] duration-150 ease-out focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.985]",
                // The chosen account is the quiet plate with bone brackets (design.md §6): the
                // dialog's gold is its own button.
                on ? "bg-white/[0.06] ring-white/25" : "ring-white/7 hover:bg-white/[0.05]",
              )}
            >
              {on ? <PixelBrackets size={8} className="-inset-0.5" /> : null}
              <AfkHead nickname={account.name} size={32} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13.5px] font-medium text-bone-50">
                  {account.name}
                </span>
                <span className="block truncate text-[12px] text-ash-300">
                  {serverLabel(account.server)}
                </span>
              </span>
              <span className="shrink-0 text-right text-[12px] text-ash-300 tabular-nums">
                {account.id === twinId ? (
                  <span className="block text-gold-200">{t("cloudAfk.move.twin")}</span>
                ) : null}
                <span className="block">{timeLeft(account, now, t)}</span>
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function Confirm({
  plan,
  target,
  desktop,
  run,
  onRun,
  failure,
}: {
  plan: CloudMovePlan | undefined;
  target: CloudAccount;
  desktop: AfkAccount;
  run: boolean;
  onRun: (run: boolean) => void;
  failure: string | null;
}) {
  const { t } = useTranslation();
  const runId = useId();
  if (!plan) {
    return (
      <div className="flex flex-col gap-3">
        <Skeleton className="h-5 w-56" />
        <Skeleton className="h-14 w-full" />
        <Skeleton className="h-10 w-full" />
      </div>
    );
  }
  const secrets = plan.config.commands.some((row) => row.words.kind === "secret");
  const replaces =
    target.name.toLowerCase() !== plan.name.toLowerCase() ||
    (plan.server !== null && serverLabel(target.server) !== serverLabel(plan.server));
  const moves = [
    plan.config.commands.length
      ? t("cloudAfk.move.rows.commands", { count: plan.config.commands.length })
      : null,
    plan.config.rules.length
      ? t("cloudAfk.move.rows.rules", { count: plan.config.rules.length })
      : null,
    plan.config.antiAfk.enabled ? t("cloudAfk.move.rows.antiAfk") : null,
  ].filter((line): line is string => line !== null);
  return (
    <div className="flex flex-col gap-4">
      {/* From this computer to the chosen cloud account. */}
      <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-3 rounded-[var(--radius-panel)] bg-white/[0.03] px-3 py-2.5 ring-1 ring-white/7">
        <span className="flex min-w-0 items-center gap-2.5">
          <AfkHead nickname={desktop.nickname} size={32} />
          <span className="min-w-0">
            <span className="block truncate text-[13.5px] font-medium text-bone-50">
              {desktop.nickname}
            </span>
            <span className="block truncate text-[12px] text-ash-300">
              {t("afkClear.here.title")}
            </span>
          </span>
        </span>
        <ArrowRight size={16} className="text-ash-400" aria-hidden />
        <span className="flex min-w-0 items-center gap-2.5">
          <span className="chamfer grid size-8 shrink-0 place-items-center bg-obsidian-800 [--cut:6px]">
            <ContentIcon kind="cloud" size={16} />
          </span>
          <span className="min-w-0">
            <span className="block truncate text-[13.5px] font-medium text-bone-50">
              {target.name}
            </span>
            <span className="block truncate text-[12px] text-ash-300">{t("cloudAfk.title")}</span>
          </span>
        </span>
      </div>
      {replaces ? (
        <p className="flex items-start gap-2 text-[12.5px] leading-snug text-gold-200">
          <WarningCircle size={15} weight="fill" className="mt-px shrink-0" aria-hidden />
          <span>{t("cloudAfk.move.replaces", { target: target.name })}</span>
        </p>
      ) : null}
      <section>
        <h3 className="mb-1.5 text-[12.5px] font-medium text-ash-300">
          {t("cloudAfk.move.moves")}
        </h3>
        <ul className="flex flex-col gap-1 text-[13px] text-bone-100">
          {(moves.length ? moves : [t("cloudAfk.move.rows.nothing")]).map((line) => (
            <li key={line} className="flex items-center gap-2">
              <CheckCircle size={15} weight="fill" className="shrink-0 text-gold-400" aria-hidden />
              {line}
            </li>
          ))}
        </ul>
        {secrets ? (
          <p className="mt-2 text-[12px] text-ash-300">{t("cloudAfk.move.secretsSealed")}</p>
        ) : null}
      </section>
      {plan.dropped.length ? (
        <section>
          <h3 className="mb-1.5 text-[12.5px] font-medium text-ash-300">
            {t("cloudAfk.move.dropped")}
          </h3>
          <ul className="flex flex-col gap-1 text-[13px] text-ash-300">
            {plan.dropped.map((item) => (
              <li key={item.what} className="flex items-center gap-2">
                <span aria-hidden className="h-px w-2.5 shrink-0 bg-ash-500" />
                {t(`cloudAfk.move.droppedKinds.${item.what}`, { count: item.count })}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {plan.adjusted.length ? (
        <ul className="flex flex-col gap-1 text-[12px] text-ash-300">
          {plan.adjusted.map((item) => (
            <li key={item}>{t(`cloudAfk.move.adjusted.${item}`)}</li>
          ))}
        </ul>
      ) : null}
      <div className="switch-row flex items-center justify-between gap-6 border-t border-white/7 pt-3.5">
        <label htmlFor={runId} className="cursor-pointer text-[13.5px] font-medium text-bone-50">
          {t("cloudAfk.move.run")}
        </label>
        <Switch
          id={runId}
          checked={run}
          onCheckedChange={onRun}
          label={t("cloudAfk.move.run")}
          hideLabel
        />
      </div>
      {plan.server === null || failure ? (
        <p role="alert" className="text-[12.5px] text-flare-400">
          {failure ?? t("cloudAfk.move.serverProblem")}
        </p>
      ) : null}
    </div>
  );
}

function Done({ result, desktop }: { result: CloudMoveData; desktop: AfkAccount }) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  return (
    <div className="flex flex-col items-center gap-3 py-2 text-center">
      <motion.span
        className="chamfer grid size-[72px] place-items-center bg-obsidian-800 [--cut:10px]"
        initial={{ opacity: 0, transform: reduce ? "none" : "scale(0.92)" }}
        animate={{
          opacity: 1,
          transform: reduce ? "none" : "scale(1)",
          transition: { duration: 0.34, ease: easeOutExpo },
        }}
      >
        <ContentIcon kind="done" size={48} />
      </motion.span>
      <p className="font-display text-[16px] font-semibold text-bone-50">
        {t("cloudAfk.move.doneTitle", { name: desktop.nickname })}
      </p>
      <div className="flex max-w-[44ch] flex-col gap-1 text-[13px] text-ash-300">
        <p>{result.started ? t("cloudAfk.move.doneRunning") : t("cloudAfk.move.doneStopped")}</p>
        <p>{t("cloudAfk.move.doneHere")}</p>
        <p>{t("cloudAfk.move.doneSecrets")}</p>
      </div>
    </div>
  );
}

export function CloudMoveDialog({
  desktop,
  accounts,
  skewMs,
  onClose,
  onOpenConsole,
}: {
  desktop: AfkAccount | undefined;
  accounts: CloudAccount[];
  skewMs: number;
  onClose: () => void;
  onOpenConsole: (cloudId: string) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const queryClient = useQueryClient();
  const open = Boolean(desktop);
  const [chosen, setChosen] = useState<string | null>(null);
  const [step, setStep] = useState<Step>("pick");
  const [direction, setDirection] = useState<1 | -1>(1);
  const [run, setRun] = useState(true);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [result, setResult] = useState<CloudMoveData | null>(null);

  // A fresh open: the clear choice made already, the rest asked.
  // biome-ignore lint/correctness/useExhaustiveDependencies: resets when another account opens
  useEffect(() => {
    if (!desktop) return;
    const first = preselect(desktop, accounts);
    setChosen(first?.id ?? null);
    setStep(first ? "confirm" : "pick");
    setDirection(1);
    setRun(true);
    setFailure(null);
    setResult(null);
  }, [desktop?.id]);

  const { data: plan } = useQuery({
    queryKey: ["cloud-afk-plan", desktop?.id],
    queryFn: () => cloudApi.movePlan(desktop?.id ?? ""),
    enabled: open && accounts.length > 0,
    staleTime: 0,
    gcTime: 0,
    retry: false,
  });

  const target = accounts.find((a) => a.id === chosen);
  const go = (next: Step, dir: 1 | -1) => {
    setDirection(dir);
    setStep(next);
  };

  const move = async () => {
    if (!desktop || !target || !plan?.server) return;
    setBusy(true);
    setFailure(null);
    try {
      const done = await cloudApi.move(desktop.id, target.id, target.rev, run);
      if (done.outcome.kind === "saved") {
        storeAccount(queryClient, done.outcome.account);
        void queryClient.invalidateQueries({ queryKey: ["afk"] });
        void queryClient.invalidateQueries({ queryKey: CLOUD_KEY });
        setResult(done);
        go("done", 1);
      } else if (done.outcome.kind === "conflict") {
        storeAccount(queryClient, done.outcome.account);
        setFailure(t("cloudAfk.move.conflict"));
      } else {
        setFailure(t("cloudAfk.move.invalid"));
      }
    } catch (error) {
      setFailure(isFriendly(error) ? error.message : t("cloudAfk.actions.failed"));
    } finally {
      setBusy(false);
    }
  };

  // Like the AFK page's own button: the site's front page, not straight to checkout (owner, 23 Oct).
  const buy = () =>
    cloudApi
      .open("start")
      .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));

  const empty = accounts.length === 0;
  const footer = empty ? (
    <Button
      variant="primary"
      icon={<ArrowSquareOut size={16} weight="bold" />}
      onClick={() => void buy()}
    >
      {t("cloudAfk.move.buy")}
    </Button>
  ) : step === "done" && result ? (
    <>
      <Button
        variant="ghost"
        icon={<Terminal size={15} />}
        onClick={() => {
          onClose();
          if (target) onOpenConsole(target.id);
        }}
      >
        {t("cloudAfk.move.openConsole")}
      </Button>
      <Button variant="primary" onClick={onClose}>
        {t("cloudAfk.move.close")}
      </Button>
    </>
  ) : step === "pick" ? (
    <>
      <Button variant="ghost" onClick={onClose}>
        {t("cloudAfk.guard.cancel")}
      </Button>
      <Button variant="primary" disabled={!chosen} onClick={() => go("confirm", 1)}>
        {t("cloudAfk.move.action")}
      </Button>
    </>
  ) : (
    <>
      <Button
        variant="ghost"
        onClick={() => (accounts.length > 1 ? go("pick", -1) : onClose())}
        disabled={busy}
      >
        {t("cloudAfk.guard.cancel")}
      </Button>
      <Button
        variant="primary"
        disabled={busy || !plan || plan.server === null}
        onClick={() => void move()}
      >
        {busy ? t("cloudAfk.move.moving") : t("cloudAfk.move.confirm")}
      </Button>
    </>
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && !busy && onClose()}
      title={t("cloudAfk.move.title", { name: desktop?.nickname ?? "" })}
      closeLabel={t("common.close")}
      width={480}
      morph
      dismissible={!busy}
      footer={footer}
    >
      {desktop ? (
        empty ? (
          <EmptyState
            icon={<ContentIcon kind="cloud" />}
            title={t("cloudAfk.move.noAccount")}
            className="py-4"
          />
        ) : (
          <AnimatePresence mode="popLayout" initial={false} custom={direction}>
            <motion.div key={step} {...stepMotion(reduce, direction)}>
              {step === "pick" ? (
                <Pick
                  accounts={accounts}
                  chosen={chosen}
                  onChoose={setChosen}
                  desktop={desktop}
                  skewMs={skewMs}
                />
              ) : step === "confirm" && target ? (
                <Confirm
                  plan={plan}
                  target={target}
                  desktop={desktop}
                  run={run}
                  onRun={setRun}
                  failure={failure}
                />
              ) : result ? (
                <Done result={result} desktop={desktop} />
              ) : null}
            </motion.div>
          </AnimatePresence>
        )
      ) : null}
    </Dialog>
  );
}
