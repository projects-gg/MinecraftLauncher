// "Ayarlar" as questions (SETUP-UX-L, the site's AFK+ settings): one quiet panel of rows, each a
// question with its item, its answer in plain words and "Değiştir"; a row opens in place into its
// fields, the rarely needed ones folded under its own "Gelişmiş". The site's questions in the site's
// order: Ne kadar harcayabilsin? (with what was spent), ProBot AI ne yapsın?, Sohbette ne yapsın?,
// Nasıl konuşsun?, Kimlere cevap versin? (the blocked players too), Ne zaman çalışsın? (while Studio
// offers the hours), Diğer ayarlar, and the launcher's own last row, Bildirimler.
// The form is one draft (design.md G6-P): edits wait across questions, sections and the panel
// closing; the docked change bar saves or drops them, Ctrl+S saves; a question holding unsaved edits
// carries the gold dot, one with a field to fix the flare mark. Saving puts only the player's own
// edits on top of the latest settings, so a rule added meanwhile or a change made on the site stays.
// A break, a blocked player, "Harcananı sıfırla" and the notices take effect at once. Amounts read
// the players' shorthands and say the number in words. "Sınırlara git" opens the question that
// holds the limit, its field in view and focused.
import { ClockCounterClockwise, Plus, X } from "@phosphor-icons/react";
import {
  Button,
  cn,
  Dialog,
  easeOutExpo,
  enter,
  IconButton,
  leave,
  Segmented,
  Select,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed, isFriendly } from "../../errors";
import { Hint, inputClass, ToggleRow } from "../../settings/controls";
import type { CloudAccount } from "../api";
import {
  AUTHORITY_ITEMS,
  authorityOf,
  limitQuestion,
  NOTIFY_GROUPS,
  PLUS_ITEMS,
  type PlusQuestion,
  plusAnswers,
  withMoney,
} from "../setup/answers";
import { Advanced, Labeled, QuestionList, QuestionRow } from "../setup/question-list";
import { type CloudNotifyPrefs, type PlusPanel, plusApi } from "./api";
import { DOCK_HEIGHT, PlusChangeDock } from "./change-dock";
import { ChatAds } from "./chat-ads";
import { CurrencyField } from "./currency-field";
import { AmountField, IntField, QuietLink, TimeField } from "./fields";
import { HistoryDialog } from "./history-dialog";
import { itemLabel, type Namer, usePlusItemData } from "./items";
import {
  applyDraft,
  blockedNameProblem,
  canSpend,
  currencyEditProblem,
  currencyFieldState,
  type EmptyLimit,
  editDraft,
  emptyLimit,
  isTime,
  type LimitField,
  limitProblems,
  liveProblems,
  type PersonaTextProblem,
  PLUS_LIMITS,
  type PlusConfig,
  type PlusLimits,
  type PlusWindow,
  PREFIX_DEFAULT,
  PREFIX_MAX,
  PRESETS,
  personaTextProblem,
  prefixProblem,
  presetOf,
  type SettingsPart,
  STATUS_PRESETS,
  sortedDays,
  WEEKDAYS,
  windowProblems,
  withCurrency,
  withSettings,
} from "./model";
import { AuthorityChoice, MoneyFits, moneyNameOf, TalkPreview } from "./setup-parts";
import { untilPhrase } from "./time";
import { plusAi } from "./types";
import {
  plusKey,
  usePlusControl,
  usePlusFeatures,
  usePlusInstantSave,
  usePlusSave,
  usePlusUi,
} from "./use-plus";
import { currentLang, formatAmount, type Lang } from "./words";

type Change = (update: (shown: SettingsPart) => SettingsPart) => void;

interface EditorProps {
  shown: SettingsPart;
  set: Change;
  panel: PlusPanel;
  submitted: boolean;
  /** The service's refusals of the last save (`who.friends[2]`, `pay`, …). */
  serverIssues: ReadonlySet<string>;
}

