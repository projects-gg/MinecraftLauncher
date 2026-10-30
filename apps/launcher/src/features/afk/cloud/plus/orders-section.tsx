// "Siparişler": what the assistant buys and sells (afkplus-free.md §7.2). On top, the player's own
// words ("Elytra alırım, en fazla 40 bin"): the free rules hint as they type, "Kontrol et" asks the
// service once and its answer opens as a card in the field's place (the order as read, at most two
// questions with ready answers, "Kaydet"); nothing is stored before "Kaydet". "Formla ekle" and
// "Envanterden seç" are the ways without the model. Then the orders: the active ones (what waits
// for the player first), the finished ones under them; a row says the order's state in a word in
// its own place, a handoff ("Takas sende") opens under its row. Things that come and go fade in
// place and the rows below glide (not in Performance mode).
import {
  ArrowClockwise,
  Cube,
  DotsThreeVertical,
  Info,
  Pause,
  PencilSimple,
  Play,
  Timer,
  Trash,
  WarningCircle,
} from "@phosphor-icons/react";
import {
  Button,
  ContextMenu,
  cn,
  enter,
  IconButton,
  leave,
  Menu,
  type MenuItem,
  SwapLabel,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, type ReactNode, useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed } from "../../errors";
import type { CloudAccount } from "../api";
import {
  type PlusApproval,
  type PlusNewCustomItem,
  type PlusNewOrder,
  type PlusOrder,
  type PlusOrderOutcome,
  type PlusPanel,
  type PlusServer,
  plusApi,
} from "./api";
import { ApprovalCard, useJoinForTrade } from "./approval-card";
import { useApprovalNow, waitsNow } from "./approval-clock";
import { AmountField, GroupTitle, QuietLink } from "./fields";
import { InventoryPick, type PickedItem } from "./inventory-pick";
import { PlusItemIcon } from "./item-icon";
import { itemInfo, itemLabel, type Namer, usePlusItemData } from "./items";
import {
  type AnswerResult,
  answer,
  type CheckAnswer,
  type Choice,
  type CustomItemLike,
  type DraftContext,
  fill,
  MAX_COUNT,
  type OrderDraft,
  type OrderQuestion,
  orderOf,
  questionsOf,
  receivedOf,
} from "./order-draft";
import { type FormField, type FormRequest, type FormSeed, OrderForm } from "./order-form";
import { checkOrderText, isMoneyQuestion, ruleHint } from "./order-rules";
import {
  activeCount,
  bareMoney,
  daysLeft,
  draftContextOf,
  moneyText,
  newCustomKey,
  type OrderState,
  orderState,
  sortedOrders,
} from "./orders";
import { moneyNameOf } from "./setup-parts";
import { plusAi } from "./types";
import { plusKey, storeSettings, usePlusFeatures } from "./use-plus";
import { cardWords, currentLang, formatAmount, type Lang, qtyItem } from "./words";

const SHOWN_QUESTIONS = 2;
/** "Okunuyor" this long without an answer: the line says the assistant cannot be reached. */
const SLOW_CHECK_MS = 8_000;

const nowSeconds = () => Math.floor(Date.now() / 1000);

/** The settings a chip changes, saved with the order in the same write. */
interface ChipSettings {
  pay?: "both";
  perDealMax?: number;
}

/** What the order writes do, with their toasts; the panel takes the stored settings. */
export function useOrderWrites(accountId: string) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const outcomeOf = (outcome: PlusOrderOutcome): PlusOrderOutcome => {
    if (outcome.kind === "saved") storeSettings(queryClient, accountId, outcome.settings);
    if (outcome.kind === "limited") {
      toast.error(t(outcome.daily ? "afkPlus.orders.writesDaily" : "afkPlus.orders.writesLimited"));
    }
    if (outcome.kind === "gone") {
      toast(t("afkPlus.orders.gone"));
      void queryClient.invalidateQueries({ queryKey: plusKey(accountId) });
    }
    return outcome;
  };
  const guard = async (run: () => Promise<PlusOrderOutcome>): Promise<PlusOrderOutcome | null> => {
    try {
      return outcomeOf(await run());
    } catch (error) {
      afkFailed(error, t("afkPlus.orders.failed"));
      return null;
    }
  };
  return {
    create: (
      order: Omit<PlusNewOrder, "id" | "at"> & { id?: string; at?: number },
      custom: PlusNewCustomItem[],
      chip: ChipSettings,
    ) =>
      guard(() =>
        plusApi.createOrder(
          accountId,
          { ...order, id: order.id ?? null, at: order.at ?? null },
          custom,
          { pay: chip.pay ?? null, perDealMax: chip.perDealMax ?? null },
        ),
      ),
    patch: (order: string, patch: Parameters<typeof plusApi.patchOrder>[2]) =>
      guard(() => plusApi.patchOrder(accountId, order, patch)),
    remove: (order: string) => guard(() => plusApi.deleteOrder(accountId, order)),
  };
}

/** An amount with its label over it and, under it, "adet başına" or its words. */
function Amount({
  label,
  value,
  under,
  muted = false,
}: {
  label: string;
  value: string;
  under?: string | null;
  /** Not given yet ("Yazılmadı"). */
  muted?: boolean;
}) {
  return (
    <span className="flex min-w-0 flex-col">
      <span className="text-[11.5px] text-ash-400">{label}</span>
      <span
        className={cn(
          "leading-tight tabular-nums",
          muted
            ? "pt-px text-[13px] text-ash-400"
            : "font-display text-[14px] font-semibold text-bone-50",
        )}
      >
        {value}
      </span>
      {under ? (
        <span className="max-w-[24ch] truncate text-[11.5px] text-ash-400">{under}</span>
      ) : null}
    </span>
  );
}

/** A line of the card the player can press to fix in the form. */
function CardField({
  children,
  onEdit,
  label,
  className,
}: {
  children: ReactNode;
  onEdit: () => void;
  label: string;
  className?: string;
}) {
  // The line itself is the button's name (what the player checks before saving); what pressing it
  // does follows for screen readers only.
  return (
    <button
      type="button"
      onClick={onEdit}
      className={cn(
        "-mx-1.5 min-w-0 cursor-pointer rounded-md px-1.5 py-0.5 text-left outline-none",
        "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/[0.05] active:scale-[0.99] focus-visible:ring-2 focus-visible:ring-gold-300",
        className,
      )}
    >
      {children}
      <span className="sr-only">, {label}</span>
    </button>
  );
}

/** The words of an order's conditions: "En az %90 dayanıklı, Keskinlik V". */
export function useConditionWords() {
  const { t } = useTranslation();
  return (
    req: {
      ench: Record<string, number>;
      dur: number | null;
      named: boolean;
      cursed: boolean;
    } | null,
    received: string | null,
    namer: Namer,
    lang: Lang,
  ): string[] => {
    if (!req || !received) return [];
    const parts: string[] = [];
    const info = itemInfo(received, namer.data);
    const wears = info ? info.maxDamage > 0 : false;
    const dur = req.dur ?? (wears ? 90 : 0);
    if (wears && dur > 0) parts.push(t("afkPlus.card.durability", { pct: dur }));
    for (const [id, level] of Object.entries(req.ench)) {
      const ench = namer.data?.enchantmentById.get(id);
      const name = ench ? ench.name[lang] : id.split(":").pop();
      parts.push(ench && ench.maxLevel === 1 ? (name ?? id) : `${name} ${roman(level)}`);
    }
    if (req.named) parts.push(t("afkPlus.card.named"));
    if (req.cursed) parts.push(t("afkPlus.card.cursed"));
    return parts;
  };
}

const ROMAN = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
const roman = (level: number) => ROMAN[level - 1] ?? String(level);

// --- the card -------------------------------------------------------------------------------------

