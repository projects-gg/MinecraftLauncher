// The first setup of an AFK+ account, in four short steps (afkplus.md §8.1, afkplus-free.md §7.2):
// where it works (and who makes the trades there) and what it does after a trade, how it talks, what
// it may spend (a buy budget, or "Şimdilik alım yapmasın"; the assistant never tells anyone either),
// and a first order in the player's own words (can be skipped). "Başlat" saves it all and switches
// the assistant on. A morph dialog: the panel glides to each step's size while the
// step slides in from the side it comes from; "Daha sonra" (or the close button) leaves it for later.
import {
  Button,
  ContentIcon,
  cn,
  Dialog,
  easeOutExpo,
  Segmented,
  Select,
  Switch,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed, isFriendly } from "../../errors";
import type { CloudAccount } from "../api";
import type { PlusConfig, PlusPanel } from "./api";
import { AmountField } from "./fields";
import { limitProblems, PLUS_LIMITS, type PlusLimits, PRESETS, personaTextProblem } from "./model";
import { OrderStarter } from "./orders-section";
import { usePlusSave } from "./use-plus";
import { formatAmount } from "./words";

type Step = "mode" | "assistant" | "limits" | "list";
const STEPS: Step[] = ["mode", "assistant", "limits", "list"];

interface Choices {
  returnRow: string | null;
  preset: string;
  intro: string;
  perDealMax: number | null;
  totalBudget: number | null;
  approvalAbove: number | null;
  /** "Şimdilik alım yapmasın": no budget, buy rules only tell. */
  notNow: boolean;
}