/** One line of text the judge reads (intro, own status), with its count and its verdict. */
function PersonaText({
  label,
  value,
  max,
  placeholder,
  help,
  judge,
  problem,
  onChange,
}: {
  label: string;
  value: string;
  max: number;
  placeholder: string;
  help?: string;
  judge?: { state: string; reason: string | null } | null;
  problem: PersonaTextProblem | null;
  onChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const length = [...value].length;
  const line = problem
    ? t(`afkPlus.settings.text.${problem}`, { max })
    : judge?.state === "rejected"
      ? t(`afkPlus.settings.judgeRejected.${judge.reason ?? "other"}`, {
          defaultValue: t("afkPlus.settings.judgeRejected.other"),
        })
      : judge?.state === "pending"
        ? t("afkPlus.settings.judgePending")
        : (help ?? null);
  const bad = Boolean(problem) || judge?.state === "rejected";
  return (
    <div className="flex flex-col gap-1.5 py-2.5">
      <label htmlFor={id} className="text-[13px] font-medium text-bone-100">
        {label}
      </label>
      <div className="relative">
        <input
          id={id}
          value={value}
          maxLength={max * 2}
          autoComplete="off"
          spellCheck={false}
          placeholder={placeholder}
          aria-invalid={bad ? true : undefined}
          aria-describedby={`${id}-line`}
          onChange={(event) => onChange(event.target.value)}
          className={cn(
            inputClass,
            "h-[34px] px-3 pr-14",
            bad && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
          )}
        />
        <span
          className={cn(
            "pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-[12px] tabular-nums transition-opacity duration-150",
            length > max * 0.8 ? "opacity-100" : "opacity-0",
            length > max ? "text-flare-300" : "text-ash-400",
          )}
        >
          {length}/{max}
        </span>
      </div>
      <p
        id={`${id}-line`}
        role={problem ? "alert" : undefined}
        className={cn(
          "min-h-[17px] text-[12px] leading-[17px]",
          bad ? "text-flare-300" : "text-ash-400",
        )}
      >
        {line}
      </p>
    </div>
  );
}

/** The tag before ProBot AI's lines (27 Oct 2026): the player's own, under its switch. */
function TagField({
  value,
  line,
  onChange,
}: {
  value: string;
  line: string | null;
  onChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  // One step in under its switch: the field belongs to that row (the AFK sheets' rule).
  return (
    <div className="flex flex-col gap-1.5 pb-2.5 pl-5">
      <input
        id={id}
        value={value}
        maxLength={PREFIX_MAX + 4}
        autoComplete="off"
        spellCheck={false}
        placeholder={PREFIX_DEFAULT.text}
        aria-label={t("afkPlus.settings.prefixLabel")}
        aria-invalid={line ? true : undefined}
        aria-describedby={line ? `${id}-line` : undefined}
        onChange={(event) => onChange(event.target.value)}
        className={cn(
          inputClass,
          "h-[34px] max-w-[220px] px-3",
          line && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
        )}
      />
      {line ? (
        <p id={`${id}-line`} role="alert" className="text-[12px] leading-[17px] text-flare-300">
          {line}
        </p>
      ) : null}
    </div>
  );
}

/** The tag's problem as the form says it, and whether it shows (typed, or after "Kaydet"). */
function tagState(
  shown: SettingsPart,
  panel: PlusPanel,
  submitted: boolean,
  serverIssues: ReadonlySet<string>,
  t: TFunction,
) {
  const tag = shown.persona.prefix;
  const saved = panel.settings.config.persona.prefix;
  const problem = prefixProblem(tag.text, [...shown.who.friends, ...panel.settings.config.blocked]);
  const edited = tag.text !== saved.text || tag.on !== saved.on;
  const line =
    problem && (submitted || (problem !== "empty" && edited))
      ? t(`afkPlus.settings.prefixErrors.${problem}`, { max: PREFIX_MAX })
      : serverIssues.has("persona.prefix.text")
        ? t("afkPlus.settings.prefixErrors.words")
        : null;
  return { tag, saved, problem, line };
}

// --- Ne kadar harcayabilsin? ----------------------------------------------------------------------

/** What an empty limit's box says (`emptyLimit`); buying nothing is said under it instead. */
const EMPTY_SHORT: Record<EmptyLimit, string | null> = {
  buysNothing: null,
  noLimit: "afkPlus.settings.noLimitShort",
  noAsk: "afkPlus.settings.noAskShort",
};

/** A limit's field: its empty meaning in the box, its problem under it, its anchor for "Sınırlara git". */
function LimitAmount({
  field,
  label,
  shown,
  set,
  maxDeal,
  submitted,
  serverIssues,
  extra,
  unit,
}: {
  field: keyof PlusLimits;
  label: string;
  shown: SettingsPart;
  set: Change;
  maxDeal: number;
  submitted: boolean;
  serverIssues: ReadonlySet<string>;
  extra?: ReactNode;
  /** The money's name, quiet inside the field, where the profile names one. */
  unit: string | null;
}) {
  const { t } = useTranslation();
  const problem = limitProblems(shown.limits, maxDeal)[field];
  const refused = serverIssues.has(`limits.${field}`);
  const error =
    (submitted || problem === "approvalAbove" || refused) && (problem || refused)
      ? problem
        ? t(`afkPlus.settings.limitErrors.${problem}`, { max: formatAmount(maxDeal) })
        : t("afkPlus.settings.limitErrors.amount")
      : null;
  // Left empty: no daily cap or no question, said in the box; no buying at all, under it.
  const empty = emptyLimit(field);
  const short = empty ? EMPTY_SHORT[empty] : null;
  return (
    <AmountField
      label={label}
      value={shown.limits[field] as number | null}
      optional={empty !== null}
      placeholder={short ? t(short) : undefined}
      help={empty === "buysNothing" ? t("afkPlus.settings.emptyBuysNothing") : undefined}
      error={error}
      extra={extra}
      unit={unit}
      anchor={`afk-plus-limits-${field}`}
      // `undefined`: the text is not an amount yet; the field says so and the draft keeps the last.
      onChange={(value) =>
        value !== undefined && set((s) => ({ ...s, limits: { ...s.limits, [field]: value } }))
      }
    />
  );
}

/** Under "Ne kadar harcayabilsin?" while it is folded: the total limit and what was spent of it. */
const metersShown = new Set<string>();

function SpentMeter({
  accountId,
  spent,
  total,
  money,
}: {
  accountId: string;
  spent: number;
  total: number;
  /** The money's name after the amount, where the profile names one. */
  money: string | null;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const share = Math.min(1, Math.max(0, spent / total));
  // It fills once, the first time it shows (scaleX, 300 ms); after that it is just there.
  const [grow] = useState(() => layout && !metersShown.has(accountId));
  useEffect(() => {
    metersShown.add(accountId);
  }, [accountId]);
  return (
    <span className="mt-1.5 flex flex-col gap-1">
      <span
        aria-hidden
        className="block h-[3px] w-full max-w-[320px] overflow-hidden rounded-full bg-white/10"
      >
        <motion.span
          className="block h-full w-full origin-left rounded-full bg-gold-500"
          initial={grow ? { transform: "scaleX(0)" } : false}
          animate={{ transform: `scaleX(${share})` }}
          transition={{ duration: 0.3, ease: easeOutExpo }}
        />
      </span>
      <span className="text-[12px] text-ash-400 tabular-nums">
        {t("setup.plus.spentOfTotal", { amount: withMoney(formatAmount(spent), money) })}
      </span>
    </span>
  );
}

function MoneyEditor({
  shown,
  set,
  account,
  panel,
  submitted,
  serverIssues,
  advanced,
  namer,
  lang,
}: EditorProps & { account: CloudAccount; advanced: boolean; namer: Namer; lang: Lang }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const control = usePlusControl(account.id);
  const [confirmReset, setConfirmReset] = useState(false);
  const limits = shown.limits;
  const maxDeal = panel.settings.server.money.maxDeal;
  const spent = panel.settings.spent ?? plusAi(account)?.spent ?? null;
  const saved = panel.settings.config.trade;
  const currency = currencyFieldState(panel.settings.server, panel.settings.rules, lang);
  const currencyRefused =
    currency.kind === "own" &&
    (currencyEditProblem(shown.trade, saved) !== null || serverIssues.has("trade.currency"));
  const unit = moneyNameOf(panel.settings.server, lang);
  const props = { shown, set, maxDeal, submitted, serverIssues, unit };
  const reset = async () => {
    setConfirmReset(false);
    const done = await control("reset_spent");
    if (done) {
      toast.success(t("afkPlus.settings.resetSpentDone"));
      void queryClient.invalidateQueries({ queryKey: plusKey(account.id) });
    }
  };
  return (
    <>
      {/* Said once, so no limit is ever read as lira (the contract has no name for a mode's money). */}
      <p className="text-[12.5px] text-ash-400">{t("afkPlus.settings.limitsMoney")}</p>
      {!canSpend(limits) ? (
        <Hint tone="warn" className="mt-2">
          {t("afkPlus.settings.needed")}
        </Hint>
      ) : null}
      {/* Each counter sits under the limit it counts against: the total counts since the last
          reset, and a reset empties it only. */}
      <div className="grid grid-cols-2 gap-x-4 gap-y-4 pt-3">
        <LimitAmount {...props} field="perDealMax" label={t("afkPlus.settings.perDealMax")} />
        <LimitAmount
          {...props}
          field="totalBudget"
          label={t("afkPlus.settings.totalBudget")}
          extra={
            spent && spent.total > 0 ? (
              <p className="-mt-1 flex flex-wrap items-baseline gap-x-2 text-[12.5px] text-bone-100">
                <span className="tabular-nums">
                  {t(
                    limits.totalBudget === null
                      ? "afkPlus.settings.spentNoBudget"
                      : "afkPlus.settings.spentTotal",
                    { amount: formatAmount(spent.total) },
                  )}
                </span>
                <QuietLink onClick={() => setConfirmReset(true)}>
                  {t("afkPlus.settings.resetSpent")}
                </QuietLink>
              </p>
            ) : null
          }
        />
      </div>
      <div className="pt-4">
        <MoneyFits
          limits={limits}
          orders={panel.settings.config.orders}
          namer={namer}
          lang={lang}
          money={moneyNameOf(panel.settings.server, lang)}
        />
      </div>
      {/* afkplus-free.md §3.5: the one quiet line, never a switch. */}
      <p className="pt-3 text-[12.5px] text-ash-400">{t("afkPlus.settings.secret")}</p>
      <Advanced
        summary={t(
          currency.kind === "hidden" ? "setup.plus.adv.money" : "setup.plus.adv.moneyCurrency",
        )}
        open={advanced}
      >
        <div className="grid grid-cols-2 gap-x-4 gap-y-4 pt-2 pb-1">
          <LimitAmount
            {...props}
            field="dailySpendMax"
            label={t("afkPlus.settings.dailySpendMax")}
            extra={
              spent && spent.today > 0 ? (
                <p className="-mt-1 text-[12.5px] text-bone-100 tabular-nums">
                  {t("afkPlus.settings.spentToday", { amount: formatAmount(spent.today) })}
                </p>
              ) : null
            }
          />
          <LimitAmount {...props} field="balanceFloor" label={t("afkPlus.settings.balanceFloor")} />
        </div>
        <CurrencyField
          state={currency}
          value={shown.trade.currency ?? ""}
          refused={currencyRefused}
          onChange={(text) => set((s) => ({ ...s, trade: withCurrency(s.trade, saved, text) }))}
        />
      </Advanced>
      <Dialog
        open={confirmReset}
        onOpenChange={setConfirmReset}
        title={t("afkPlus.settings.resetSpentTitle")}
        description={t("afkPlus.settings.resetSpentBody")}
        closeLabel={t("common.close")}
        width={420}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmReset(false)}>
              {t("afkPlus.settings.discard")}
            </Button>
            <Button variant="primary" onClick={() => void reset()}>
              {t("afkPlus.settings.resetSpent")}
            </Button>
          </>
        }
      />
    </>
  );
}

// --- ProBot AI ne yapsın? -------------------------------------------------------------------------

function ActEditor({
  shown,
  set,
  panel,
  submitted,
  serverIssues,
  account,
}: EditorProps & { account: CloudAccount }) {
  const { t } = useTranslation();
  const itemPay = panel.settings.rules.features.itemPay;
  const maxDeal = panel.settings.server.money.maxDeal;
  const authority = authorityOf(shown.authority);
  const pay = (["money", "items", "both"] as const).find((p) => p === shown.pay) ?? "money";
  const problems = limitProblems(shown.limits, maxDeal);
  const refusedHere = [...serverIssues].some((field) => /^(authority|allow|pay)\b/.test(field));
  // A plain command row of the account to run after a trade (never a secret one).
  const rows = account.config.commands.filter((command) => !command.secret && command.text);
  const returnRow = rows.some((row) => row.id === shown.trade.returnRow)
    ? (shown.trade.returnRow ?? "stay")
    : "stay";
  const unit = moneyNameOf(panel.settings.server, currentLang());
  const props = { shown, set, maxDeal, submitted, serverIssues, unit };
  return (
    <>
      {refusedHere ? (
        <p role="alert" className="pb-2 text-[12.5px] text-flare-300">
          {t("afkPlus.settings.fineRefused")}
        </p>
      ) : null}
      <AuthorityChoice
        value={authority}
        onChange={(next) => set((s) => ({ ...s, authority: next }))}
      >
        {/* The approval limits work only here (README: `deal`); their values stay while it is off. */}
        {authority === "deal" ? (
          <div className="grid grid-cols-2 gap-x-4 gap-y-4 pt-2 pl-3">
            <LimitAmount
              {...props}
              field="approvalAbove"
              label={t("afkPlus.settings.approvalAbove")}
              extra={
                <p className="-mt-1 text-[12px] text-ash-400">
                  {t("afkPlus.settings.approvalHelp")}
                </p>
              }
            />
            <LimitAmount
              {...props}
              field="newCounterpartApprovalAbove"
              label={t("afkPlus.settings.newCounterpart")}
              extra={
                <p className="-mt-1 text-[12px] text-ash-400">
                  {t("afkPlus.settings.newCounterpartHelp")}
                </p>
              }
            />
          </div>
        ) : null}
      </AuthorityChoice>
      <div className="pt-3">
        <ToggleRow
          title={t("afkPlus.settings.allowBuy")}
          help={t("setup.plus.sidesHelp.buy")}
          checked={shown.allow.buy}
          onCheckedChange={(buy) => set((s) => ({ ...s, allow: { ...s.allow, buy } }))}
        />
        <ToggleRow
          title={t("afkPlus.settings.allowSell")}
          help={t("setup.plus.sidesHelp.sell")}
          checked={shown.allow.sell}
          onCheckedChange={(sell) => set((s) => ({ ...s, allow: { ...s.allow, sell } }))}
        />
      </div>
      <Labeled label={t("afkPlus.settings.returnRow")} help={t("afkPlus.settings.returnHelp")}>
        <Select
          label={t("afkPlus.settings.returnRow")}
          value={returnRow}
          onValueChange={(next) => {
            const row = next === "stay" ? null : next;
            set((s) => ({ ...s, trade: { ...s.trade, returnRow: row } }));
          }}
          options={[
            { value: "stay", label: t("afkPlus.settings.returnStay") },
            ...rows.map((row) => ({ value: row.id, label: row.text ?? row.id })),
          ]}
          className="max-w-[360px]"
        />
      </Labeled>
      <Advanced
        summary={t(itemPay ? "setup.plus.adv.act" : "setup.plus.adv.actNoPay")}
        open={submitted && Boolean(problems.approvalTimeoutMin)}
      >
        {itemPay ? (
          <Labeled label={t("afkPlus.settings.pay")}>
            <Segmented
              label={t("afkPlus.settings.pay")}
              value={pay}
              onValueChange={(next) => set((s) => ({ ...s, pay: next }))}
              className="self-start"
              options={[
                { value: "money", label: t("afkPlus.settings.pays.money") },
                { value: "items", label: t("afkPlus.settings.pays.items") },
                { value: "both", label: t("afkPlus.settings.pays.both") },
              ]}
            />
          </Labeled>
        ) : null}
        <IntField
          label={t("afkPlus.settings.timeout")}
          value={shown.limits.approvalTimeoutMin}
          onChange={(value) =>
            value !== null &&
            set((s) => ({ ...s, limits: { ...s.limits, approvalTimeoutMin: value } }))
          }
          after={t("afkPlus.settings.timeoutValue")}
          error={
            submitted && problems.approvalTimeoutMin
              ? t("afkPlus.settings.limitErrors.timeout")
              : null
          }
          className="py-2.5"
          inputClassName="w-[72px] text-center"
        />
      </Advanced>
    </>
  );
}

// --- Sohbette ne yapsın? --------------------------------------------------------------------------

function ChatEditor({ shown, set, panel, account }: EditorProps & { account: CloudAccount }) {
  const { t } = useTranslation();
  const d = shown.discovery;
  const lang = currentLang();
  const templates = panel.settings.server.ads;
  // "Sohbete ilan" as the service plans it (TRADE-ANY trade-any-4); none from an older service.
  const plan = panel.settings.adPreview ?? null;
  const setD = (parts: Partial<SettingsPart["discovery"]>) =>
    set((s) => ({ ...s, discovery: { ...s.discovery, ...parts } }));
  const choosable = d.ads && templates.length > 1;
  return (
    <>
      <ToggleRow
        lead
        title={t("afkPlus.settings.ads")}
        help={t(plan ? "afkPlus.settings.adsHelpPlan" : "afkPlus.settings.adsHelp")}
        checked={d.ads}
        disabled={!templates.length}
        onCheckedChange={(ads) =>
          setD({ ads, adTemplate: d.adTemplate || (templates[0]?.id ?? d.adTemplate) })
        }
      />
      {d.ads && templates.length ? (
        <>
          {/* The minutes are written only as picked here, and only to a service that plans them. */}
          <ChatAds
            plan={plan}
            minutes={d.adEveryMin}
            name={account.name}
            onMinutes={(adEveryMin) => setD({ adEveryMin })}
          />
          {plan ? (
            <p className="pb-1 text-[12.5px] text-pretty text-ash-400">
              {t("setup.plus.ads.note")}
            </p>
          ) : null}
        </>
      ) : null}
      {/* The setup's switch, on by default and said: on scans the chat and writes to a seller once,
          off keeps the scan as it is (the site's rule). */}
      <ToggleRow
        title={t("setup.plus.writeSellers")}
        checked={d.scanChat && d.proactivePm}
        onCheckedChange={(on) =>
          setD(on ? { scanChat: true, proactivePm: true } : { proactivePm: false })
        }
      />
      {/* The ad's text waits here while the server's profile offers more than one (the site's). */}
      <Advanced
        summary={`${t("setup.plus.adv.chat")}${choosable ? t("setup.plus.adv.chatText") : ""}`}
      >
        <ToggleRow
          title={t("afkPlus.settings.scanChat")}
          checked={d.scanChat}
          onCheckedChange={(scanChat) => setD({ scanChat, proactivePm: scanChat && d.proactivePm })}
        />
        {choosable ? (
          <Labeled label={t("afkPlus.settings.adTemplate")}>
            <Select
              label={t("afkPlus.settings.adTemplate")}
              value={
                templates.some((tpl) => tpl.id === d.adTemplate)
                  ? d.adTemplate
                  : (templates[0]?.id ?? "")
              }
              onValueChange={(adTemplate) => setD({ adTemplate })}
              options={templates.map((tpl) => ({
                value: tpl.id,
                label: lang === "en" ? tpl.en : tpl.tr,
              }))}
              className="max-w-[360px]"
            />
          </Labeled>
        ) : null}
      </Advanced>
    </>
  );
}

// --- Nasıl konuşsun? ------------------------------------------------------------------------------

function TalkEditor({
  shown,
  set,
  panel,
  submitted,
  serverIssues,
  account,
  namer,
  lang,
}: EditorProps & { account: CloudAccount; namer: Namer; lang: Lang }) {
  const { t } = useTranslation();
  const persona = shown.persona;
  const preset = presetOf(persona.preset);
  const judge = panel.settings.judge;
  const savedPersona = panel.settings.config.persona;
  const introProblem = personaTextProblem(persona.intro, PLUS_LIMITS.introMax);
  const setPersona = (parts: Partial<SettingsPart["persona"]>) =>
    set((s) => ({ ...s, persona: { ...s.persona, ...parts } }));
  const {
    tag,
    problem: tagProblem,
    line: tagLine,
  } = tagState(shown, panel, submitted, serverIssues, t);
  const shownTag = tag.on && tag.text.trim() && !tagProblem ? tag.text.trim() : null;
  return (
    <>
      <Labeled label={t("afkPlus.settings.persona")}>
        <Segmented
          label={t("afkPlus.settings.persona")}
          value={preset}
          onValueChange={(value) => setPersona({ preset: value })}
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
        checked={persona.playerTalk}
        onCheckedChange={(playerTalk) => setPersona({ playerTalk })}
      />
      <div className="pt-3 pb-1">
        <TalkPreview
          preset={preset}
          playerTalk={persona.playerTalk}
          tag={shownTag}
          name={account.name}
          orders={panel.settings.config.orders}
          perDealMax={shown.limits.perDealMax}
          namer={namer}
          lang={lang}
        />
      </div>
      <Advanced
        summary={t("setup.plus.adv.talk")}
        open={(submitted && introProblem !== null) || tagLine !== null}
      >
        <PersonaText
          label={t("afkPlus.settings.intro")}
          value={persona.intro}
          max={PLUS_LIMITS.introMax}
          placeholder={t("afkPlus.settings.introPlaceholder")}
          help={t("afkPlus.settings.introHelp")}
          // The verdict is about the saved words: it shows only while they are on screen.
          judge={persona.intro === savedPersona.intro && persona.intro ? judge.intro : null}
          problem={submitted || introProblem === "length" ? introProblem : null}
          onChange={(intro) => setPersona({ intro })}
        />
        <ToggleRow
          title={t("afkPlus.settings.prefix")}
          checked={tag.on}
          onCheckedChange={(on) =>
            setPersona({
              prefix: { on, text: on || !tagProblem ? tag.text : savedPersona.prefix.text },
            })
          }
        />
        {tag.on ? (
          <TagField
            value={tag.text}
            line={tagLine}
            onChange={(text) => setPersona({ prefix: { on: true, text } })}
          />
        ) : tagLine ? (
          <p role="alert" className="pb-2.5 text-[12px] text-flare-300">
            {tagLine}
          </p>
        ) : null}
        <Labeled label={t("afkPlus.settings.lang")}>
          <Segmented
            label={t("afkPlus.settings.lang")}
            value={["auto", "tr", "en"].includes(shown.lang) ? shown.lang : "auto"}
            onValueChange={(lang) => set((s) => ({ ...s, lang }))}
            // Language names are written in their own language, never translated (design.md §7).
            options={[
              { value: "auto", label: t("afkPlus.settings.langs.auto") },
              { value: "tr", label: "Türkçe" },
              { value: "en", label: "English" },
            ]}
            className="self-start"
          />
        </Labeled>
        {/* Small talk said as its opposite, on by default (27 Oct 2026). */}
        <ToggleRow
          title={t("afkPlus.settings.offTopic")}
          checked={!persona.smallTalk}
          onCheckedChange={(onTopic) => setPersona({ smallTalk: !onTopic })}
        />
      </Advanced>
    </>
  );
}

// --- Kimlere cevap versin? ------------------------------------------------------------------------

/** "Kimlerle → Yalnız arkadaşlarımla": the in-game names it deals with (part of the draft). */
function FriendsList({
  shown,
  set,
  panel,
  refused,
}: {
  shown: SettingsPart;
  set: Change;
  panel: PlusPanel;
  /** The places of names the service refused at the last save. */
  refused: ReadonlySet<number>;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const id = useId();
  const [name, setName] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const friends = shown.who.friends;
  const max = panel.settings.rules.friendsMax || PLUS_LIMITS.friendsMax;
  const add = () => {
    const value = name.trim();
    const found = blockedNameProblem(value, friends, panel.settings.config.blocked);
    if (found) {
      setProblem(t(`afkPlus.settings.friendsErrors.${found}`));
      return;
    }
    if (friends.length >= max) {
      setProblem(t("afkPlus.settings.friendsErrors.full", { count: max }));
      return;
    }
    set((s) => ({ ...s, who: { ...s.who, friends: [...s.who.friends, value] } }));
    setName("");
  };
  return (
    <div className="flex flex-col gap-2 pb-2.5">
      <div className="flex flex-col gap-1.5">
        <label htmlFor={id} className="sr-only">
          {t("afkPlus.settings.friendsPlaceholder")}
        </label>
        <div className="flex gap-2">
          <input
            id={id}
            value={name}
            maxLength={16}
            autoComplete="off"
            spellCheck={false}
            placeholder={t("afkPlus.settings.friendsPlaceholder")}
            aria-invalid={problem ? true : undefined}
            aria-describedby={problem ? `${id}-error` : undefined}
            onChange={(event) => {
              setName(event.target.value);
              setProblem(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                add();
              }
            }}
            className={cn(
              inputClass,
              "h-[34px] max-w-[240px] px-3",
              problem && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
            )}
          />
          <Button variant="secondary" disabled={!name.trim()} onClick={add}>
            {t("afkPlus.settings.friendsAdd")}
          </Button>
        </div>
        {problem ? (
          <p id={`${id}-error`} role="alert" className="text-[12px] text-flare-300">
            {problem}
          </p>
        ) : null}
      </div>
      {refused.size ? (
        <p role="alert" className="text-[12px] text-flare-300">
          {t("afkPlus.settings.friendsErrors.refused")}
        </p>
      ) : null}
      {friends.length ? (
        <ul className="flex flex-wrap gap-1.5">
          <AnimatePresence initial={false}>
            {friends.map((entry, index) => (
              <motion.li
                key={entry}
                layout={layout ? "position" : false}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.2) }}
                exit={{ opacity: 0, transition: leave(0.14) }}
                className={cn(
                  "flex items-center gap-1 rounded-md bg-white/[0.05] py-0.5 pr-0.5 pl-2.5 text-[13px] text-bone-100",
                  refused.has(index) && "ring-1 ring-flare-400/60",
                )}
              >
                <span data-selectable>{entry}</span>
                <IconButton
                  label={t("afkPlus.settings.friendsRemove", { name: entry })}
                  icon={<X size={13} />}
                  size="sm"
                  onClick={() =>
                    set((s) => ({
                      ...s,
                      who: { ...s.who, friends: s.who.friends.filter((f) => f !== entry) },
                    }))
                  }
                />
              </motion.li>
            ))}
          </AnimatePresence>
        </ul>
      ) : (
        <p className="text-[12.5px] text-ash-300">{t("afkPlus.settings.friendsEmpty")}</p>
      )}
    </div>
  );
}