function QuestionChips({
  question,
  title,
  unit,
  ctx,
  namer,
  money,
  counting,
  onChoice,
}: {
  question: OrderQuestion;
  /** The question's words (the bare amount field's name). */
  title: string;
  /** What the amount counts when it is not the server's money ("Elmas"), beside the field. */
  unit: string | null;
  ctx: DraftContext;
  namer: Namer;
  money: (value: number) => string;
  /** The draft's amounts count a paying item (a limit is then 1 to 9,999 of it). */
  counting: boolean;
  onChoice: (choice: Choice) => void;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  // No ready amounts (a price counted in a paying item): the field itself, not a lone "Başka".
  const bare =
    (question.kind === "limit" || question.kind === "qty") && question.amounts.length === 0;
  const [other, setOther] = useState(false);
  // A question that turns bare (an answer cleared its amounts) opens its field at once.
  const open = bare || other;
  const [amount, setAmount] = useState<number | null>(null);
  // An answer puts the next question's chips where these were: the second press of a double click
  // (`detail` 2) must not answer that one too.
  const chip = (key: string, label: ReactNode, choice: Choice, icon?: ReactNode) => (
    <li key={key}>
      <button
        type="button"
        onClick={(event) => {
          if (event.detail > 1) return;
          onChoice(choice);
        }}
        className={cn(
          "chamfer flex h-[30px] cursor-pointer items-center gap-2 bg-white/[0.06] px-3 text-[13px] whitespace-nowrap text-bone-100 outline-none [--cut:6px]",
          // The ring inside: the chamfer's clip cuts an outside ring to a sliver.
          "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/10 active:scale-[0.97] focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-inset",
          icon ? "pl-1" : null,
        )}
      >
        {icon}
        {label}
      </button>
    </li>
  );
  const others = (amounts: number[], counts: boolean, item?: string) => (
    <>
      {amounts.map((value) =>
        chip(
          String(value),
          item ? qtyItem(value, item, lang) : counts ? formatAmount(value) : money(value),
          { amount: value },
        ),
      )}
      <li>
        <button
          type="button"
          aria-expanded={other}
          onClick={(event) => {
            if (event.detail > 1) return;
            setOther((open) => !open);
          }}
          className={cn(
            "chamfer flex h-[30px] cursor-pointer items-center bg-white/[0.06] px-3 text-[13px] text-bone-100 outline-none [--cut:6px]",
            "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/10 active:scale-[0.97] focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-inset",
            other && "bg-white/12",
          )}
        >
          {t("afkPlus.questions.other")}
        </button>
      </li>
    </>
  );
  let chips: ReactNode;
  switch (question.kind) {
    case "which":
      chips = question.options.map((ref) =>
        chip(
          ref,
          itemLabel(ref, namer, lang),
          { option: ref },
          <PlusItemIcon item={ref} custom={namer.custom} size={24} />,
        ),
      );
      break;
    case "side":
      chips = [
        chip("buy", t("afkPlus.questions.buying"), "buy"),
        chip("sell", t("afkPlus.questions.selling"), "sell"),
      ];
      break;
    case "custom":
      chips = question.chips.map((value) =>
        chip(value, t(`afkPlus.questions.customChips.${value}`), value),
      );
      break;
    case "limit":
      chips = others(question.amounts, false);
      break;
    case "pay":
      chips = [
        chip("money", t("afkPlus.questions.payMoney"), "money"),
        chip("item", itemLabel(question.item, namer, lang), "item"),
      ];
      break;
    case "pay_off":
      chips = [
        chip("turn_on", t("afkPlus.questions.turnOn"), "turn_on"),
        chip(
          "switch",
          t(question.want === "item" ? "afkPlus.questions.useMoney" : "afkPlus.questions.useItem"),
          "switch",
        ),
      ];
      break;
    case "per":
      chips = [
        chip("each", t("afkPlus.questions.each"), "each"),
        chip("total", t("afkPlus.questions.total"), "total"),
      ];
      break;
    case "qty":
      chips = others(question.amounts, true);
      break;
    case "far":
      chips = [
        chip("yes", t("afkPlus.questions.yesRight"), "yes"),
        chip("fix", t("afkPlus.questions.fix"), "fix"),
      ];
      break;
    case "over_cap":
      chips = [
        chip("raise", t("afkPlus.questions.raise", { amount: money(question.limit) }), "raise"),
        chip("use_cap", t("afkPlus.questions.useCap", { cap: money(question.cap) }), "use_cap"),
      ];
      break;
  }
  const counts = question.kind === "qty" || (question.kind === "limit" && counting);
  const top = counts ? MAX_COUNT : ctx.money.maxDeal;
  return (
    <div className="flex flex-col gap-2">
      {bare ? null : <ul className="flex flex-wrap gap-1.5">{chips}</ul>}
      <AnimatePresence initial={false}>
        {open ? (
          <motion.form
            key="other"
            className="flex items-start gap-2"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.18) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
            onSubmit={(event: FormEvent) => {
              event.preventDefault();
              if (amount !== null && amount >= 1 && amount <= top) onChoice({ amount });
            }}
          >
            <AmountField
              label={bare ? title : t("afkPlus.questions.otherLabel")}
              hideLabel
              value={amount}
              onChange={(value) => setAmount(value ?? null)}
              className="w-[200px]"
              autoFocus={!bare}
            />
            {unit ? (
              <span className="shrink-0 pt-[8px] text-[13px] text-ash-300">{unit}</span>
            ) : null}
            <Button
              type="submit"
              size="sm"
              variant="secondary"
              className="mt-[3px]"
              disabled={amount === null || amount < 1 || amount > top}
            >
              {t("afkPlus.questions.use")}
            </Button>
          </motion.form>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

function questionText(
  question: OrderQuestion,
  t: (key: string, values?: Record<string, unknown>) => string,
  money: (value: number) => string,
  draft: OrderDraft,
): string {
  switch (question.kind) {
    case "which":
      return t("afkPlus.questions.which");
    case "side":
      return t("afkPlus.questions.side");
    case "custom":
      return t("afkPlus.questions.custom", { name: question.mention });
    case "limit":
      return t(
        question.side === "buy" ? "afkPlus.questions.limitBuy" : "afkPlus.questions.limitSell",
      );
    case "pay":
      return t(draft.side === "sell" ? "afkPlus.questions.paySell" : "afkPlus.questions.payBuy");
    case "pay_off":
      return t(
        question.want === "item" ? "afkPlus.questions.payOffItem" : "afkPlus.questions.payOffMoney",
      );
    case "per":
      return t("afkPlus.questions.per", { price: money(question.price) });
    case "qty":
      return t("afkPlus.questions.qty");
    case "far":
      return t("afkPlus.questions.far", {
        limit: money(question.limit),
        target: money(question.target),
      });
    case "over_cap":
      return t("afkPlus.questions.overCap", { cap: money(question.cap) });
  }
}

export interface CardState {
  answer: CheckAnswer;
  draft: OrderDraft;
  chip: ChipSettings;
  text: string;
}

function withChip(ctx: DraftContext, chip: ChipSettings): DraftContext {
  return {
    ...ctx,
    pay: chip.pay ?? ctx.pay,
    limits: { ...ctx.limits, perDealMax: chip.perDealMax ?? ctx.limits.perDealMax },
  };
}

/** The order as read, in plain lines; each part opens the form at its field. */
function OrderCard({
  card,
  ctx,
  namer,
  server,
  saving,
  onChoice,
  onEdit,
  onDiscard,
  onSave,
}: {
  card: CardState;
  ctx: DraftContext;
  namer: Namer;
  server: PlusServer;
  saving: boolean;
  onChoice: (question: OrderQuestion, choice: Choice) => void;
  onEdit: (field?: FormField) => void;
  onDiscard: () => void;
  onSave: () => void;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  const conditions = useConditionWords();
  const { draft } = card;
  const live = withChip(ctx, card.chip);
  const questions = questionsOf(draft, live);
  const shown = questions.slice(0, SHOWN_QUESTIONS);
  const firstQuestion = useRef<HTMLDivElement>(null);
  const serverTrade = server.trade === "owner" ? "owner" : "bot";
  const bare = bareMoney(server);
  const counts = draft.amounts === "count";
  const payName = draft.pay && draft.pay !== "money" ? itemLabel(draft.pay, namer, lang) : null;
  const amountOf = (value: number) =>
    counts && payName
      ? qtyItem(value, payName, lang)
      : counts
        ? formatAmount(value, lang)
        : moneyText(value, server, lang);
  // In words under the figure from 10.000 (`cardWords`): a count is no money.
  const words = (value: number | null) => {
    if (value === null || counts) return null;
    return cardWords(value, lang);
  };
  const perItem = draft.unit === "each" && (draft.qty ?? 0) > 1;
  const itemName = draft.item ? itemLabel(draft.item, namer, lang) : null;
  const owner = draft.trade === "owner" || serverTrade === "owner";
  const conds = conditions(draft.req, receivedOf(draft), namer, lang);
  const ready = questions.length === 0 && orderOf(draft, live) !== null;
  // A name the game does not know ("Kristal Kalkan") stays the card's title, quiet, while its
  // question waits.
  const mention = questions.find(
    (question): question is Extract<OrderQuestion, { kind: "custom" }> =>
      question.kind === "custom",
  )?.mention;
  // "Kaydet" says what is missing when pressed too early, as the form does (nothing is disabled).
  const [tried, setTried] = useState(false);
  // A figure a shown question asks for is left out until it is answered; one no question asks
  // says it is not given ("Yazılmadı"), never a bare "?".
  const asked = new Set(
    shown.map((question) =>
      question.kind === "qty" ? "qty" : question.kind === "limit" ? "limit" : null,
    ),
  );
  const figure = (value: number | null, of: (value: number) => string) =>
    value === null ? t("afkPlus.card.unset") : of(value);
  // "adet başına" changes what the figure means, so it comes before the figure's words.
  const underOf = (value: number | null) =>
    value === null ? null : perItem ? t("afkPlus.card.each") : words(value);
  const notes = card.answer.notes
    .map((note) => ({ note, text: t(`afkPlus.card.notes.${note}`, { defaultValue: "" }) }))
    .filter((one) => one.text);

  return (
    <div className="rounded-[var(--radius-panel)] bg-obsidian-800/70 p-3.5 ring-1 ring-white/8 ring-inset">
      <div className="flex items-start gap-3">
        {draft.item ? (
          <PlusItemIcon
            item={draft.item}
            custom={[...namer.custom, ...(draft.customItems ?? [])]}
            size={40}
          />
        ) : (
          // Not known yet: the same line cube as an unseen custom item's.
          <span
            aria-hidden
            className="chamfer grid size-[40px] shrink-0 place-items-center bg-obsidian-800 [--cut:6px]"
          >
            <Cube size={18} className="text-ash-400" />
          </span>
        )}
        <div className="min-w-0 flex-1">
          <CardField onEdit={() => onEdit("item")} label={t("afkPlus.card.editItem")}>
            <span className="flex items-baseline gap-2">
              <span
                className={cn(
                  "truncate font-display text-[15px] font-semibold",
                  itemName ? "text-bone-50" : "text-ash-300",
                )}
              >
                {itemName ?? mention ?? t("afkPlus.card.noItem")}
              </span>
              {draft.side ? (
                <span className="shrink-0 text-[12.5px] text-ash-300">
                  {t(draft.side === "buy" ? "afkPlus.card.toBuy" : "afkPlus.card.toSell")}
                </span>
              ) : null}
            </span>
          </CardField>
          <div className="mt-2 flex flex-wrap gap-x-6 gap-y-2">
            {draft.qty === null && asked.has("qty") ? null : (
              <CardField onEdit={() => onEdit("qty")} label={t("afkPlus.card.editQty")}>
                <Amount
                  label={t("afkPlus.card.qty")}
                  value={figure(draft.qty, (value) => formatAmount(value, lang))}
                  muted={draft.qty === null}
                />
              </CardField>
            )}
            <CardField onEdit={() => onEdit("target")} label={t("afkPlus.card.editTarget")}>
              <Amount
                label={t("afkPlus.card.target")}
                value={figure(draft.target, amountOf)}
                under={underOf(draft.target)}
                muted={draft.target === null}
              />
            </CardField>
            {draft.limit === null && asked.has("limit") ? null : (
              <CardField onEdit={() => onEdit("limit")} label={t("afkPlus.card.editLimit")}>
                <Amount
                  label={t(draft.side === "sell" ? "afkPlus.card.atLeast" : "afkPlus.card.atMost")}
                  value={figure(draft.limit, amountOf)}
                  under={underOf(draft.limit)}
                  muted={draft.limit === null}
                />
              </CardField>
            )}
          </div>
          <div className="mt-2 grid grid-cols-1 gap-x-6 gap-y-0.5 text-[12.5px] text-bone-100 min-[900px]:grid-cols-2">
            <CardField onEdit={() => onEdit("pay")} label={t("afkPlus.card.editPay")}>
              {payName
                ? t("afkPlus.card.forItem", { item: payName })
                : t(bare && !counts ? "afkPlus.card.forMoneyBare" : "afkPlus.card.forMoney")}
            </CardField>
            <CardField onEdit={() => onEdit("style")} label={t("afkPlus.card.editStyle")}>
              {t("afkPlus.card.style", {
                style: t(`afkPlus.form.styles.${draft.style}`).toLocaleLowerCase(
                  lang === "tr" ? "tr" : "en",
                ),
              })}
            </CardField>
            <CardField onEdit={() => onEdit("days")} label={t("afkPlus.card.editDays")}>
              {t("afkPlus.card.valid", { count: daysLeft(draft.until, nowSeconds()) })}
            </CardField>
            <CardField onEdit={() => onEdit("trade")} label={t("afkPlus.card.editTrade")}>
              {owner
                ? t(serverTrade === "owner" ? "afkPlus.card.ownerHere" : "afkPlus.card.owner")
                : t("afkPlus.card.bot")}
            </CardField>
            {conds.length ? (
              <CardField
                onEdit={() => onEdit("req")}
                label={t("afkPlus.card.editReq")}
                className="min-[900px]:col-span-2"
              >
                {conds.join(", ")}
              </CardField>
            ) : null}
          </div>
          {draft.note ? (
            <p className="mt-2 text-[12.5px] text-ash-400 [overflow-wrap:anywhere]">
              {/["“”]/.test(draft.note)
                ? t("afkPlus.card.wroteQuoted", { text: draft.note })
                : t("afkPlus.card.wrote", { text: draft.note })}
            </p>
          ) : null}
        </div>
      </div>

      {notes.length ? (
        <ul className="mt-3 flex flex-col gap-1">
          {notes.map(({ note, text }) => (
            <li key={note} className="flex items-start gap-2 text-[12.5px] text-ash-300">
              <Info size={14} className="mt-[2px] shrink-0 text-ash-400" aria-hidden />
              {text}
            </li>
          ))}
        </ul>
      ) : null}

      <AnimatePresence initial={false}>
        {shown.length ? (
          <motion.div
            key="questions"
            ref={firstQuestion}
            tabIndex={-1}
            className="mt-3 flex flex-col gap-3 border-t border-white/7 pt-3 outline-none"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.18) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            {shown.map((question) => (
              <div
                key={`${question.kind}-${"field" in question ? question.field : ""}`}
                className="flex flex-col gap-1.5"
              >
                <p className="text-[13px] font-medium text-bone-50">
                  {questionText(question, t, amountOf, draft)}
                </p>
                <QuestionChips
                  question={question}
                  title={questionText(question, t, amountOf, draft)}
                  // A price counted in the paying item ("Elmas") says so beside its field; a
                  // count of the order's own item does not.
                  unit={question.kind === "limit" && counts && payName ? payName : null}
                  ctx={live}
                  namer={namer}
                  money={amountOf}
                  counting={counts}
                  onChoice={(choice) => onChoice(question, choice)}
                />
              </div>
            ))}
            {/* "Kaydet" pressed early: said here, at the questions it waits for. */}
            {tried && !ready ? (
              <p role="alert" className="flex items-center gap-2 text-[12.5px] text-bone-100">
                <WarningCircle
                  size={14}
                  weight="fill"
                  aria-hidden
                  className="shrink-0 text-gold-300"
                />
                {t(shown.length > 1 ? "afkPlus.card.answerFirstMany" : "afkPlus.card.answerFirst")}
              </p>
            ) : null}
          </motion.div>
        ) : null}
      </AnimatePresence>

      {/* The card is the assistant's reading of the player's words: it can be wrong (27 Oct 2026). */}
      <p className="mt-3 text-[12px] text-ash-400">{t("afkPlus.aiNote")}</p>
      <div className="mt-2 flex justify-end gap-2">
        <Button size="sm" variant="ghost" onClick={onDiscard}>
          {t("afkPlus.card.discard")}
        </Button>
        <Button size="sm" variant="secondary" onClick={() => onEdit()}>
          {t("afkPlus.card.edit")}
        </Button>
        <Button
          size="sm"
          variant="primary"
          aria-disabled={saving ? true : undefined}
          onClick={() => {
            if (saving) return;
            if (!ready) {
              // The questions first: the first one gets the focus, the line says it.
              setTried(true);
              firstQuestion.current?.querySelector<HTMLElement>("button, input")?.focus();
              return;
            }
            onSave();
          }}
        >
          <SwapLabel
            labels={[t("afkPlus.card.save"), t("afkPlus.settings.saving")]}
            active={saving ? 1 : 0}
          />
        </Button>
      </div>
      {tried && !ready && !shown.length ? (
        <p role="alert" className="mt-2 text-right text-[12.5px] text-ash-300">
          {t("afkPlus.card.fixFirst")}
        </p>
      ) : null}
    </div>
  );
}

// --- the field ------------------------------------------------------------------------------------

/** Whether typed orders can be checked: on, switched off by Studio, or the machine is away. */
export type IntakeChecks = "on" | "off" | "away";

type IntakeLine =
  | { kind: "rule"; reason: string; rule?: string }
  // Money that may be the game's: asked with one button, "Bu oyun parası" (28 Oct 2026).
  | { kind: "money" }
  | { kind: "limited"; reason: "rate" | "daily" }
  | { kind: "unavailable"; reason: string }
  | { kind: "offline" }
  | { kind: "slow" }
  | { kind: "off" };

function intakeLineText(
  line: IntakeLine,
  t: (key: string, values?: Record<string, unknown>) => string,
  max: number,
) {
  switch (line.kind) {
    case "rule":
      if (line.reason === "loosen") {
        // The hard rule as a sentence ("ProBot AI bütçeni ve sınırlarını kimseye söylemez.").
        const rule = t(`afkPlus.intake.rules.${line.rule ?? "prepay"}`);
        const first = rule.charAt(0).toLocaleUpperCase(currentLang() === "tr" ? "tr" : "en");
        return t("afkPlus.intake.reject.loosen", { rule: first + rule.slice(1) });
      }
      if (line.reason === "orders_full")
        return t("afkPlus.intake.reject.orders_full", { count: max });
      return t(`afkPlus.intake.reject.${line.reason}`, {
        defaultValue: t("afkPlus.intake.reject.unsafe"),
      });
    case "money":
      return t("afkPlus.intake.money");
    case "limited":
      return t(line.reason === "daily" ? "afkPlus.intake.daily" : "afkPlus.intake.rate");
    case "unavailable":
      return t(
        line.reason === "paused" || line.reason === "off"
          ? "afkPlus.intake.aiPaused"
          : "afkPlus.intake.aiAway",
      );
    case "offline":
      return t("afkPlus.intake.offline");
    case "slow":
      return t("afkPlus.intake.aiAway");
    case "off":
      return t("afkPlus.intake.off");
  }
}

function OrderIntake({
  accountId,
  ctx,
  namer,
  server,
  active,
  orderMax,
  checksLeft,
  checks,
  canPick,
  disabledLine,
  awaySaid = false,
  aiDown = false,
  onForm,
  onPick,
  onCard,
  onSaved,
  quietLabel = false,
}: {
  accountId: string;
  ctx: DraftContext;
  namer: Namer;
  server: PlusServer;
  active: number;
  orderMax: number;
  checksLeft: number | null;
  /** The field's question is said above it already (the setup's step): only screen readers hear it. */
  quietLabel?: boolean;
  /** Typed orders: on, switched off (the field is not shown), or the machine cannot be asked. */
  checks: IntakeChecks;
  canPick: boolean;
  /** Why nothing can be added now (maintenance, the cap), or null. */
  disabledLine: string | null;
  /** The panel's notice above already says the machine is away (the field says what works). */
  awaySaid?: boolean;
  /** The AI is out (`paused`, `outage`): "Kontrol et" waits, the form works. */
  aiDown?: boolean;
  onForm: (request: FormRequest) => void;
  onPick: (field: "item" | "pay", then: (picked: PickedItem) => void) => void;
  /** The card is up (the wizard moves on when it is saved). */
  onCard?: (open: boolean) => void;
  /** The card's order is saved. */
  onSaved?: () => void;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  // The example's amount with the server's money name, where its profile names one.
  const money = moneyNameOf(server, lang);
  const { layout } = useMotionPrefs();
  const id = useId();
  const writes = useOrderWrites(accountId);
  const maxDeal = server.money.maxDeal;
  // The server's money name in the language "Kontrol et" asks in (the launcher's): an amount
  // written with it is no real money ("50 bin TL" where the money is TL).
  const label = server.money.label;
  const currency = label === null ? null : lang === "en" ? label.en : label.tr;
  const [text, setText] = useState("");
  const [checking, setChecking] = useState(false);
  const [line, setLine] = useState<IntakeLine | null>(null);
  const [card, setCard] = useState<CardState | null>(null);
  const [saving, setSaving] = useState(false);
  const [left, setLeft] = useState<number | null>(checksLeft);
  useEffect(() => setLeft(checksLeft), [checksLeft]);
  useEffect(() => onCard?.(card !== null), [card, onCard]);
  // After an answer that the AI cannot be asked, "Kontrol et" waits until the text changes.
  const canCheck = checks === "on" && !aiDown && line?.kind !== "unavailable";
  // One save of the card at a time (a double click lands before the disabled button renders).
  const savingNow = useRef(false);

  // The free rules as a hint while typing (the service checks again on "Kontrol et"); money that
  // may be the game's is asked on "Kontrol et", not while typing.
  const { hint, moneyAsked } = useMemo(() => {
    const trimmed = text.trim();
    if (!trimmed) return { hint: null, moneyAsked: false };
    const check = checkOrderText(trimmed, { maxDeal, active, orderMax, currency });
    const rejected = ruleHint(check);
    const asked = check.result === "question:rmt";
    // Too short is no news while the player is still typing; the button says it by waiting.
    if (!rejected || rejected.reason === "length") {
      const long = [...trimmed].length > 200;
      const tooLong: IntakeLine | null = long ? { kind: "rule", reason: "length" } : null;
      return { hint: tooLong, moneyAsked: asked };
    }
    const ruled = { kind: "rule", reason: rejected.reason, rule: rejected.rule } as IntakeLine;
    return { hint: ruled, moneyAsked: asked };
  }, [text, maxDeal, active, orderMax, currency]);
  const shownLine = line ?? hint;
  const enough = [...text.trim()].length >= 3;
  const outOfChecks = left !== null && left <= 0;
  const full = active >= orderMax;
  const blocked = Boolean(disabledLine) || full;
  // Nothing typed here can be checked now: the field rests, the line says what works.
  const resting = blocked || checks === "away" || aiDown;

  // `inGame`: the player said the words' money is the game's ("Bu oyun parası"); the same words go
  // again with it, and the rails still refuse.
  const check = async (event?: FormEvent, inGame = false) => {
    event?.preventDefault();
    if (!enough || checking || !canCheck || outOfChecks || blocked) return;
    if (hint && hint.kind === "rule") {
      setLine(hint);
      return;
    }
    if (moneyAsked && !inGame) {
      setLine({ kind: "money" });
      return;
    }
    setChecking(true);
    setLine(null);
    const slow = window.setTimeout(() => setLine({ kind: "slow" }), SLOW_CHECK_MS);
    try {
      const outcome = await plusApi.check(accountId, text, inGame);
      window.clearTimeout(slow);
      switch (outcome.kind) {
        case "draft": {
          const parsed = JSON.parse(outcome.answer) as CheckAnswer;
          setCard({ answer: parsed, draft: parsed.draft, chip: {}, text });
          setLeft(parsed.left);
          setLine(null);
          break;
        }
        case "rejected":
          setLine(
            isMoneyQuestion(outcome.reason, outcome.rule)
              ? { kind: "money" }
              : { kind: "rule", reason: outcome.reason, rule: outcome.rule ?? undefined },
          );
          break;
        case "limited":
          setLine({ kind: "limited", reason: outcome.reason === "daily" ? "daily" : "rate" });
          if (outcome.reason === "daily") setLeft(0);
          break;
        case "unavailable":
          setLine({ kind: "unavailable", reason: outcome.reason });
          break;
        case "offline":
          setLine({ kind: "offline" });
          break;
        case "off":
          setLine({ kind: "off" });
          break;
      }
    } catch (error) {
      window.clearTimeout(slow);
      setLine(null);
      afkFailed(error, t("afkPlus.intake.failed"));
    } finally {
      setChecking(false);
    }
  };

  const clearCard = () => {
    setCard(null);
    setText("");
    setLine(null);
  };

  const seedOf = (draft: OrderDraft): FormRequest => ({
    kind: "new",
    onSaved: clearCard,
    seed: {
      side: draft.side ?? undefined,
      item: draft.item ?? undefined,
      qty: draft.qty ?? undefined,
      pay: draft.pay ?? undefined,
      unit: draft.unit ?? undefined,
      target: draft.target ?? undefined,
      limit: draft.limit ?? undefined,
      style: draft.style,
      trade: draft.trade,
      until: draft.until,
      req: draft.req
        ? {
            ench: draft.req.ench,
            dur: draft.req.dur,
            named: draft.req.named,
            cursed: draft.req.cursed,
          }
        : undefined,
      note: draft.note,
      src: "text",
      custom: draft.customItems ?? [],
    },
  });

  const choose = (question: OrderQuestion, choice: Choice) => {
    if (!card) return;
    const live = withChip(ctx, card.chip);
    const key = newCustomKey([...ctx.custom, ...(card.draft.customItems ?? [])]);
    const result: AnswerResult = answer(card.draft, question, choice, live, key);
    if ("form" in result) {
      onForm({ ...seedOf(card.draft), focus: result.form });
      return;
    }
    if ("pick" in result) {
      const field = result.pick;
      onPick(field, (picked) => {
        const draft = applyPick(card.draft, field, picked, live);
        if (picked.kind === "custom" && !picked.custom.name.trim()) {
          onForm({ ...seedOf(draft), focus: "name" });
          return;
        }
        setCard((current) => (current ? { ...current, draft } : current));
      });
      return;
    }
    setCard({
      ...card,
      draft: result.draft,
      chip: {
        ...card.chip,
        ...(result.settings?.pay ? { pay: result.settings.pay } : {}),
        ...(result.settings?.limits ? { perDealMax: result.settings.limits.perDealMax } : {}),
      },
    });
  };

  const save = async () => {
    if (!card || savingNow.current) return;
    const live = withChip(ctx, card.chip);
    const order = orderOf(card.draft, live);
    if (!order) return;
    savingNow.current = true;
    setSaving(true);
    const outcome = await writes.create(
      order,
      (card.draft.customItems ?? []).map(toNewCustom),
      card.chip,
    );
    savingNow.current = false;
    setSaving(false);
    if (!outcome) return;
    if (outcome.kind === "saved") {
      toast.success(t("afkPlus.orders.saved", { item: itemLabel(order.item, namer, lang) }));
      clearCard();
      onSaved?.();
    } else if (outcome.kind === "invalid") {
      // The form says where, with the card's order in it.
      onForm({ ...seedOf(card.draft), issues: outcome.issues });
    }
  };

  // Why "Kontrol et" waits, when nothing else is said: the machine is away, today's checks are
  // used.
  const passiveLine =
    checks === "away"
      ? t(awaySaid ? "afkPlus.intake.formOnly" : "afkPlus.intake.offline")
      : aiDown
        ? t("afkPlus.intake.aiAway")
        : outOfChecks
          ? t("afkPlus.intake.daily")
          : null;
  const ruleLine = !disabledLine && !full && shownLine?.kind === "rule";
  const moneyLine = !disabledLine && !full && shownLine?.kind === "money";
  const lineText =
    disabledLine ??
    (full
      ? t("afkPlus.intake.reject.orders_full", { count: orderMax })
      : shownLine
        ? intakeLineText(shownLine, t, orderMax)
        : passiveLine);

  const links = (
    <div className="-mt-0.5 flex flex-wrap gap-x-4 gap-y-1">
      <QuietLink disabled={blocked} onClick={() => onForm({ kind: "new" })}>
        {t("afkPlus.intake.form")}
      </QuietLink>
      <QuietLink
        disabled={!canPick || blocked}
        onClick={() =>
          onPick("item", (picked) =>
            onForm({
              kind: "new",
              seed: seedOfPick(picked),
              focus: picked.kind === "custom" && !picked.custom.name.trim() ? "name" : "side",
            }),
          )
        }
      >
        {t("afkPlus.intake.pick")}
      </QuietLink>
    </div>
  );

  return (
    <motion.section
      layout={layout ? "position" : false}
      aria-labelledby={checks === "off" && !card ? undefined : `${id}-label`}
      aria-label={checks === "off" && !card ? t("afkPlus.panel.sections.orders") : undefined}
    >
      <AnimatePresence initial={false} mode="popLayout">
        {card ? (
          <motion.div
            key="card"
            layout={layout ? "position" : false}
            initial={{ opacity: 0, transform: "translateY(4px)" }}
            animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.15) }}
          >
            <p
              id={`${id}-label`}
              className={cn(
                "mb-1.5 text-[13px] font-medium text-bone-100",
                quietLabel && "sr-only",
              )}
            >
              {t("afkPlus.intake.label")}
            </p>
            <OrderCard
              card={card}
              ctx={ctx}
              namer={namer}
              server={server}
              saving={saving}
              onChoice={choose}
              onEdit={(field) => onForm({ ...seedOf(card.draft), focus: field })}
              onDiscard={() => setCard(null)}
              onSave={() => void save()}
            />
          </motion.div>
        ) : checks === "off" ? (
          // Typed orders are switched off: the form and the inventory only.
          <motion.div
            key="links"
            layout={layout ? "position" : false}
            className="flex flex-col gap-1.5"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            {disabledLine || full ? (
              <p
                role="status"
                className="flex min-w-0 items-start gap-1.5 text-[12px] leading-[18px] text-ash-300"
              >
                <WarningCircle
                  size={14}
                  weight="fill"
                  aria-hidden
                  className="mt-[2px] shrink-0 text-gold-300"
                />
                <span>
                  {disabledLine ?? t("afkPlus.intake.reject.orders_full", { count: orderMax })}
                </span>
              </p>
            ) : null}
            {links}
          </motion.div>
        ) : (
          <motion.form
            key="field"
            layout={layout ? "position" : false}
            onSubmit={(event: FormEvent) => void check(event)}
            className="flex flex-col gap-1.5"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            <label
              htmlFor={id}
              id={`${id}-label`}
              className={cn("text-[13px] font-medium text-bone-100", quietLabel && "sr-only")}
            >
              {t("afkPlus.intake.label")}
            </label>
            <div
              className={cn(
                "flex items-center gap-1 rounded-md border bg-obsidian-800/80 p-[3px] pl-0 transition-[border-color,box-shadow,opacity] duration-150",
                "border-white/10 hover:border-white/18 focus-within:border-gold-400/60 focus-within:ring-2 focus-within:ring-gold-400/20",
                resting && "opacity-55",
              )}
            >
              <input
                id={id}
                value={text}
                maxLength={260}
                autoComplete="off"
                spellCheck={false}
                readOnly={checking}
                disabled={resting}
                placeholder={
                  money
                    ? t("afkPlus.intake.placeholderMoney", { money })
                    : t("afkPlus.intake.placeholder")
                }
                aria-describedby={`${id}-line`}
                onChange={(event) => {
                  setText(event.target.value);
                  setLine(null);
                }}
                className="h-[30px] min-w-0 flex-1 bg-transparent pl-3 text-[13.5px] text-bone-50 outline-none placeholder:text-ash-500 disabled:cursor-not-allowed"
              />
              <Button
                type="submit"
                size="sm"
                variant={enough && canCheck && !blocked ? "primary" : "secondary"}
                disabled={!enough || checking || !canCheck || outOfChecks || blocked}
              >
                <SwapLabel
                  labels={[t("afkPlus.intake.check"), t("afkPlus.intake.reading")]}
                  active={checking ? 1 : 0}
                />
              </Button>
            </div>
            <div
              id={`${id}-line`}
              className="flex min-h-[18px] flex-wrap items-start justify-between gap-x-3 text-[12px] leading-[18px]"
            >
              <p
                role={lineText ? "status" : undefined}
                className="flex min-w-0 items-start gap-1.5 text-ash-300"
              >
                {lineText ? (
                  <>
                    <WarningCircle
                      size={14}
                      weight="fill"
                      aria-hidden
                      className={cn(
                        "mt-[2px] shrink-0",
                        ruleLine ? "text-flare-400" : "text-gold-300",
                      )}
                    />
                    {moneyLine ? (
                      // The question's one answer, quiet (the gold is "Kontrol et"'s): the same
                      // words again as the game's money; the hard rules stay.
                      <span className="flex min-w-0 flex-wrap items-baseline gap-x-2">
                        <span>{lineText}</span>
                        <QuietLink
                          tone="bone"
                          disabled={checking}
                          onClick={() => void check(undefined, true)}
                        >
                          {t("afkPlus.intake.inGame")}
                        </QuietLink>
                      </span>
                    ) : (
                      <span>{lineText}</span>
                    )}
                  </>
                ) : null}
              </p>
              {left !== null && left <= 5 && !blocked && canCheck ? (
                <p className="shrink-0 text-ash-400 tabular-nums">
                  {left > 0 ? t("afkPlus.intake.left", { count: left }) : null}
                </p>
              ) : null}
            </div>
            {links}
          </motion.form>
        )}
      </AnimatePresence>
    </motion.section>
  );
}

function toNewCustom(item: CustomItemLike): PlusNewCustomItem {
  return {
    k: item.k,
    name: item.name,
    base: item.base,
    fp: item.fp,
    look: item.look ? { cmd: item.look.cmd ?? null, model: item.look.model ?? null } : null,
    from: item.from,
  };
}

/** A picked item put into a card's draft (the item, or what pays for it). */
function applyPick(
  draft: OrderDraft,
  field: "item" | "pay",
  picked: PickedItem,
  ctx: DraftContext,
): OrderDraft {
  const next: OrderDraft = structuredClone(draft);
  if (picked.kind === "vanilla") {
    next[field] = picked.ref;
    if (receivedOf({ ...next }) === picked.ref && next.req) {
      next.req = { ...next.req, ench: picked.req.ench, dur: picked.req.dur };
    }
  } else if (picked.kind === "known") {
    next[field] = picked.ref;
  } else {
    // A key no custom item of the account or of the card uses.
    const own = next.customItems ?? [];
    const custom = own.some((item) => item.k === picked.custom.k)
      ? { ...picked.custom, k: newCustomKey([...ctx.custom, ...own]) }
      : picked.custom;
    next[field] = `custom:${custom.k}`;
    next.customItems = [...own, custom];
  }
  delete next.open[field];
  return fill(next, ctx);
}

/** A picked item as the form's start: the item, its conditions, sold by default (it is in hand). */
function seedOfPick(picked: PickedItem): FormSeed {
  if (picked.kind === "vanilla") {
    return { item: picked.ref, side: "sell", req: picked.req, src: "inv", custom: [] };
  }
  if (picked.kind === "known") return { item: picked.ref, side: "sell", src: "inv", custom: [] };
  return {
    item: `custom:${picked.custom.k}`,
    side: "sell",
    src: "inv",
    custom: [picked.custom],
  };
}

// --- the list -------------------------------------------------------------------------------------

// What waits for the player is the brightest word in the row, not gold (the approval under it
// says what it is).
const STATUS_TONE: Record<string, string> = {
  waiting_ok: "text-bone-50",
  your_trade: "text-bone-50",
  trading: "text-bone-50",
  haggling: "text-bone-50",
  looking: "text-ash-300",
  side_off: "text-ash-400",
  paused: "text-ash-400",
  done: "text-ash-300",
  expired: "text-ash-400",
  service: "text-ash-400",
};

function OrderRow({
  accountId,
  order,
  state,
  namer,
  approvals,
  server,
  canLower,
  maintenance,
  onEdit,
  onPause,
  onExtend,
  onRestart,
  onRemove,
  onJoin,
}: {
  accountId: string;
  order: PlusOrder;
  state: OrderState;
  namer: Namer;
  /** This order's pending approvals (a deal, a handoff, a first sight). */
  approvals: PlusApproval[];
  server: PlusServer;
  canLower: boolean;
  /** AFK+ is under maintenance (`plusMode: paused`): nothing moves. */
  maintenance: boolean;
  onEdit: () => void;
  onPause: (on: boolean) => void;
  onExtend: () => void;
  onRestart: () => void;
  onRemove: () => void;
  onJoin: () => void;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const lang = currentLang();
  const name = itemLabel(order.item, namer, lang);
  const finished = state.status === "done" || state.status === "expired";
  // A paused or finished order rests: its picture dims and its words go a step quieter, each still
  // AA on the panel (the whole row at 60 % put "Süresi doldu" near 3:1; afk.projects.gg, SITE-W2).
  const resting = !order.on || finished;
  const counts = order.pay !== "money";
  const payName = counts ? itemLabel(order.pay, namer, lang) : null;
  const price = (value: number) =>
    payName ? qtyItem(value, payName, lang) : moneyText(value, server, lang);
  const days = daysLeft(order.until, nowSeconds());
  const limit = t(
    order.side === "buy" ? "afkPlus.orders.metaAtMost" : "afkPlus.orders.metaAtLeast",
    { price: price(order.limit) },
  );
  const meta = [
    t(order.side === "buy" ? "afkPlus.card.toBuy" : "afkPlus.card.toSell"),
    // "tanesi en fazla 4.500" / "toplam en fazla 9.000": what the price counts.
    order.qty > 1
      ? t(order.unit === "each" ? "afkPlus.orders.perPiece" : "afkPlus.orders.inTotal", {
          terms: limit,
        })
      : limit,
    !finished && days <= 3
      ? days <= 1
        ? t("afkPlus.orders.endsToday")
        : t("afkPlus.orders.daysLeft", { count: days })
      : null,
  ]
    .filter(Boolean)
    .join(", ");
  // What waits under the row says the state too: the machine's last word can be older than the
  // approval (it is away), and "Aranıyor" over an open approval reads wrong. Under maintenance
  // nothing moves; "waits for you" with nothing waiting is an old word.
  const shownStatus = approvals.some((a) => a.kind === "handoff")
    ? "your_trade"
    : approvals.length
      ? "waiting_ok"
      : maintenance && !finished
        ? "service"
        : state.status === "waiting_ok" || state.status === "your_trade"
          ? "looking"
          : state.status;
  const status =
    shownStatus === "side_off"
      ? t(order.side === "buy" ? "afkPlus.status.buyOff" : "afkPlus.status.sellOff")
      : shownStatus === "looking" && order.side === "sell"
        ? t("afkPlus.status.lookingSell")
        : t(`afkPlus.status.${shownStatus}`, { defaultValue: t("afkPlus.status.looking") });
  const items: MenuItem[] = finished
    ? [
        {
          label: t("afkPlus.orders.restart"),
          icon: <ArrowClockwise size={16} />,
          onSelect: onRestart,
        },
        {
          label: t("afkPlus.orders.remove"),
          icon: <Trash size={16} />,
          onSelect: onRemove,
          danger: true,
        },
      ]
    : [
        { label: t("afkPlus.orders.edit"), icon: <PencilSimple size={16} />, onSelect: onEdit },
        order.on
          ? {
              label: t("afkPlus.orders.pause"),
              icon: <Pause size={16} />,
              onSelect: () => onPause(false),
            }
          : {
              label: t("afkPlus.orders.resume"),
              icon: <Play size={16} />,
              onSelect: () => onPause(true),
            },
        { label: t("afkPlus.orders.extend"), icon: <Timer size={16} />, onSelect: onExtend },
        {
          label: t("afkPlus.orders.remove"),
          icon: <Trash size={16} />,
          onSelect: onRemove,
          danger: true,
        },
      ];
  return (
    <motion.li
      layout={layout ? "position" : false}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: enter(0.22) }}
      exit={{ opacity: 0, transition: leave(0.15) }}
    >
      <ContextMenu items={items}>
        <div className="flex items-center gap-2 rounded-md py-1.5 pr-1 pl-1.5 transition-colors duration-150 hover:bg-white/[0.03]">
          <button
            type="button"
            onClick={finished ? onRestart : onEdit}
            aria-label={t(finished ? "afkPlus.orders.restartLabel" : "afkPlus.orders.editLabel", {
              item: name,
            })}
            className={cn(
              "flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-md py-1 text-left outline-none",
              "transition-[transform,scale] duration-150 ease-out active:scale-[0.99] focus-visible:ring-2 focus-visible:ring-gold-300",
            )}
          >
            <span className={cn("inline-flex shrink-0", resting && "opacity-60")}>
              <PlusItemIcon item={order.item} custom={namer.custom} size={32} />
            </span>
            <span className="min-w-0 flex-1">
              <span
                className={cn(
                  "block truncate text-[13.5px] font-medium",
                  resting ? "text-ash-300" : "text-bone-50",
                )}
              >
                {name}
              </span>
              {/* The terms wrap rather than lose their end (the time left; the site's, TASTE-SWEEP r1). */}
              <span
                className={cn(
                  "block text-[12px] text-pretty tabular-nums",
                  resting ? "text-ash-400" : "text-ash-300",
                )}
              >
                {meta}
              </span>
            </span>
            <span className="shrink-0 text-right">
              <span
                className={cn(
                  "block text-[13px] tabular-nums",
                  resting ? "text-ash-300" : "text-bone-100",
                )}
              >
                {t(
                  order.side === "sell"
                    ? "afkPlus.orders.progressSold"
                    : "afkPlus.orders.progressBought",
                  { got: Math.min(state.got, order.qty), qty: order.qty },
                )}
              </span>
              <span className={cn("block text-[12px]", STATUS_TONE[shownStatus])}>{status}</span>
            </span>
          </button>
          <Menu
            trigger={
              <IconButton
                label={t("afkPlus.orders.more", { item: name })}
                icon={<DotsThreeVertical size={18} weight="bold" />}
                size="sm"
              />
            }
            items={items}
          />
        </div>
      </ContextMenu>
      <AnimatePresence initial={false}>
        {approvals.map((approval) => (
          <motion.div
            key={approval.id}
            layout={layout ? "position" : false}
            className="mt-1 mr-1.5 mb-2 ml-[46px]"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.15) }}
          >
            <ApprovalCard
              accountId={accountId}
              approval={approval}
              namer={namer}
              server={server}
              canLower={canLower}
              maintenance={maintenance}
              onJoin={onJoin}
              compact
            />
          </motion.div>
        ))}
      </AnimatePresence>
    </motion.li>
  );
}

