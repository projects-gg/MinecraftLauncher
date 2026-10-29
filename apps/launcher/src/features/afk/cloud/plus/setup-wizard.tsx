// The first setup of an AFK+ account (SETUP-UX-L): the site's five questions in the site's order, one
// a step, in the launcher's morph Dialog (setup/setup-dialog.tsx): what it buys and sells (an order
// in the player's own words, saved in the step), how much it may spend (two amounts, or "Şimdilik
// alım yapmasın"; asked while there is a buy, on by itself while there is no order, skipped while
// every order is a sale), what it does (the three authorities, the approval limits under the one
// they work in, and where it goes after a trade), what it does in chat (ads, off by default; the
// minutes only to a service that plans them), and how it talks (the tone, as the game shows a
// private message; the introduction under "Gelişmiş"). "Başlat" saves it all as one write and
// switches ProBot AI on. "Daha sonra", ✕ and Esc keep the choices for the session (`usePlusUi`).
// The hard rules hold: buying needs both limits or "Şimdilik alım yapmasın"; the budget is never
// said; an approval limit above the per-deal limit waits.
import { cn, Segmented, Select, toast } from "@pl/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed, isFriendly } from "../../errors";
import { ToggleRow } from "../../settings/controls";
import type { CloudAccount } from "../api";
import {
  AD_EVERY_DEFAULT,
  AUTHORITY_ITEMS,
  adsWrite,
  authorityOf,
  moneyStep,
  PLUS_ITEMS,
  PLUS_STEPS,
  type PlusStep,
  plusAnswers,
} from "../setup/answers";
import { Advanced, Labeled } from "../setup/question-list";
import { type FrameStep, SetupDialog } from "../setup/setup-dialog";
import { serverLabel } from "../status";
import type { PlusConfig, PlusPanel } from "./api";
import { ChatAds } from "./chat-ads";
import { AmountField } from "./fields";
import { itemLabel, type Namer, usePlusItemData } from "./items";
import {
  limitProblems,
  PLUS_LIMITS,
  type PlusLimits,
  PRESETS,
  personaTextProblem,
  presetOf,
} from "./model";
import { OrderStarter } from "./orders-section";
import { AuthorityChoice, MoneyFits, moneyNameOf, OrderLines, TalkPreview } from "./setup-parts";
import { type PlusSetupChoices, usePlusSave, usePlusUi } from "./use-plus";
import { currentLang, formatAmount } from "./words";

function choicesOf(config: PlusConfig): PlusSetupChoices {
  return {
    step: "orders",
    perDealMax: config.limits.perDealMax,
    totalBudget: config.limits.totalBudget,
    notNow: null,
    authority: authorityOf(config.authority),
    approvalAbove: config.limits.approvalAbove,
    newCounterpartApprovalAbove: config.limits.newCounterpartApprovalAbove,
    returnRow: config.trade.returnRow,
    ads: config.discovery.ads,
    adEveryMin: config.discovery.adEveryMin ?? null,
    writeSellers: config.discovery.scanChat && config.discovery.proactivePm,
    preset: presetOf(config.persona.preset),
    playerTalk: config.persona.playerTalk,
    intro: config.persona.intro,
  };
}

/** The settings as the setup would leave them: what "Başlat" writes (and the rail's answers). */
function applied(
  config: PlusConfig,
  choices: PlusSetupChoices,
  { notNow, skipMoney, intervals }: { notNow: boolean; skipMoney: boolean; intervals: boolean },
): PlusConfig {
  const spend = (value: number | null, stored: number | null) =>
    skipMoney ? stored : notNow ? null : value;
  return {
    ...config,
    authority: choices.authority,
    persona: {
      ...config.persona,
      preset: choices.preset,
      playerTalk: choices.playerTalk,
      intro: choices.intro.trim(),
    },
    trade: { ...config.trade, returnRow: choices.returnRow },
    limits: {
      ...config.limits,
      perDealMax: spend(choices.perDealMax, config.limits.perDealMax),
      totalBudget: spend(choices.totalBudget, config.limits.totalBudget),
      approvalAbove: choices.approvalAbove,
      newCounterpartApprovalAbove: choices.newCounterpartApprovalAbove,
    },
    discovery: {
      ...adsWrite(config.discovery, choices.ads, choices.adEveryMin, intervals),
      // On: it scans the chat and writes to a seller once; off keeps the scan as it was.
      scanChat: choices.writeSellers || config.discovery.scanChat,
      proactivePm: choices.writeSellers,
    },
  };
}