/** "Engellenenler": a name blocked or freed at once (never part of the draft). */
function BlockedList({ account, panel }: { account: CloudAccount; panel: PlusPanel }) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const control = usePlusControl(account.id);
  const id = useId();
  const [name, setName] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const blocked = panel.settings.config.blocked;
  const add = async () => {
    const value = name.trim();
    const found = blockedNameProblem(value, blocked);
    if (found) {
      setProblem(
        t(
          found === "format"
            ? "afkPlus.settings.blockedFormat"
            : "afkPlus.settings.blockedDuplicate",
        ),
      );
      return;
    }
    if (blocked.length >= PLUS_LIMITS.blockedMax) return;
    const done = await control("block", value);
    if (done) {
      setName("");
      toast.success(t("afkPlus.messages.blockedDone", { name: value }));
    }
  };
  return (
    <div className="flex flex-col gap-1.5 py-2.5">
      <span className="text-[13px] font-medium text-bone-100">
        {t("afkPlus.settings.groups.blocked")}
      </span>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void add();
        }}
        className="flex flex-col gap-1.5"
      >
        <label htmlFor={id} className="sr-only">
          {t("afkPlus.settings.blockedPlaceholder")}
        </label>
        <div className="flex gap-2">
          <input
            id={id}
            value={name}
            maxLength={16}
            autoComplete="off"
            spellCheck={false}
            placeholder={t("afkPlus.settings.blockedPlaceholder")}
            aria-invalid={problem ? true : undefined}
            aria-describedby={problem ? `${id}-error` : undefined}
            onChange={(event) => {
              setName(event.target.value);
              setProblem(null);
            }}
            className={cn(
              inputClass,
              "h-[34px] max-w-[240px] px-3",
              problem && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
            )}
          />
          <Button type="submit" variant="secondary" disabled={!name.trim()}>
            {t("afkPlus.settings.blockedAdd")}
          </Button>
        </div>
        {problem ? (
          <p id={`${id}-error`} role="alert" className="text-[12px] text-flare-300">
            {problem}
          </p>
        ) : null}
      </form>
      {blocked.length ? (
        <ul className="flex flex-wrap gap-1.5 pt-1">
          <AnimatePresence initial={false}>
            {blocked.map((entry) => (
              <motion.li
                key={entry}
                layout={layout ? "position" : false}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.2) }}
                exit={{ opacity: 0, transition: leave(0.14) }}
                className="flex items-center gap-1 rounded-md bg-white/[0.05] py-0.5 pr-0.5 pl-2.5 text-[13px] text-bone-100"
              >
                <span data-selectable>{entry}</span>
                <IconButton
                  label={t("afkPlus.settings.blockedRemove", { name: entry })}
                  icon={<X size={13} />}
                  size="sm"
                  onClick={() =>
                    void control("unblock", entry).then((done) => {
                      if (done) toast.success(t("afkPlus.messages.unblockedDone", { name: entry }));
                    })
                  }
                />
              </motion.li>
            ))}
          </AnimatePresence>
        </ul>
      ) : (
        <p className="text-[12.5px] text-ash-400">{t("afkPlus.settings.blockedEmpty")}</p>
      )}
    </div>
  );
}