export function OrdersSection({ account, panel }: { account: CloudAccount; panel: PlusPanel }) {
  const { t } = useTranslation();
  const lang = currentLang();
  const { layout } = useMotionPrefs();
  const features = usePlusFeatures();
  const queryClient = useQueryClient();
  const writes = useOrderWrites(account.id);
  const settings = panel.settings;
  const { config, server, rules } = settings;
  const items = usePlusItemData(server.mc);
  const namer: Namer = {
    data: items.data,
    custom: config.customItems,
    unnamed: t("afkPlus.items.unnamed"),
  };
  const ctx = draftContextOf(config, server, rules, items.data);
  const now = nowSeconds();
  const states = new Map(
    config.orders.map((order) => [
      order.id,
      orderState(order, settings.orders[order.id], config.allow, now),
    ]),
  );
  // What still waits (its time not run out), read again when one runs out: it shows under its
  // order's row and puts that order first.
  const approvalNow = useApprovalNow(panel.approvals);
  const pending = panel.approvals.filter((a) => waitsNow(a, approvalNow));
  const { active, finished } = sortedOrders(
    config.orders,
    states,
    new Set(pending.map((a) => a.order)),
  );
  const activeOrders = activeCount(config.orders, states);
  const ai = plusAi(account);
  const away = panel.machineAway || ai?.state === "unknown";
  const maintenance = ai?.reason === "service" && ai.state === "paused";
  const [form, setForm] = useState<FormRequest | null>(null);
  const [picking, setPicking] = useState<{
    field: "item" | "pay";
    then: (picked: PickedItem) => void;
  } | null>(null);
  const approvalsOf = (order: string) => pending.filter((a) => a.order === order);

  // A row's action refused by the service says why (the form shows its refusals on its fields).
  const refused = (outcome: PlusOrderOutcome | null, fallback: string) => {
    if (outcome?.kind !== "invalid") return;
    const cap = outcome.issues.find((issue) => issue.reason === "limit_orders");
    toast.error(
      cap
        ? t(cap.problem === "stored" ? "afkPlus.form.errors.stored" : "afkPlus.form.errors.active")
        : t(fallback),
    );
  };

  const remove = (order: PlusOrder) => {
    void writes.remove(order.id).then((outcome) => {
      if (outcome?.kind !== "saved") return;
      if (outcome.deferred) {
        toast(t("afkPlus.orders.removeDeferred"));
        return;
      }
      toast(t("afkPlus.orders.removed"), {
        duration: 10_000,
        action: {
          label: t("afkPlus.orders.undo"),
          onClick: () =>
            void writes
              .create(order, [], {})
              .then((back) => refused(back, "afkPlus.orders.undoFailed")),
        },
      });
    });
  };

  const pause = (order: PlusOrder, on: boolean) => {
    // Shown at once; the stored settings come back with the answer (or the old ones on a failure).
    queryClient.setQueryData<PlusPanel>(plusKey(account.id), (current) =>
      current
        ? {
            ...current,
            settings: {
              ...current.settings,
              config: {
                ...current.settings.config,
                orders: current.settings.config.orders.map((o) =>
                  o.id === order.id ? { ...o, on } : o,
                ),
              },
            },
          }
        : current,
    );
    void writes.patch(order.id, emptyPatch({ on })).then((outcome) => {
      if (outcome?.kind === "saved") return;
      void queryClient.invalidateQueries({ queryKey: plusKey(account.id) });
      refused(outcome, on ? "afkPlus.orders.resumeFailed" : "afkPlus.orders.failed");
    });
  };

  const extend = (order: PlusOrder) => {
    void writes.patch(order.id, emptyPatch({ extend: true })).then((outcome) => {
      if (outcome?.kind === "saved") {
        toast.success(t("afkPlus.orders.extended", { count: rules.orderDaysDefault }));
      } else {
        refused(outcome, "afkPlus.orders.extendFailed");
      }
    });
  };

  const restart = (order: PlusOrder) => {
    setForm({
      kind: "new",
      seed: {
        side: order.side === "sell" ? "sell" : "buy",
        item: order.item,
        qty: order.qty,
        pay: order.pay,
        unit: order.unit === "each" ? "each" : "total",
        target: order.target,
        limit: order.limit,
        style:
          (["patient", "balanced", "quick"] as const).find((s) => s === order.style) ?? "balanced",
        trade: order.trade === "owner" ? "owner" : "auto",
        req: order.req ?? undefined,
        note: order.note,
        src: order.src === "inv" ? "inv" : order.src === "text" ? "text" : "form",
        custom: [],
      },
    });
  };

  const join = useJoinForTrade(account);
  const disabledLine = maintenance ? t("afkPlus.intake.maintenance") : null;

  return (
    <div className="flex flex-col gap-7">
      <OrderIntake
        accountId={account.id}
        ctx={ctx}
        namer={namer}
        server={server}
        active={activeOrders}
        orderMax={rules.orderMax}
        checksLeft={rules.checksLeft}
        checks={!features.orders || !rules.features.orders ? "off" : away ? "away" : "on"}
        canPick={!away}
        disabledLine={disabledLine}
        awaySaid
        aiDown={ai?.state === "paused" && ai.reason === "outage"}
        onForm={setForm}
        onPick={(field, then) => setPicking({ field, then })}
      />

      <motion.section
        layout={layout ? "position" : false}
        aria-labelledby={active.length ? "afk-plus-active" : undefined}
      >
        {/* The title only over orders: no "Etkin" over an empty list (the site's, TASTE-SWEEP r1). */}
        {active.length ? (
          <GroupTitle
            id="afk-plus-active"
            aside={
              away && settings.asOf ? (
                <span className="text-[12px] text-ash-400">
                  {t("afkPlus.orders.asOf", { time: clockOf(settings.asOf, lang) })}
                </span>
              ) : null
            }
          >
            {t("afkPlus.orders.active")}
          </GroupTitle>
        ) : null}
        {active.length ? (
          <ul className="-mx-1.5 flex flex-col">
            <AnimatePresence initial={false}>
              {active.map((order) => {
                const state = states.get(order.id) as OrderState;
                return (
                  <OrderRow
                    key={order.id}
                    accountId={account.id}
                    order={order}
                    state={state}
                    namer={namer}
                    approvals={approvalsOf(order.id)}
                    server={server}
                    canLower={ai?.state === "on"}
                    maintenance={maintenance}
                    onEdit={() => setForm({ kind: "edit", order })}
                    onPause={(on) => pause(order, on)}
                    onExtend={() => extend(order)}
                    onRestart={() => restart(order)}
                    onRemove={() => remove(order)}
                    onJoin={join.ask}
                  />
                );
              })}
            </AnimatePresence>
          </ul>
        ) : (
          // Neutral, one line, no sprite (afkplus-free.md §7.1); with finished orders under it, the
          // line says there is no active one.
          <p className="py-1.5 text-[13px] text-ash-300">
            {t(finished.length ? "afkPlus.orders.noneActive" : "afkPlus.orders.empty")}
          </p>
        )}
      </motion.section>

      <AnimatePresence initial={false}>
        {finished.length ? (
          <motion.section
            key="finished"
            layout={layout ? "position" : false}
            aria-labelledby="afk-plus-finished"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.15) }}
          >
            <GroupTitle id="afk-plus-finished">{t("afkPlus.orders.finished")}</GroupTitle>
            <ul className="-mx-1.5 flex flex-col">
              <AnimatePresence initial={false}>
                {finished.map((order) => (
                  <OrderRow
                    key={order.id}
                    accountId={account.id}
                    order={order}
                    state={states.get(order.id) as OrderState}
                    namer={namer}
                    approvals={approvalsOf(order.id)}
                    server={server}
                    canLower={false}
                    maintenance={maintenance}
                    onEdit={() => setForm({ kind: "edit", order })}
                    onPause={(on) => pause(order, on)}
                    onExtend={() => extend(order)}
                    onRestart={() => restart(order)}
                    onRemove={() => remove(order)}
                    onJoin={join.ask}
                  />
                ))}
              </AnimatePresence>
            </ul>
          </motion.section>
        ) : null}
      </AnimatePresence>

      <OrderForm
        accountId={account.id}
        request={form}
        settings={settings}
        namer={namer}
        items={items}
        canPick={!away}
        onPick={(field, then) => setPicking({ field, then })}
        onClose={() => setForm(null)}
      />
      <InventoryPick
        account={account}
        open={picking !== null}
        customItems={config.customItems}
        onClose={() => setPicking(null)}
        onPicked={(picked) => {
          const then = picking?.then;
          setPicking(null);
          then?.(picked);
        }}
      />
      {join.dialog}
    </div>
  );
}