type Field = "perDealMax" | "totalBudget" | "approvalAbove" | "newCounterpartApprovalAbove";

/** The limits' problems as the setup asks them: both spending amounts, or none while not buying. */
function problemsOf(
  limits: PlusLimits,
  { notNow, skipMoney, maxDeal }: { notNow: boolean; skipMoney: boolean; maxDeal: number },
): Partial<Record<Field, "required" | "approvalAbove" | "maxDeal" | "amount">> {
  const found: Partial<Record<Field, "required" | "approvalAbove" | "maxDeal" | "amount">> = {};
  if (!notNow && !skipMoney) {
    if (limits.perDealMax === null) found.perDealMax = "required";
    if (limits.totalBudget === null) found.totalBudget = "required";
  }
  const general = limitProblems(limits, maxDeal);
  for (const key of [
    "perDealMax",
    "totalBudget",
    "approvalAbove",
    "newCounterpartApprovalAbove",
  ] as const) {
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
  const lang = currentLang();
  const save = usePlusSave(account.id);
  const kept = usePlusUi((s) => s.setups[account.id] ?? null);
  const setSetup = usePlusUi((s) => s.setSetup);
  const [direction, setDirection] = useState<1 | -1>(1);
  const [tried, setTried] = useState<PlusStep | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const items = usePlusItemData(panel?.settings.server.mc ?? null);
  if (!panel) return null;

  const settings = panel.settings;
  const config = settings.config;
  const server = settings.server;
  const choices = kept ?? choicesOf(config);
  const money = moneyStep(config.orders);
  const skipMoney = money === "skip";
  const notNow = choices.notNow ?? money === "notNow";
  const plan = settings.adPreview ?? null;
  const intervals = plan !== null;
  const maxDeal = server.money.maxDeal || PLUS_LIMITS.maxDeal;
  const virtual = applied(config, choices, { notNow, skipMoney, intervals });
  const problems = problemsOf(virtual.limits, { notNow, skipMoney, maxDeal });
  const introProblem = personaTextProblem(choices.intro, PLUS_LIMITS.introMax);
  const namer: Namer = {
    data: items.data,
    custom: config.customItems,
    unnamed: t("afkPlus.items.unnamed"),
  };
  const moneyName = moneyNameOf(server, lang);
  const answers = plusAnswers(
    virtual,
    {
      lang,
      money: moneyName,
      itemName: (ref) => itemLabel(ref, namer, lang),
      breakUntil: null,
      schedule: false,
      saver: false,
      // The minutes as picked here, never under the server's floor (the service's own rule).
      preview: plan
        ? {
            ...plan,
            everyMin: Math.max(choices.adEveryMin ?? AD_EVERY_DEFAULT, plan.floorMin),
            stoppedToday: false,
          }
        : null,
      notify: null,
    },
    t,
  );

  // The money step left behind while it was skipped (every order became a sale) moves on.
  const step: PlusStep = choices.step === "money" && skipMoney ? "act" : choices.step;
  const index = PLUS_STEPS.indexOf(step);
  const frameSteps: FrameStep[] = PLUS_STEPS.map((id) => ({
    id,
    question: t(`setup.plus.questions.${id}`),
    item: id === "act" ? AUTHORITY_ITEMS[choices.authority] : PLUS_ITEMS[id],
    answer: id === "money" && skipMoney ? t("setup.plus.sellOnly") : answers[id],
    skipped: id === "money" && skipMoney,
  }));

  const change = (parts: Partial<PlusSetupChoices>) => {
    setSetup(account.id, { ...choices, ...parts });
    setFailure(null);
  };
  const go = (to: PlusStep) => {
    const toIndex = PLUS_STEPS.indexOf(to);
    setDirection(toIndex > index ? 1 : -1);
    setTried(null);
    change({ step: to });
  };
  const neighbour = (from: number, by: 1 | -1): PlusStep | null => {
    let at = from + by;
    while (PLUS_STEPS[at] === "money" && skipMoney) at += by;
    return PLUS_STEPS[at] ?? null;
  };
  const blocked = (at: PlusStep): boolean => {
    if (at === "money") return Boolean(problems.perDealMax || problems.totalBudget);
    if (at === "act") {
      return (
        choices.authority === "deal" &&
        Boolean(problems.approvalAbove || problems.newCounterpartApprovalAbove)
      );
    }
    if (at === "talk") return introProblem !== null;
    return false;
  };
  // The choices wait as they are, the step included (each change is kept as it is made); the panel
  // stays usable.
  const later = () => {
    setTried(null);
    onLater();
  };

  const finish = async () => {
    if (busy) return;
    const stuck = PLUS_STEPS.find((at) => blocked(at));
    if (stuck) {
      go(stuck);
      setTried(stuck);
      return;
    }
    setBusy(true);
    setFailure(null);
    const picked = choices;
    try {
      const outcome = await save((latest) => ({
        ...applied(latest, picked, {
          notNow,
          skipMoney: moneyStep(latest.orders) === "skip",
          intervals,
        }),
        enabled: true,
      }));
      if (outcome.kind === "invalid") {
        setFailure(t("afkPlus.setup.invalid"));
        return;
      }
      toast.success(t("setup.plus.started"));
      setSetup(account.id, null);
      onDone();
    } catch (error) {
      if (isFriendly(error)) setFailure(error.message);
      else afkFailed(error, t("afkPlus.setup.failed"));
    } finally {
      setBusy(false);
    }
  };

  const next = () => {
    if (blocked(step)) {
      setTried(step);
      return;
    }
    const to = neighbour(index, 1);
    if (to) go(to);
    else void finish();
  };
  const back = neighbour(index, -1);
  const last = index === PLUS_STEPS.length - 1;
  const showProblems = tried === step;
  const name = server.name ? (lang === "en" ? server.name.en : server.name.tr) : null;
  const where = !server.on
    ? t("afkPlus.server.off")
    : server.trade === "owner"
      ? t("afkPlus.server.owner")
      : t("setup.plus.where", { server: name || serverLabel(account.server) });

  const amountError = (field: Field): string | null => {
    const problem = problems[field];
    if (!problem || problem === "required") return null;
    // The approval limit says its own problem as it is typed; the rest after "Devam".
    if (!showProblems && problem !== "approvalAbove") return null;
    return t(`afkPlus.settings.limitErrors.${problem}`, { max: formatAmount(maxDeal, lang) });
  };
  // "A limit or no buying for now" is one choice: its line is said once, under both fields.
  const required =
    showProblems &&
    step === "money" &&
    !notNow &&
    (problems.perDealMax === "required" || problems.totalBudget === "required");
  const prefix = config.persona.prefix;
  const tag = prefix.on && prefix.text.trim() ? prefix.text.trim() : null;

  const body =
    step === "orders" ? (
      <div className="flex flex-col gap-4">
        <OrderStarter account={account} panel={panel} />
        <OrderLines orders={config.orders} namer={namer} server={server} lang={lang} />
      </div>
    ) : step === "money" ? (
      <div className="flex flex-col">
        {/* Quiet while "Şimdilik alım yapmasın" is on, never locked: typing an amount turns it off
            (the site's rule; one choice, either amounts or no buying). */}
        <div
          className={cn(
            "grid min-w-0 grid-cols-2 gap-x-4 transition-opacity duration-150",
            notNow && "opacity-55",
          )}
        >
          <AmountField
            label={t("afkPlus.settings.perDealMax")}
            value={choices.perDealMax}
            optional
            unit={moneyName}
            invalid={required && choices.perDealMax === null}
            error={amountError("perDealMax")}
            onChange={(value) =>
              value !== undefined &&
              change(value === null ? { perDealMax: null } : { perDealMax: value, notNow: false })
            }
          />
          <AmountField
            label={t("afkPlus.settings.totalBudget")}
            value={choices.totalBudget}
            optional
            unit={moneyName}
            invalid={required && choices.totalBudget === null}
            error={amountError("totalBudget")}
            onChange={(value) =>
              value !== undefined &&
              change(value === null ? { totalBudget: null } : { totalBudget: value, notNow: false })
            }
          />
        </div>
        {required ? (
          <p role="alert" className="mt-1 text-[12px] text-flare-400">
            {t("setup.plus.required")}
          </p>
        ) : null}
        <ToggleRow
          title={t("setup.plus.notNow")}
          help={t("setup.plus.notNowHelp")}
          checked={notNow}
          onCheckedChange={(on) => change({ notNow: on })}
        />
        {notNow ? null : (
          <div className="pt-3">
            <MoneyFits
              limits={virtual.limits}
              orders={config.orders}
              namer={namer}
              lang={lang}
              money={moneyName}
            />
          </div>
        )}
        <p className="pt-3 text-[12.5px] text-ash-400">{t("afkPlus.settings.secret")}</p>
      </div>
    ) : step === "act" ? (
      <div className="flex flex-col">
        <AuthorityChoice value={choices.authority} onChange={(authority) => change({ authority })}>
          {choices.authority === "deal" ? (
            <div className="grid grid-cols-2 gap-x-4 pt-2 pl-3">
              <AmountField
                label={t("afkPlus.settings.approvalAbove")}
                value={choices.approvalAbove}
                optional
                unit={moneyName}
                placeholder={t("afkPlus.settings.noAskShort")}
                error={amountError("approvalAbove")}
                extra={
                  <p className="-mt-1 text-[12px] text-ash-400">
                    {t("afkPlus.settings.approvalHelp")}
                  </p>
                }
                onChange={(value) => value !== undefined && change({ approvalAbove: value })}
              />
              <AmountField
                label={t("afkPlus.settings.newCounterpart")}
                value={choices.newCounterpartApprovalAbove}
                optional
                unit={moneyName}
                placeholder={t("afkPlus.settings.noAskShort")}
                error={amountError("newCounterpartApprovalAbove")}
                extra={
                  <p className="-mt-1 text-[12px] text-ash-400">
                    {t("afkPlus.settings.newCounterpartHelp")}
                  </p>
                }
                onChange={(value) =>
                  value !== undefined && change({ newCounterpartApprovalAbove: value })
                }
              />
            </div>
          ) : null}
        </AuthorityChoice>
        <ReturnRowField
          account={account}
          value={choices.returnRow}
          onChange={(returnRow) => change({ returnRow })}
        />
      </div>
    ) : step === "chat" ? (
      <div className="flex flex-col">
        <ToggleRow
          lead
          title={t("afkPlus.settings.ads")}
          checked={choices.ads}
          disabled={!server.ads.length}
          onCheckedChange={(ads) => change({ ads })}
        />
        {choices.ads && server.ads.length ? (
          <>
            <ChatAds
              plan={plan}
              minutes={choices.adEveryMin}
              name={account.name}
              onMinutes={(adEveryMin) => change({ adEveryMin })}
            />
            {plan ? (
              <p className="pt-1 text-[12.5px] text-pretty text-ash-400">
                {t("setup.plus.ads.note")}
              </p>
            ) : null}
          </>
        ) : null}
        {/* On by default, said rather than hidden: it scans the chat and writes once to a seller.
            Its own block (a line and room above), never read as an option of the ads. */}
        <div className="mt-3">
          <ToggleRow
            title={t("setup.plus.writeSellers")}
            checked={choices.writeSellers}
            onCheckedChange={(writeSellers) => change({ writeSellers })}
          />
        </div>
      </div>
    ) : (
      <div className="flex flex-col">
        <Labeled label={t("afkPlus.settings.persona")}>
          <Segmented
            label={t("afkPlus.settings.persona")}
            value={presetOf(choices.preset)}
            onValueChange={(preset) => change({ preset })}
            options={PRESETS.map((value) => ({
              value,
              label: t(`afkPlus.settings.presets.${value}`),
            }))}
            className="self-start"
          />
        </Labeled>
        <ToggleRow
          title={t("afkPlus.settings.playerTalk")}
          help={t("afkPlus.settings.playerTalkHelp")}
          checked={choices.playerTalk}
          onCheckedChange={(playerTalk) => change({ playerTalk })}
        />
        <div className="pt-3">
          <TalkPreview
            preset={presetOf(choices.preset)}
            playerTalk={choices.playerTalk}
            tag={tag}
            name={account.name}
            orders={config.orders}
            perDealMax={virtual.limits.perDealMax}
            namer={namer}
            lang={lang}
          />
        </div>
        <Advanced
          summary={t("afkPlus.settings.intro")}
          open={showProblems && introProblem !== null}
        >
          <IntroField
            value={choices.intro}
            problem={showProblems || introProblem === "length" ? introProblem : null}
            onChange={(intro) => change({ intro })}
          />
        </Advanced>
      </div>
    );

  return (
    <SetupDialog
      open={open}
      title={t("setup.plus.title")}
      where={where}
      steps={frameSteps}
      current={index}
      direction={direction}
      onGo={(to) => {
        const target = PLUS_STEPS[to];
        if (target) go(target);
      }}
      onLater={later}
      railFoot={t("setup.plus.railFoot")}
      lede={step === "act" ? null : t(`setup.plus.ledes.${step}`)}
      onBack={back ? () => go(back) : undefined}
      primary={{
        label: last ? (busy ? t("setup.plus.starting") : t("setup.plus.start")) : t("setup.next"),
        onClick: next,
      }}
      note={last ? t("setup.plus.changeLater") : null}
      busy={busy}
      failure={failure}
    >
      {body}
    </SetupDialog>
  );
}

/** "Takastan sonra": a plain command row of the account to run after a trade, or stay at spawn. */
function ReturnRowField({
  account,
  value,
  onChange,
}: {
  account: CloudAccount;
  value: string | null;
  onChange: (value: string | null) => void;
}) {
  const { t } = useTranslation();
  const rows = account.config.commands.filter((command) => !command.secret && command.text);
  return (
    <Labeled
      label={t("afkPlus.settings.returnRow")}
      help={t("afkPlus.settings.returnHelp")}
      className="pt-4"
    >
      <Select
        label={t("afkPlus.settings.returnRow")}
        value={rows.some((row) => row.id === value) ? (value ?? "stay") : "stay"}
        onValueChange={(next) => onChange(next === "stay" ? null : next)}
        options={[
          { value: "stay", label: t("afkPlus.settings.returnStay") },
          ...rows.map((row) => ({ value: row.id, label: row.text ?? row.id })),
        ]}
        className="max-w-[340px]"
      />
    </Labeled>
  );
}

/** "Tanıtım": how it introduces itself in a first message (checked as typed for its length). */
function IntroField({
  value,
  problem,
  onChange,
}: {
  value: string;
  problem: ReturnType<typeof personaTextProblem>;
  onChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-1.5 py-2.5">
      <label htmlFor="afk-plus-setup-intro" className="text-[13px] font-medium text-bone-100">
        {t("afkPlus.settings.intro")}
      </label>
      <input
        id="afk-plus-setup-intro"
        value={value}
        maxLength={PLUS_LIMITS.introMax * 2}
        autoComplete="off"
        spellCheck={false}
        placeholder={t("afkPlus.settings.introPlaceholder")}
        aria-invalid={problem ? true : undefined}
        onChange={(event) => onChange(event.target.value)}
        className={
          problem
            ? "h-[34px] w-full rounded-md border border-flare-400/70 bg-obsidian-800/80 px-3 text-[13.5px] text-bone-50 placeholder:text-ash-500 focus:border-flare-400 focus:ring-2 focus:ring-flare-400/25 focus:outline-none"
            : "h-[34px] w-full rounded-md border border-white/10 bg-obsidian-800/80 px-3 text-[13.5px] text-bone-50 transition-colors placeholder:text-ash-500 hover:border-white/18 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-400/25 focus:outline-none"
        }
      />
      <p className={problem ? "text-[12px] text-flare-300" : "text-[12px] text-ash-400"}>
        {problem
          ? t(`afkPlus.settings.text.${problem}`, { max: PLUS_LIMITS.introMax })
          : t("afkPlus.settings.introHelp")}
      </p>
    </div>
  );
}