function WhoEditor({
  shown,
  set,
  panel,
  submitted,
  serverIssues,
  account,
}: EditorProps & { account: CloudAccount }) {
  const { t } = useTranslation();
  const persona = shown.persona;
  const status = persona.statusPreset ?? (persona.status ? "own" : "none");
  const statusProblem = personaTextProblem(persona.status, PLUS_LIMITS.statusMax);
  const savedPersona = panel.settings.config.persona;
  const who = shown.who.mode === "friends" ? "friends" : "all";
  const refusedFriends = new Set<number>();
  for (const field of serverIssues) {
    const friend = /^who\.friends\[(\d+)\]/.exec(field);
    if (friend) refusedFriends.add(Number(friend[1]));
  }
  const setPersona = (parts: Partial<SettingsPart["persona"]>) =>
    set((s) => ({ ...s, persona: { ...s.persona, ...parts } }));
  return (
    <>
      <Labeled label={t("afkPlus.settings.scope")}>
        <Select
          label={t("afkPlus.settings.scope")}
          value={
            ["all", "trade", "off"].includes(shown.replies.scope) ? shown.replies.scope : "trade"
          }
          onValueChange={(scope) => set((s) => ({ ...s, replies: { ...s.replies, scope } }))}
          options={(["all", "trade", "off"] as const).map((value) => ({
            value,
            label: t(`afkPlus.settings.scopes.${value}`),
          }))}
          className="max-w-[300px]"
        />
      </Labeled>
      <Labeled label={t("afkPlus.settings.who")}>
        <Segmented
          label={t("afkPlus.settings.who")}
          value={who}
          onValueChange={(mode) => set((s) => ({ ...s, who: { ...s.who, mode } }))}
          className="self-start"
          options={[
            { value: "all", label: t("afkPlus.settings.whoAll") },
            { value: "friends", label: t("afkPlus.settings.whoFriends") },
          ]}
        />
      </Labeled>
      {who === "friends" ? (
        <FriendsList shown={shown} set={set} panel={panel} refused={refusedFriends} />
      ) : null}
      <ToggleRow
        title={t("afkPlus.settings.away")}
        checked={shown.replies.away}
        onCheckedChange={(away) => set((s) => ({ ...s, replies: { ...s.replies, away } }))}
      />
      {/* What the away line says ("Şu an AFK'yım: Akşam dönerim"), right under the switch that sends
          it. Never greyed out with it: breaks and the hours outside the working windows use it too. */}
      <Labeled label={t("afkPlus.settings.status")}>
        <Select
          label={t("afkPlus.settings.status")}
          value={status}
          onValueChange={(value) =>
            value === "own"
              ? setPersona({ statusPreset: null, status: persona.status })
              : value === "none"
                ? setPersona({ statusPreset: null, status: "" })
                : setPersona({ statusPreset: value, status: "" })
          }
          options={[
            { value: "none", label: t("afkPlus.settings.statusNone") },
            ...STATUS_PRESETS.map((value) => ({
              value,
              label: t(`afkPlus.settings.statusPresets.${value}`),
            })),
            { value: "own", label: t("afkPlus.settings.statusOwn") },
          ]}
          className="max-w-[300px]"
        />
      </Labeled>
      {status === "own" ? (
        <PersonaText
          label={t("afkPlus.settings.statusText")}
          value={persona.status}
          max={PLUS_LIMITS.statusMax}
          placeholder={t("afkPlus.settings.statusPlaceholder")}
          judge={
            persona.status === savedPersona.status && persona.status
              ? panel.settings.judge.status
              : null
          }
          problem={submitted || statusProblem === "length" ? statusProblem : null}
          onChange={(text) => setPersona({ status: text, statusPreset: null })}
        />
      ) : null}
      <div className="mt-1 border-t border-white/6">
        <BlockedList account={account} panel={panel} />
      </div>
    </>
  );
}