/**
 * The order field with its form and inventory pick, without the list: the setup's first step ("Ne
 * alıp satsın?", which asks the field's question itself, so its label is for screen readers only).
 * `onSaved`: an order was saved (by the card or the form).
 */
export function OrderStarter({
  account,
  panel,
  limits,
  onSaved,
}: {
  account: CloudAccount;
  panel: PlusPanel;
  /** Limits not saved yet (the setup's step 3): the card asks against these. */
  limits?: DraftContext["limits"];
  onSaved?: () => void;
}) {
  const { t } = useTranslation();
  const features = usePlusFeatures();
  const settings = panel.settings;
  const { config, server, rules } = settings;
  const items = usePlusItemData(server.mc);
  const namer: Namer = {
    data: items.data,
    custom: config.customItems,
    unnamed: t("afkPlus.items.unnamed"),
  };
  const stored = draftContextOf(config, server, rules, items.data);
  const ctx = limits ? { ...stored, limits } : stored;
  const now = nowSeconds();
  const states = new Map(
    config.orders.map((order) => [
      order.id,
      orderState(order, settings.orders[order.id], config.allow, now),
    ]),
  );
  const ai = plusAi(account);
  const away = panel.machineAway || ai?.state === "unknown";
  const maintenance = ai?.reason === "service" && ai.state === "paused";
  const [form, setForm] = useState<FormRequest | null>(null);
  const [picking, setPicking] = useState<{
    field: "item" | "pay";
    then: (picked: PickedItem) => void;
  } | null>(null);
  const withSaved = (request: FormRequest): FormRequest =>
    request.kind === "new"
      ? {
          ...request,
          onSaved: () => {
            request.onSaved?.();
            onSaved?.();
          },
        }
      : request;
  return (
    <>
      <OrderIntake
        accountId={account.id}
        ctx={ctx}
        namer={namer}
        server={server}
        active={activeCount(config.orders, states)}
        orderMax={rules.orderMax}
        checksLeft={rules.checksLeft}
        checks={!features.orders || !rules.features.orders ? "off" : away ? "away" : "on"}
        canPick={!away}
        disabledLine={maintenance ? t("afkPlus.intake.maintenance") : null}
        aiDown={ai?.state === "paused" && ai.reason === "outage"}
        onForm={(request) => setForm(withSaved(request))}
        onPick={(field, then) => setPicking({ field, then })}
        onSaved={onSaved}
        quietLabel
      />
      <OrderForm
        accountId={account.id}
        request={form}
        settings={settings}
        namer={namer}
        items={items}
        canPick={!away}
        onPick={(field, then) => setPicking({ field, then })}
        onClose={() => setForm(null)}
      />
      <InventoryPick
        account={account}
        open={picking !== null}
        customItems={config.customItems}
        onClose={() => setPicking(null)}
        onPicked={(picked) => {
          const then = picking?.then;
          setPicking(null);
          then?.(picked);
        }}
      />
    </>
  );
}

/** A patch with nothing in it but what is given. */
export function emptyPatch(
  parts: Partial<Parameters<typeof plusApi.patchOrder>[2]>,
): Parameters<typeof plusApi.patchOrder>[2] {
  return {
    on: null,
    qty: null,
    req: null,
    noReq: false,
    pay: null,
    unit: null,
    target: null,
    limit: null,
    style: null,
    trade: null,
    until: null,
    extend: false,
    ...parts,
  };
}

function clockOf(at: number, lang: Lang): string {
  return new Date(at * 1000).toLocaleTimeString(lang === "en" ? "en-GB" : "tr-TR", {
    hour: "2-digit",
    minute: "2-digit",
  });
}