function choicesOf(config: PlusConfig): Choices {
  return {
    returnRow: config.trade.returnRow,
    preset: (PRESETS as string[]).includes(config.persona.preset)
      ? config.persona.preset
      : "friendly",
    intro: config.persona.intro,
    perDealMax: config.limits.perDealMax,
    totalBudget: config.limits.totalBudget,
    approvalAbove: config.limits.approvalAbove,
    notNow: false,
  };
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

function StepHead({ title, lede }: { title: string; lede: string }) {
  return (
    <header className="mb-4">
      {/* Under the dialog's own Tektur title: the step's question in the text face, a step down. */}
      <h3 className="text-[15px] leading-tight font-semibold text-bone-50">{title}</h3>
      <p className="mt-1 max-w-[52ch] text-[13px] text-pretty text-ash-300">{lede}</p>
    </header>
  );
}

function ModeStep({
  account,
  panel,
  choices,
  onChange,
}: {
  account: CloudAccount;
  panel: PlusPanel;
  choices: Choices;
  onChange: (parts: Partial<Choices>) => void;
}) {
  const { t, i18n } = useTranslation();
  const server = panel.settings.server;
  const name = server.name ? (i18n.language === "en" ? server.name.en : server.name.tr) : null;
  const rows = account.config.commands.filter((command) => !command.secret && command.text);
  return (
    <div>
      {/* The mode is the server's (not a pick here): the step says where, it does not ask. */}
      <StepHead title={t("afkPlus.setup.mode.title")} lede={t("afkPlus.setup.mode.lede")} />
      <div className="flex items-center gap-3 rounded-[var(--radius-panel)] bg-white/[0.04] px-3.5 py-3">
        <span className="chamfer grid size-[40px] shrink-0 place-items-center bg-obsidian-800 [--cut:6px]">
          <ContentIcon kind="world" size={16} />
        </span>
        <div className="min-w-0">
          <p className="text-[12px] text-ash-400">{t("afkPlus.setup.mode.mode")}</p>
          <p className="truncate text-[14px] font-medium text-bone-50">
            {name ?? t("afkPlus.setup.mode.generic")}
          </p>
        </div>
      </div>
      {/* Where the bot does not trade itself (or AFK+ does not run), it is said once, here. */}
      {!server.on ? (
        <p className="mt-2 text-[12.5px] text-gold-200">{t("afkPlus.server.off")}</p>
      ) : server.trade === "owner" ? (
        <p className="mt-2 text-[12.5px] text-ash-300">{t("afkPlus.server.owner")}</p>
      ) : null}
      <div className="mt-4 flex flex-col gap-1.5">
        <span className="text-[13px] font-medium text-bone-100">
          {t("afkPlus.settings.returnRow")}
        </span>
        <Select
          label={t("afkPlus.settings.returnRow")}
          value={
            rows.some((row) => row.id === choices.returnRow)
              ? (choices.returnRow ?? "stay")
              : "stay"
          }
          onValueChange={(value) => onChange({ returnRow: value === "stay" ? null : value })}
          options={[
            { value: "stay", label: t("afkPlus.settings.returnStay") },
            ...rows.map((row) => ({ value: row.id, label: row.text ?? row.id })),
          ]}
          className="max-w-[340px]"
        />
        <p className="text-[12px] text-ash-400">{t("afkPlus.settings.returnHelp")}</p>
      </div>
    </div>
  );
}

function AssistantStep({
  choices,
  onChange,
  showProblems,
}: {
  choices: Choices;
  onChange: (parts: Partial<Choices>) => void;
  showProblems: boolean;
}) {
  const { t } = useTranslation();
  const problem = personaTextProblem(choices.intro, PLUS_LIMITS.introMax);
  return (
    <div>
      <StepHead
        title={t("afkPlus.setup.assistant.title")}
        lede={t("afkPlus.setup.assistant.lede")}
      />
      <div className="flex flex-col gap-1.5">
        <span className="text-[13px] font-medium text-bone-100">
          {t("afkPlus.settings.persona")}
        </span>
        <Segmented
          label={t("afkPlus.settings.persona")}
          value={choices.preset}
          onValueChange={(preset) => onChange({ preset })}
          options={PRESETS.map((value) => ({
            value,
            label: t(`afkPlus.settings.presets.${value}`),
          }))}
          className="self-start"
        />
        {/* As it would write it with the new account's defaults: no tag (27 Oct 2026). */}
        <p className="mt-1.5 self-start rounded-[10px] bg-obsidian-700/70 px-3 py-1.5 text-[13.5px] text-bone-50 ring-1 ring-white/8 ring-inset">
          {t(`afkPlus.settings.samples.${choices.preset}`)}
        </p>
      </div>
      <div className="mt-4 flex flex-col gap-1.5">
        <label htmlFor="afk-plus-setup-intro" className="text-[13px] font-medium text-bone-100">
          {t("afkPlus.settings.intro")}
        </label>
        <input
          id="afk-plus-setup-intro"
          value={choices.intro}
          maxLength={PLUS_LIMITS.introMax * 2}
          autoComplete="off"
          spellCheck={false}
          placeholder={t("afkPlus.settings.introPlaceholder")}
          aria-invalid={showProblems && problem ? true : undefined}
          onChange={(event) => onChange({ intro: event.target.value })}
          className={cn(
            "h-[34px] w-full rounded-md border border-white/10 bg-obsidian-800/80 px-3 text-[13.5px] text-bone-50 transition-colors placeholder:text-ash-500 hover:border-white/18 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-400/25 focus:outline-none",
            showProblems && problem && "border-flare-400/70",
          )}
        />
        <p
          className={cn("text-[12px]", showProblems && problem ? "text-flare-300" : "text-ash-400")}
        >
          {showProblems && problem
            ? t(`afkPlus.settings.text.${problem}`, { max: PLUS_LIMITS.introMax })
            : t("afkPlus.settings.introHelp")}
        </p>
      </div>
    </div>
  );
}

function LimitsStep({
  choices,
  onChange,
  showProblems,
  maxDeal,
}: {
  choices: Choices;
  onChange: (parts: Partial<Choices>) => void;
  showProblems: boolean;
  maxDeal: number;
}) {
  const { t } = useTranslation();
  const lineId = useId();
  const problems = limitsProblems(choices, maxDeal);
  // "A limit or no buying for now" is one choice: its line is said once, right under the fields
  // that miss an amount (both edges flare), never under a field that has one.
  const required =
    showProblems &&
    !choices.notNow &&
    (problems.perDealMax === "required" || problems.totalBudget === "required");
  const missing = {
    perDealMax: required && choices.perDealMax === null,
    totalBudget: required && choices.totalBudget === null,
  };
  const both = missing.perDealMax && missing.totalBudget;
  // Both missing: one line that names both, so it never reads as the left field's alone.
  const requiredLine = t("afkPlus.setup.limits.required");
  const error = (key: "perDealMax" | "totalBudget" | "approvalAbove") => {
    if (!showProblems || choices.notNow) return null;
    if (key !== "approvalAbove" && missing[key] && !both) {
      return t("afkPlus.setup.limits.requiredOne");
    }
    const problem = problems[key];
    if (!problem || problem === "required") return null;
    return t(`afkPlus.settings.limitErrors.${problem}`, { max: formatAmount(maxDeal) });
  };
  return (
    <div>
      <StepHead title={t("afkPlus.setup.limits.title")} lede={t("afkPlus.setup.limits.lede")} />
      <fieldset
        disabled={choices.notNow}
        className={cn(
          "m-0 grid min-w-0 grid-cols-2 gap-x-4 gap-y-1 border-0 p-0 transition-opacity duration-150",
          choices.notNow && "opacity-55",
        )}
      >
        <AmountField
          label={t("afkPlus.settings.perDealMax")}
          value={choices.perDealMax}
          optional
          error={error("perDealMax")}
          invalid={missing.perDealMax}
          lineless={both}
          describedBy={both ? lineId : undefined}
          onChange={(value) => value !== undefined && onChange({ perDealMax: value })}
        />
        <AmountField
          label={t("afkPlus.settings.totalBudget")}
          value={choices.totalBudget}
          optional
          error={error("totalBudget")}
          invalid={missing.totalBudget}
          lineless={both}
          describedBy={both ? lineId : undefined}
          onChange={(value) => value !== undefined && onChange({ totalBudget: value })}
        />
        {both ? (
          // Where the two fields' own lines were (5.25 + 17 px), so nothing below moves.
          <p
            id={lineId}
            role="alert"
            className="col-span-2 mt-[1.75px] min-h-[17px] text-[12px] leading-[17px] text-flare-400"
          >
            {requiredLine}
          </p>
        ) : null}
        <AmountField
          label={t("afkPlus.settings.approvalAbove")}
          value={choices.approvalAbove}
          optional
          placeholder={t("afkPlus.settings.noLimitShort")}
          error={error("approvalAbove")}
          onChange={(value) => value !== undefined && onChange({ approvalAbove: value })}
        />
      </fieldset>
      <div className="mt-3 border-t border-white/7 pt-3">
        <Switch
          label={t("afkPlus.setup.limits.notNow")}
          description={t("afkPlus.setup.limits.notNowHelp")}
          checked={choices.notNow}
          onCheckedChange={(notNow) => onChange({ notNow })}
        />
      </div>
      <p className="mt-3 text-[12.5px] text-ash-300">{t("afkPlus.setup.limits.secret")}</p>
    </div>
  );
}

type LimitProblem = "required" | "approvalAbove" | "maxDeal" | "amount";

function limitsProblems(
  choices: Choices,
  maxDeal: number,
): Partial<Record<"perDealMax" | "totalBudget" | "approvalAbove", LimitProblem>> {
  if (choices.notNow) return {};
  const found: Partial<Record<"perDealMax" | "totalBudget" | "approvalAbove", LimitProblem>> = {};
  if (choices.perDealMax === null) found.perDealMax = "required";
  if (choices.totalBudget === null) found.totalBudget = "required";
  const limits = {
    perDealMax: choices.perDealMax,
    dailySpendMax: null,
    totalBudget: choices.totalBudget,
    balanceFloor: 0,
    approvalAbove: choices.approvalAbove,
    newCounterpartApprovalAbove: null,
    approvalTimeoutMin: 10,
  } satisfies PlusLimits;
  const general = limitProblems(limits, maxDeal);
  for (const key of ["perDealMax", "totalBudget", "approvalAbove"] as const) {
    const problem = general[key];
    if (problem && !found[key]) found[key] = problem === "timeout" ? "amount" : problem;
  }
  return found;
}

export function PlusSetupWizard({
  account,
  panel,
  open,
  onLater,
  onDone,
}: {
  account: CloudAccount;
  panel: PlusPanel | undefined;
  open: boolean;
  onLater: () => void;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const save = usePlusSave(account.id);
  const [step, setStep] = useState<Step>("mode");
  const [direction, setDirection] = useState<1 | -1>(1);
  const [choices, setChoices] = useState<Choices | null>(null);
  const [tried, setTried] = useState<Step | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [added, setAdded] = useState(false);
  const current = choices ?? (panel ? choicesOf(panel.settings.config) : null);
  const maxDeal = panel?.settings.server.money.maxDeal ?? PLUS_LIMITS.maxDeal;
  const index = STEPS.indexOf(step);

  const change = (parts: Partial<Choices>) => {
    if (!current) return;
    setChoices({ ...current, ...parts });
    setFailure(null);
  };
  const go = (to: Step) => {
    setDirection(STEPS.indexOf(to) > index ? 1 : -1);
    setStep(to);
    setTried(null);
  };
  const blocked = (at: Step): boolean => {
    if (!current) return true;
    if (at === "assistant") return personaTextProblem(current.intro, PLUS_LIMITS.introMax) !== null;
    if (at === "limits") return Object.keys(limitsProblems(current, maxDeal)).length > 0;
    return false;
  };
  const next = () => {
    if (blocked(step)) {
      setTried(step);
      return;
    }
    const to = STEPS[index + 1];
    if (to) go(to);
  };

  const finish = async () => {
    if (!current || busy) return;
    setBusy(true);
    setFailure(null);
    const picked = current;
    try {
      const outcome = await save((latest) => ({
        ...latest,
        enabled: true,
        persona: { ...latest.persona, preset: picked.preset, intro: picked.intro.trim() },
        // The money's name stays as it is ("Para birimi" is in the settings only).
        trade: { ...latest.trade, returnRow: picked.returnRow },
        limits: {
          ...latest.limits,
          perDealMax: picked.notNow ? null : picked.perDealMax,
          totalBudget: picked.notNow ? null : picked.totalBudget,
          approvalAbove: picked.notNow ? latest.limits.approvalAbove : picked.approvalAbove,
        },
      }));
      if (outcome.kind === "invalid") {
        setFailure(t("afkPlus.setup.invalid"));
        return;
      }
      toast.success(t("afkPlus.setup.done"));
      setChoices(null);
      setStep("mode");
      setAdded(false);
      onDone();
    } catch (error) {
      if (isFriendly(error)) setFailure(error.message);
      else afkFailed(error, t("afkPlus.setup.failed"));
    } finally {
      setBusy(false);
    }
  };

  const last = step === "list";
  const footer = (
    <>
      <span className="mr-auto text-[12.5px] text-ash-400 tabular-nums">
        {t("afkPlus.setup.step", { step: index + 1, total: STEPS.length })}
      </span>
      {index === 0 ? (
        <Button variant="ghost" onClick={onLater} disabled={busy}>
          {t("afkPlus.setup.later")}
        </Button>
      ) : (
        <Button variant="ghost" onClick={() => go(STEPS[index - 1] ?? "mode")} disabled={busy}>
          {t("afkPlus.setup.back")}
        </Button>
      )}
      {last ? (
        <Button variant="primary" onClick={() => void finish()} disabled={busy}>
          {busy ? t("afkPlus.setup.finishing") : t("afkPlus.setup.finish")}
        </Button>
      ) : (
        <Button variant="primary" onClick={next}>
          {t("afkPlus.setup.next")}
        </Button>
      )}
    </>
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => !nextOpen && !busy && onLater()}
      title={t("afkPlus.setup.title")}
      closeLabel={t("afkPlus.setup.later")}
      width={540}
      morph
      dismissible={!busy}
      dismissOnOutsideClick={false}
      footer={footer}
    >
      {current && panel ? (
        <AnimatePresence mode="popLayout" initial={false} custom={direction}>
          <motion.div key={step} {...stepMotion(reduce, direction)}>
            {step === "mode" ? (
              <ModeStep account={account} panel={panel} choices={current} onChange={change} />
            ) : step === "assistant" ? (
              <AssistantStep
                choices={current}
                onChange={change}
                showProblems={tried === "assistant"}
              />
            ) : step === "limits" ? (
              <LimitsStep
                choices={current}
                onChange={change}
                showProblems={tried === "limits"}
                maxDeal={maxDeal}
              />
            ) : (
              <div>
                <StepHead
                  title={t("afkPlus.setup.list.title")}
                  lede={t("afkPlus.setup.list.lede")}
                />
                <OrderStarter
                  account={account}
                  panel={panel}
                  // The limits of step 3 are saved with "Başlat": the first order is asked against
                  // them, not the stored ones.
                  limits={
                    current
                      ? {
                          perDealMax: current.notNow ? null : current.perDealMax,
                          totalBudget: current.notNow ? null : current.totalBudget,
                        }
                      : undefined
                  }
                  onSaved={() => setAdded(true)}
                />
                {added ? (
                  <p role="status" className="mt-2 text-[12.5px] text-jade-400">
                    {t("afkPlus.setup.list.added")}
                  </p>
                ) : null}
              </div>
            )}
            {failure ? (
              <p role="alert" className="mt-3 text-[12.5px] text-flare-300">
                {failure}
              </p>
            ) : null}
          </motion.div>
        </AnimatePresence>
      ) : null}
    </Dialog>
  );
}