// --- Ne zaman çalışsın? ---------------------------------------------------------------------------

function DayChips({
  days,
  onChange,
  invalid,
}: {
  days: number[];
  onChange: (days: number[]) => void;
  invalid: boolean;
}) {
  const { t } = useTranslation();
  return (
    <fieldset
      aria-invalid={invalid ? true : undefined}
      className="m-0 flex min-w-0 flex-wrap gap-1 border-0 p-0"
    >
      <legend className="sr-only">{t("afkPlus.settings.daysLabel")}</legend>
      {WEEKDAYS.map((day) => {
        const on = days.includes(day);
        return (
          <button
            key={day}
            type="button"
            aria-pressed={on}
            aria-label={t(`afkPlus.settings.daysLong.d${day}`)}
            onClick={() =>
              onChange(sortedDays(on ? days.filter((d) => d !== day) : [...days, day]))
            }
            className={cn(
              "chamfer h-[28px] w-[38px] cursor-pointer text-[12px] font-semibold outline-none [--cut:5px]",
              "transition-[background-color,color,transform,scale] duration-150 ease-out active:scale-[0.96] focus-visible:ring-2 focus-visible:ring-gold-300",
              // The quiet plate, as every other pick here: the gold is the save's.
              on
                ? "bg-[color:var(--segmented-plate)] text-bone-50"
                : "bg-white/[0.06] text-ash-300 hover:bg-white/10 hover:text-bone-100",
            )}
          >
            {t(`afkPlus.settings.days.d${day}`)}
          </button>
        );
      })}
    </fieldset>
  );
}

function WindowRow({
  window,
  onChange,
  onRemove,
  showProblems,
}: {
  window: PlusWindow;
  onChange: (window: PlusWindow) => void;
  onRemove: () => void;
  showProblems: boolean;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const problems = windowProblems(window);
  const daysBad = showProblems && problems.includes("days");
  const timeBad = showProblems && problems.includes("time");
  return (
    <motion.li
      layout={layout ? "position" : false}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: enter(0.22) }}
      exit={{ opacity: 0, transition: leave(0.15) }}
      className="flex flex-col gap-1.5 rounded-[var(--radius-panel)] bg-white/[0.03] px-3 py-2.5"
    >
      <div className="flex flex-wrap items-end gap-x-4 gap-y-2">
        <DayChips
          days={window.days}
          invalid={daysBad}
          onChange={(days) => onChange({ ...window, days })}
        />
        <div className="flex items-end gap-2">
          <TimeField
            label={t("afkPlus.settings.from")}
            value={window.from}
            invalid={timeBad && !isTime(window.from)}
            onChange={(from) => from && onChange({ ...window, from })}
          />
          <TimeField
            label={t("afkPlus.settings.to")}
            value={window.to}
            invalid={timeBad && !isTime(window.to)}
            onChange={(to) => to && onChange({ ...window, to })}
          />
        </div>
        <IconButton
          label={t("afkPlus.settings.removeWindow")}
          icon={<X size={15} />}
          size="sm"
          className="mb-[3px] ml-auto"
          onClick={onRemove}
        />
      </div>
      {daysBad || timeBad ? (
        <p role="alert" className="text-[12px] text-flare-300">
          {daysBad ? t("afkPlus.settings.windowErrorDays") : t("afkPlus.settings.windowErrorTime")}
        </p>
      ) : null}
    </motion.li>
  );
}

/** "Mola ver": until a day 1 to 30 days ahead; saved at once, ended at once. */
function BreakRow({ account, panel }: { account: CloudAccount; panel: PlusPanel }) {
  const { t } = useTranslation();
  const save = usePlusInstantSave(account.id);
  const [picking, setPicking] = useState(false);
  const [days, setDays] = useState<(typeof PLUS_LIMITS.breakDays)[number]>(3);
  const lang = currentLang();
  const now = Date.now();
  const until = panel.settings.config.pauseUntil;
  const onBreak = until !== null && until * 1000 > now;
  // A break ends at the start of the day after its last day, in the computer's time.
  const end = (() => {
    const date = new Date(now);
    date.setDate(date.getDate() + days);
    date.setHours(0, 0, 0, 0);
    return Math.floor(date.getTime() / 1000);
  })();
  const take = async () => {
    setPicking(false);
    const done = await save(
      (config) => ({ ...config, pauseUntil: end }),
      "afkPlus.settings.failed",
    );
    if (done) toast.success(t("afkPlus.settings.breakSet"));
  };
  const finish = async () => {
    const done = await save(
      (config) => ({ ...config, pauseUntil: null }),
      "afkPlus.settings.failed",
    );
    if (done) toast.success(t("afkPlus.settings.breakEnded"));
  };
  return (
    <div className="mt-3 border-t border-white/7 pt-3">
      <AnimatePresence initial={false} mode="popLayout">
        {onBreak ? (
          <motion.div
            key="on"
            className="flex flex-wrap items-center justify-between gap-3"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            <p className="text-[13.5px] font-medium text-bone-50">
              {t("afkPlus.settings.breakOn", {
                when: untilPhrase(until ?? 0, now, lang),
              })}
            </p>
            <Button size="sm" variant="secondary" onClick={() => void finish()}>
              {t("afkPlus.settings.breakEnd")}
            </Button>
          </motion.div>
        ) : picking ? (
          <motion.div
            key="pick"
            className="flex flex-col gap-2.5"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            {/* The row keeps its name while the length is picked ("Süre" names the choice). */}
            <span className="text-[13.5px] font-medium text-bone-50">
              {t("afkPlus.settings.breakTitle")}
            </span>
            <Segmented
              label={t("afkPlus.settings.breakFor")}
              value={String(days)}
              onValueChange={(value) =>
                setDays(Number(value) as (typeof PLUS_LIMITS.breakDays)[number])
              }
              options={PLUS_LIMITS.breakDays.map((value) => ({
                value: String(value),
                label: t("afkPlus.settings.breakDays", { count: value }),
              }))}
              className="self-start"
            />
            <div className="flex flex-wrap items-center gap-3">
              <p className="text-[12.5px] text-ash-300">
                {t("afkPlus.settings.breakPreview", { when: untilPhrase(end, now, lang) })}
              </p>
              <div className="ml-auto flex gap-2">
                <Button size="sm" variant="ghost" onClick={() => setPicking(false)}>
                  {t("afkPlus.settings.discard")}
                </Button>
                <Button size="sm" variant="secondary" onClick={() => void take()}>
                  {t("afkPlus.settings.breakTake")}
                </Button>
              </div>
            </div>
          </motion.div>
        ) : (
          <motion.div
            key="off"
            className="flex flex-wrap items-center justify-between gap-3"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            <div>
              <p className="text-[13.5px] font-medium text-bone-50">
                {t("afkPlus.settings.breakTitle")}
              </p>
              <p className="text-[12.5px] text-ash-400">{t("afkPlus.settings.breakHelp")}</p>
            </div>
            <Button size="sm" variant="secondary" onClick={() => setPicking(true)}>
              {t("afkPlus.settings.breakTake")}
            </Button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function TimeEditor({
  shown,
  set,
  panel,
  submitted,
  account,
}: EditorProps & { account: CloudAccount }) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const schedule = shown.schedule;
  const setWindows = (windows: PlusWindow[]) =>
    set((s) => ({ ...s, schedule: { ...s.schedule, windows } }));
  return (
    <>
      <ToggleRow
        lead
        title={t("afkPlus.settings.hoursSwitch")}
        help={t("afkPlus.settings.hoursNote")}
        checked={schedule.on}
        onCheckedChange={(on) =>
          set((s) => ({
            ...s,
            schedule: {
              on,
              // The first time: every evening, so the switch never means "never".
              windows:
                on && !s.schedule.windows.length
                  ? [{ days: [1, 2, 3, 4, 5, 6, 7], from: "16:00", to: "23:00" }]
                  : s.schedule.windows,
            },
          }))
        }
      />
      <AnimatePresence initial={false}>
        {schedule.on ? (
          <motion.div
            key="windows"
            layout={layout ? "position" : false}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.15) }}
            className="flex flex-col gap-2 pt-1"
          >
            <ul className="flex flex-col gap-2">
              <AnimatePresence initial={false}>
                {schedule.windows.map((window, index) => (
                  <WindowRow
                    // biome-ignore lint/suspicious/noArrayIndexKey: windows have no ids; a removal re-keys the rest, which is fine for plain fields
                    key={index}
                    window={window}
                    showProblems={submitted}
                    onChange={(next) =>
                      setWindows(schedule.windows.map((w, i) => (i === index ? next : w)))
                    }
                    onRemove={() => setWindows(schedule.windows.filter((_, i) => i !== index))}
                  />
                ))}
              </AnimatePresence>
            </ul>
            <div className="flex flex-wrap items-center gap-3">
              <Button
                size="sm"
                variant="ghost"
                icon={<Plus size={14} weight="bold" />}
                disabled={schedule.windows.length >= PLUS_LIMITS.windowsMax}
                onClick={() =>
                  setWindows([...schedule.windows, { days: [6, 7], from: "12:00", to: "23:00" }])
                }
              >
                {t("afkPlus.settings.addWindow")}
              </Button>
              <span className="text-[12px] text-ash-400">
                {schedule.windows.length >= PLUS_LIMITS.windowsMax
                  ? t("afkPlus.settings.windowsFull")
                  : t("afkPlus.settings.turkeyTime")}
              </span>
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
      <BreakRow account={account} panel={panel} />
    </>
  );
}

// --- Diğer ayarlar --------------------------------------------------------------------------------

function OtherEditor({ shown, set, submitted, saver }: EditorProps & { saver: boolean }) {
  const { t } = useTranslation();
  return (
    <>
      {saver ? (
        <Labeled label={t("afkPlus.settings.saver")}>
          <Segmented
            label={t("afkPlus.settings.saver")}
            value={shown.replies.saveAt === 50 ? "50" : shown.replies.saveAt === 80 ? "80" : "off"}
            onValueChange={(value) =>
              set((s) => ({
                ...s,
                replies: { ...s.replies, saveAt: value === "off" ? null : Number(value) },
              }))
            }
            options={(["80", "50", "off"] as const).map((value) => ({
              value,
              label: t(`afkPlus.settings.savers.${value}`),
            }))}
            className="self-start"
          />
        </Labeled>
      ) : null}
      <div className="py-2.5">
        <TimeField
          label={t("afkPlus.settings.summaryAt")}
          value={shown.summaryAt}
          invalid={submitted && !isTime(shown.summaryAt)}
          onChange={(time) => time && set((s) => ({ ...s, summaryAt: time }))}
        />
      </div>
    </>
  );
}

// --- Bildirimler (the launcher's own; at once, every cloud account) -------------------------------

const NOTIFY_KEY = ["afk-plus-notify"] as const;

function useNotifyPrefs() {
  return useQuery({
    queryKey: NOTIFY_KEY,
    queryFn: () => plusApi.notifyPrefs(),
    staleTime: 60_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
}

function NotifyEditor() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const query = useNotifyPrefs();
  const prefs = query.data;
  const write = async (next: CloudNotifyPrefs) => {
    const before = prefs;
    queryClient.setQueryData(NOTIFY_KEY, next);
    try {
      queryClient.setQueryData(NOTIFY_KEY, await plusApi.setNotifyPrefs(next));
    } catch (error) {
      if (before) queryClient.setQueryData(NOTIFY_KEY, before);
      afkFailed(error, t("afkPlus.notify.failed"));
    }
  };
  return (
    <>
      <p className="text-[12.5px] text-ash-400">{t("afkPlus.settings.notifyAll")}</p>
      {!prefs ? (
        query.isError ? (
          <p className="py-2 text-[12.5px] text-ash-300">{t("afkPlus.notify.loadFailed")}</p>
        ) : (
          <div className="h-[220px]" aria-hidden />
        )
      ) : (
        <>
          {/* One column, in groups: a switch sits on its own row's line, never beside another's words. */}
          <div className="flex flex-col gap-3 pt-1">
            {NOTIFY_GROUPS.map((group) => (
              <fieldset key={group.key} className="m-0 min-w-0 border-0 p-0">
                <legend className="p-0 pt-1 text-[12px] font-semibold text-ash-300">
                  {t(`afkPlus.notify.groups.${group.key}`)}
                </legend>
                {group.events.map((event) => (
                  <ToggleRow
                    key={event}
                    title={t(`afkPlus.notify.events.${event}`)}
                    checked={prefs.events[event] ?? true}
                    onCheckedChange={(on) =>
                      void write({ ...prefs, events: { ...prefs.events, [event]: on } })
                    }
                  />
                ))}
              </fieldset>
            ))}
          </div>
          <div className="mt-2 border-t border-white/7 pt-1">
            <ToggleRow
              title={t("afkPlus.notify.quiet")}
              help={t("afkPlus.notify.quietBody")}
              checked={prefs.quiet.enabled}
              onCheckedChange={(enabled) =>
                void write({ ...prefs, quiet: { ...prefs.quiet, enabled } })
              }
            />
            {prefs.quiet.enabled ? (
              <div className="flex gap-2 pb-2">
                <TimeField
                  label={t("afkPlus.settings.from")}
                  value={prefs.quiet.from}
                  onChange={(from) =>
                    from && void write({ ...prefs, quiet: { ...prefs.quiet, from } })
                  }
                />
                <TimeField
                  label={t("afkPlus.settings.to")}
                  value={prefs.quiet.to}
                  onChange={(to) => to && void write({ ...prefs, quiet: { ...prefs.quiet, to } })}
                />
              </div>
            ) : null}
          </div>
        </>
      )}
    </>
  );
}

// --- The section ---------------------------------------------------------------------------------------

/** Every problem the launcher can see before sending, so "Kaydet" never sends what will be refused. */
function hasProblems(shown: SettingsPart, maxDeal: number, saved: PlusConfig): boolean {
  const limits = limitProblems(shown.limits, maxDeal);
  return (
    Object.keys(limits).length > 0 ||
    personaTextProblem(shown.persona.intro, PLUS_LIMITS.introMax) !== null ||
    prefixProblem(shown.persona.prefix.text, [...shown.who.friends, ...saved.blocked]) !== null ||
    (shown.persona.statusPreset === null &&
      personaTextProblem(shown.persona.status, PLUS_LIMITS.statusMax) !== null) ||
    (shown.schedule.on && shown.schedule.windows.some((w) => windowProblems(w).length > 0)) ||
    !isTime(shown.summaryAt) ||
    currencyEditProblem(shown.trade, saved.trade) !== null
  );
}

/** What each question edits: a question holding edits not saved yet carries the gold dot. */
const QUESTION_PARTS: Record<Exclude<PlusQuestion, "notify">, (s: SettingsPart) => unknown> = {
  money: (s) => [
    s.limits.perDealMax,
    s.limits.totalBudget,
    s.limits.dailySpendMax,
    s.limits.balanceFloor,
    s.trade.currency ?? null,
  ],
  act: (s) => [
    s.authority,
    s.allow,
    s.pay,
    s.trade.returnRow,
    s.limits.approvalAbove,
    s.limits.newCounterpartApprovalAbove,
    s.limits.approvalTimeoutMin,
  ],
  chat: (s) => s.discovery,
  talk: (s) => [
    s.persona.preset,
    s.persona.playerTalk,
    s.persona.intro,
    s.persona.prefix,
    s.persona.smallTalk,
    s.lang,
  ],
  who: (s) => [s.replies.scope, s.replies.away, s.who, s.persona.status, s.persona.statusPreset],
  time: (s) => s.schedule,
  more: (s) => [s.replies.saveAt, s.summaryAt],
};

/** The questions with a field to fix: the flare mark on their row. */
function marksOf(
  shown: SettingsPart,
  panel: PlusPanel,
  submitted: boolean,
  serverIssues: ReadonlySet<string>,
): Set<PlusQuestion> {
  const saved = panel.settings.config;
  const maxDeal = panel.settings.server.money.maxDeal;
  const limits = limitProblems(shown.limits, maxDeal);
  const issued = (pattern: RegExp) => [...serverIssues].some((field) => pattern.test(field));
  const marks = new Set<PlusQuestion>();
  const shownLimit = (field: keyof PlusLimits) =>
    Boolean(limits[field]) && (submitted || limits[field] === "approvalAbove");
  if (
    shownLimit("perDealMax") ||
    shownLimit("totalBudget") ||
    shownLimit("dailySpendMax") ||
    shownLimit("balanceFloor") ||
    currencyEditProblem(shown.trade, saved.trade) !== null ||
    issued(/^(limits\.(perDealMax|totalBudget|dailySpendMax|balanceFloor)|trade\.currency)/)
  ) {
    marks.add("money");
  }
  if (
    shownLimit("approvalAbove") ||
    shownLimit("newCounterpartApprovalAbove") ||
    (submitted && Boolean(limits.approvalTimeoutMin)) ||
    issued(/^(authority|allow|pay|limits\.(approvalAbove|newCounterpart|approvalTimeout))/)
  ) {
    marks.add("act");
  }
  const tag = prefixProblem(shown.persona.prefix.text, [...shown.who.friends, ...saved.blocked]);
  const tagEdited =
    shown.persona.prefix.text !== saved.persona.prefix.text ||
    shown.persona.prefix.on !== saved.persona.prefix.on;
  if (
    (submitted && personaTextProblem(shown.persona.intro, PLUS_LIMITS.introMax) !== null) ||
    personaTextProblem(shown.persona.intro, PLUS_LIMITS.introMax) === "length" ||
    (tag !== null && (submitted || (tag !== "empty" && tagEdited))) ||
    issued(/^persona\.(intro|prefix)/)
  ) {
    marks.add("talk");
  }
  if (
    (submitted &&
      shown.persona.statusPreset === null &&
      personaTextProblem(shown.persona.status, PLUS_LIMITS.statusMax) !== null) ||
    issued(/^(who|replies|persona\.status)/)
  ) {
    marks.add("who");
  }
  if (
    submitted &&
    shown.schedule.on &&
    shown.schedule.windows.some((w) => windowProblems(w).length > 0)
  ) {
    marks.add("time");
  }
  if ((submitted && !isTime(shown.summaryAt)) || issued(/^summaryAt/)) marks.add("more");
  return marks;
}

export function SettingsSection({
  account,
  panel,
  dock,
}: {
  account: CloudAccount;
  panel: PlusPanel;
  /** Where the change bar docks (the panel's slot at the bottom of its scroller). */
  dock: HTMLElement | null;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  const features = usePlusFeatures();
  const save = usePlusSave(account.id);
  const draft = usePlusUi((s) => s.drafts[account.id] ?? null);
  const setDraft = usePlusUi((s) => s.setDraft);
  const focusGroup = usePlusUi((s) => s.focusGroup);
  const focusField = usePlusUi((s) => s.focusField);
  const setFocusGroup = usePlusUi((s) => s.setFocusGroup);
  const { layout } = useMotionPrefs();
  const [submitted, setSubmitted] = useState(false);
  const [serverIssues, setServerIssues] = useState<ReadonlySet<string>>(new Set());
  const [failure, setFailure] = useState<{ message: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [history, setHistory] = useState(false);
  // One question open at a time; "Sınırlara git" opens the one holding the limit.
  const [open, setOpen] = useState<PlusQuestion | null>(null);
  const [advancedFor, setAdvancedFor] = useState<PlusQuestion | null>(null);
  const [landing, setLanding] = useState<LimitField | null>(null);
  const saved = panel.settings.config;
  const shown = useMemo(() => applyDraft(saved, draft), [saved, draft]);
  const dirty = draft !== null;
  const maxDeal = panel.settings.server.money.maxDeal;
  const items = usePlusItemData(panel.settings.server.mc);
  const notify = useNotifyPrefs();
  const namer: Namer = {
    data: items.data,
    custom: saved.customItems,
    unnamed: t("afkPlus.items.unnamed"),
  };

  const set: Change = (update) => {
    setDraft(account.id, editDraft(saved, usePlusUi.getState().drafts[account.id], update));
    setServerIssues(new Set());
    setFailure(null);
  };

  const submit = async () => {
    if (!draft || saving) return;
    setSubmitted(true);
    if (hasProblems(shown, maxDeal, saved)) {
      // The first question with a field to fix opens, so the mark is where the player looks.
      const first = [...marksOf(shown, panel, true, serverIssues)][0];
      if (first && first !== open) setOpen(first);
      return;
    }
    setSaving(true);
    try {
      const sent = draft;
      const outcome = await save((latest) => withSettings(latest, applyDraft(latest, sent)));
      if (outcome.kind === "invalid") {
        setServerIssues(new Set(outcome.issues.map((issue) => issue.field)));
        setFailure({ message: t("afkPlus.settings.fixFirst") });
        return;
      }
      // Edits typed while it saved stay a draft.
      if (usePlusUi.getState().drafts[account.id] === sent) setDraft(account.id, null);
      setSubmitted(false);
      toast.success(t("afkPlus.settings.saved"));
    } catch (error) {
      setFailure({ message: isFriendly(error) ? error.message : t("afkPlus.settings.failed") });
    } finally {
      setSaving(false);
    }
  };

  // Ctrl+S saves while something waits (as the instance settings and the cloud sheet do). The field
  // with the caret lets go first: an amount that is not one goes back to the last good value, so
  // what is saved is what the fields show (a click on "Kaydet" does the same by itself).
  const shortcut = useRef(() => {});
  shortcut.current = () => void submit();
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      if (event.key.toLowerCase() !== "s") return;
      event.preventDefault();
      const active = document.activeElement;
      if (active instanceof HTMLInputElement) active.blur();
      requestAnimationFrame(() => shortcut.current());
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // "Sınırlara git": the question holding the limit opens (its "Gelişmiş" too when the field is
  // there), then the field comes into view and takes the focus; at once under reduced motion and in
  // Performance mode. A field that does not show (an approval limit while another authority is
  // chosen) brings its question into view instead.
  useEffect(() => {
    if (!focusGroup) return;
    if (focusGroup === "limits" && focusField) {
      const target = limitQuestion(focusField as LimitField);
      setOpen(target.question);
      setAdvancedFor(target.advanced ? target.question : null);
      setLanding(focusField as LimitField);
    }
    setFocusGroup(null);
  }, [focusGroup, focusField, setFocusGroup]);
  useEffect(() => {
    if (!landing) return;
    const frame = requestAnimationFrame(() => {
      const behavior = layout ? "smooth" : "auto";
      const field = document.getElementById(`afk-plus-limits-${landing}`);
      const node = field ?? document.getElementById(`question-${limitQuestion(landing).question}`);
      node?.scrollIntoView({ block: field ? "center" : "start", behavior });
      field?.querySelector("input")?.focus({ preventScroll: true });
      setLanding(null);
    });
    return () => cancelAnimationFrame(frame);
  }, [landing, layout]);

  // A problem the form shows blocks the save at once (the AFK sheets' rule, SHEET-UX decision 27):
  // after "Kaydet", while an edited tag or money name reads wrong (its line shows as it is typed),
  // while a field is marked as it is typed (a text too long, an approval above the per-deal limit),
  // and while the service's marks stand (any edit takes them away).
  const { problem: tagProblem, saved: savedTag } = tagState(
    shown,
    panel,
    submitted,
    serverIssues,
    t,
  );
  const tagShown =
    tagProblem !== null &&
    tagProblem !== "empty" &&
    (shown.persona.prefix.text !== savedTag.text || shown.persona.prefix.on !== savedTag.on);
  const blocking =
    (submitted && hasProblems(shown, maxDeal, saved)) ||
    tagShown ||
    currencyEditProblem(shown.trade, saved.trade) !== null ||
    liveProblems(shown, maxDeal) ||
    serverIssues.size > 0;
  const props: EditorProps = { shown, set, panel, submitted, serverIssues };

  const now = Date.now();
  const pause = saved.pauseUntil;
  const answers = plusAnswers(
    { ...shown, orders: saved.orders, blocked: saved.blocked },
    {
      lang,
      money: moneyNameOf(panel.settings.server, lang),
      itemName: (ref) => itemLabel(ref, namer, lang),
      breakUntil: pause !== null && pause * 1000 > now ? untilPhrase(pause, now, lang) : null,
      schedule: features.schedule,
      saver: features.saver,
      preview: panel.settings.adPreview ?? null,
      notify: notify.data ?? null,
    },
    t,
  );
  const marks = marksOf(shown, panel, submitted, serverIssues);
  const unsaved = (question: Exclude<PlusQuestion, "notify">) =>
    dirty &&
    JSON.stringify(QUESTION_PARTS[question](shown)) !==
      JSON.stringify(QUESTION_PARTS[question](saved));
  const spent = panel.settings.spent ?? plusAi(account)?.spent ?? null;
  const total = shown.limits.totalBudget;
  const toggle = (question: PlusQuestion) => {
    setAdvancedFor(null);
    setOpen((current) => (current === question ? null : question));
  };
  const row = (
    question: PlusQuestion,
    editor: ReactNode,
    more: { item?: string; extra?: ReactNode } = {},
  ) => (
    <QuestionRow
      key={question}
      id={question}
      item={more.item ?? PLUS_ITEMS[question]}
      question={question === "more" ? t("setup.other") : t(`setup.plus.questions.${question}`)}
      answer={answers[question]}
      extra={more.extra}
      open={open === question}
      onToggle={() => toggle(question)}
      dirty={question !== "notify" && unsaved(question)}
      alert={marks.has(question)}
    >
      {editor}
    </QuestionRow>
  );

  return (
    <>
      {features.history ? (
        <div className="-mt-1 mb-2 flex justify-end">
          <Button
            size="sm"
            variant="ghost"
            icon={<ClockCounterClockwise size={15} />}
            onClick={() => setHistory(true)}
          >
            {t("afkPlus.settings.history")}
          </Button>
        </div>
      ) : null}
      <QuestionList label={t("setup.questionsLabel")}>
        {row(
          "money",
          <MoneyEditor
            {...props}
            account={account}
            advanced={advancedFor === "money"}
            namer={namer}
            lang={lang}
          />,
          {
            extra:
              total !== null && spent && spent.total > 0 ? (
                <SpentMeter
                  accountId={account.id}
                  spent={spent.total}
                  total={total}
                  money={moneyNameOf(panel.settings.server, lang)}
                />
              ) : null,
          },
        )}
        {row("act", <ActEditor {...props} account={account} />, {
          item: AUTHORITY_ITEMS[authorityOf(shown.authority)],
        })}
        {row("chat", <ChatEditor {...props} account={account} />)}
        {row("talk", <TalkEditor {...props} account={account} namer={namer} lang={lang} />)}
        {row("who", <WhoEditor {...props} account={account} />)}
        {features.schedule ? row("time", <TimeEditor {...props} account={account} />) : null}
        {row("more", <OtherEditor {...props} saver={features.saver} />)}
        {row("notify", <NotifyEditor />)}
      </QuestionList>
      {/* Always the dock's room, open or not: the last rows scroll out from under it and its coming
          and going never moves the list. */}
      <div aria-hidden className="shrink-0" style={{ height: DOCK_HEIGHT }} />

      <PlusChangeDock dock={dock} open={dirty} label={t("afkPlus.settings.barLabel")}>
        <p
          role={failure || blocking ? "alert" : undefined}
          className={cn(
            "min-w-0 flex-1 text-[12.5px]",
            failure || blocking ? "text-flare-300" : "text-bone-100",
          )}
        >
          {failure
            ? failure.message
            : blocking
              ? t("afkPlus.settings.fixFirst")
              : t("afkPlus.settings.dirty")}
        </p>
        <Button
          variant="ghost"
          disabled={saving}
          onClick={() => {
            setDraft(account.id, null);
            setSubmitted(false);
            setServerIssues(new Set());
            setFailure(null);
          }}
        >
          {t("afkPlus.settings.discard")}
        </Button>
        <Button variant="primary" disabled={saving || blocking} onClick={() => void submit()}>
          {saving ? t("afkPlus.settings.saving") : t("afkPlus.settings.save")}
        </Button>
      </PlusChangeDock>

      <HistoryDialog
        open={history}
        onClose={() => setHistory(false)}
        account={account}
        panel={panel}
      />
    </>
  );
}
