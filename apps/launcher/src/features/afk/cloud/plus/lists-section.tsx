// "Listeler": what the assistant buys and sells. On top, a rule typed in the player's own words
// (QoL Q2): the service reads it into a draft card (the item, how many, the amounts in figures and
// words, the style, when it asks), and nothing is saved before "Listeye ekle". Then the two lists:
// a row per rule with its amounts, its switch and a "…" menu (also on right click); "Düzenle" opens
// the rule in a dialog. List edits are saved at once (shown at once, put back if the save fails);
// a removed rule comes back from the toast. Things that come and go fade in place and the rows
// below glide (not in Performance mode).
import { DotsThreeVertical, PencilSimple, Plus, Trash, WarningCircle } from "@phosphor-icons/react";
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
  Switch,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, type ReactNode, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed } from "../../errors";
import type { CloudAccount } from "../api";
import {
  type PlusBuyRow,
  type PlusCatalog,
  type PlusLimits,
  type PlusPanel,
  type PlusParseDraft,
  type PlusSellRow,
  plusApi,
} from "./api";
import { QuietEmpty } from "./feed-section";
import { GroupTitle } from "./fields";
import { itemName, PlusItemIcon } from "./item-icon";
import {
  canSpend,
  isWide,
  listMaxOf,
  newBuyRow,
  newRowId,
  newSellRow,
  PLUS_LIMITS,
  putRule,
  removeRule,
  rowFromDraft,
  type Side,
  setRuleEnabled,
} from "./model";
import { type RuleDraft, RuleEditor } from "./rule-editor";
import { plusKey, usePlusFeatures, usePlusInstantSave, usePlusUi } from "./use-plus";
import { amountWords, currentLang, formatAmount } from "./words";

/** When the rule asks first, in words: "35.000 üstünde sorar", "Her seferinde sorar". */
export function useAskPhrase() {
  const { t } = useTranslation();
  return (side: Side, approval: string, limits: PlusLimits): string => {
    if (approval === "always") return t("afkPlus.lists.asksAlways");
    if (approval === "above" && limits.approvalAbove !== null) {
      return t(side === "buy" ? "afkPlus.lists.asksAbove" : "afkPlus.lists.asksAboveSell", {
        amount: formatAmount(limits.approvalAbove),
      });
    }
    return t(side === "buy" ? "afkPlus.lists.asksNever" : "afkPlus.lists.asksNeverSell");
  };
}

/** One amount with its label over it and, for a price per item, "adet başına" under it. */
function Amount({
  label,
  value,
  perItem,
  words,
}: {
  label: string;
  value: number;
  perItem?: boolean;
  words?: boolean;
}) {
  const { t } = useTranslation();
  // Spans only: rows put this inside their edit button.
  return (
    <span className="flex min-w-0 flex-col items-end text-right">
      <span className="text-[11.5px] text-ash-400">{label}</span>
      <span className="font-display text-[14px] leading-tight font-semibold text-bone-50 tabular-nums">
        {formatAmount(value)}
      </span>
      {words ? (
        <span className="max-w-[22ch] truncate text-[11.5px] text-ash-400">
          {amountWords(value, currentLang())}
        </span>
      ) : perItem ? (
        <span className="text-[11.5px] text-ash-400">{t("afkPlus.lists.each")}</span>
      ) : null}
    </span>
  );
}

// --- A rule typed in words (QoL Q2) ----------------------------------------------------------------

type TypedFailure = { text: string; pick: boolean } | null;

/** The draft card under the typed field: the rule as read, "Listeye ekle" saves it. */
function DraftCard({
  draft,
  catalog,
  limits,
  full,
  onAdd,
  onEdit,
  onDiscard,
}: {
  draft: PlusParseDraft;
  catalog: PlusCatalog | null;
  limits: PlusLimits;
  /** The draft's list has no room left. */
  full: boolean;
  onAdd: (item: string) => void;
  onEdit: (item: string | null) => void;
  onDiscard: () => void;
}) {
  const { t } = useTranslation();
  const ask = useAskPhrase();
  const [item, setItem] = useState<string | null>(draft.item);
  const [confirm, setConfirm] = useState(false);
  const side: Side = draft.side === "sell" ? "sell" : "buy";
  const target = draft.target ?? null;
  const limit = side === "buy" ? (draft.max ?? null) : (draft.min ?? null);
  const priced = side === "buy" ? limit !== null : target !== null;
  const perItem = draft.unit !== "total" && draft.qty > 1;
  const wide =
    side === "buy" && target !== null && limit !== null && isWide({ target, max: limit });
  const style = ["patient", "balanced", "quick"].includes(draft.style) ? draft.style : "balanced";
  const candidates = item ? [] : draft.candidates.slice(0, 5);

  return (
    <div className="rounded-[var(--radius-panel)] bg-obsidian-800/70 p-3.5 ring-1 ring-white/8 ring-inset">
      {item ? (
        <div className="flex items-start gap-3">
          <PlusItemIcon item={item} size={40} />
          <div className="min-w-0 flex-1">
            <p className="flex items-baseline gap-2">
              <span className="truncate font-display text-[15px] font-semibold text-bone-50">
                {itemName(catalog, item)}
              </span>
              <span className="shrink-0 text-[12.5px] text-ash-300">
                {t(side === "buy" ? "afkPlus.lists.draftBuy" : "afkPlus.lists.draftSell")}
              </span>
            </p>
            <p className="mt-0.5 text-[12.5px] text-ash-300">
              {[
                t("afkPlus.lists.qty", { count: Math.max(1, draft.qty) }),
                t("afkPlus.lists.styleLine", { style: t(`afkPlus.lists.styles.${style}`) }),
                ask(side, draft.approval, limits),
              ].join(", ")}
            </p>
          </div>
          <div className="flex shrink-0 gap-5">
            {target !== null ? (
              <Amount label={t("afkPlus.lists.target")} value={target} words perItem={perItem} />
            ) : null}
            {limit !== null ? (
              <Amount
                label={side === "buy" ? t("afkPlus.lists.max") : t("afkPlus.lists.min")}
                value={limit}
                words
                perItem={perItem}
              />
            ) : null}
          </div>
        </div>
      ) : (
        <div className="flex flex-col gap-2.5">
          <p className="font-display text-[15px] font-semibold text-bone-50">
            {t("afkPlus.lists.which")}
          </p>
          <ul className="flex flex-wrap gap-1.5">
            {candidates.map((id) => (
              <li key={id}>
                <button
                  type="button"
                  onClick={() => setItem(id)}
                  className={cn(
                    "flex cursor-pointer items-center gap-2 rounded-md bg-white/[0.05] py-1 pr-3 pl-1 text-[13px] text-bone-100 outline-none",
                    "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/9 active:scale-[0.97] focus-visible:ring-2 focus-visible:ring-gold-300",
                  )}
                >
                  <PlusItemIcon item={id} size={24} />
                  {itemName(catalog, id)}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {item && !priced ? (
        <p className="mt-3 text-[12.5px] text-gold-200">{t("afkPlus.lists.parse.no_amount")}</p>
      ) : null}
      {full ? (
        <p className="mt-3 text-[12.5px] text-ash-300">
          {t("afkPlus.lists.full", { count: listMaxOf(catalog) })}
        </p>
      ) : null}

      <AnimatePresence initial={false} mode="popLayout">
        {confirm && wide && target !== null && limit !== null ? (
          <motion.div
            key="confirm"
            className="mt-3 flex flex-wrap items-center justify-between gap-3 border-t border-white/7 pt-3"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.18) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            <p className="min-w-0 flex-1 text-[12.5px] text-bone-100">
              <span className="font-semibold text-gold-200">{t("afkPlus.editor.wideTitle")} </span>
              {t("afkPlus.editor.wide", {
                max: formatAmount(limit),
                target: formatAmount(target),
              })}
            </p>
            <div className="flex gap-2">
              <Button size="sm" variant="ghost" onClick={() => setConfirm(false)}>
                {t("afkPlus.editor.cancel")}
              </Button>
              <Button size="sm" variant="primary" onClick={() => item && onAdd(item)}>
                {t("afkPlus.editor.wideConfirm")}
              </Button>
            </div>
          </motion.div>
        ) : (
          <motion.div
            key="actions"
            className="mt-3 flex justify-end gap-2"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.18) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            <Button size="sm" variant="ghost" onClick={onDiscard}>
              {t("afkPlus.lists.discard")}
            </Button>
            <Button
              size="sm"
              variant={item && priced ? "secondary" : "primary"}
              disabled={full}
              onClick={() => onEdit(item)}
            >
              {t("afkPlus.lists.edit")}
            </Button>
            {item && priced ? (
              <Button
                size="sm"
                variant="primary"
                disabled={full}
                onClick={() => (wide ? setConfirm(true) : onAdd(item))}
              >
                {t("afkPlus.lists.add")}
              </Button>
            ) : null}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/**
 * "Ne almak ya da satmak istiyorsun?": one line in the player's words, read by the service into a
 * draft card. Used by Listeler and by the setup wizard's last step.
 */
export function TypedRule({
  account,
  panel,
  onAdded,
  onPick,
}: {
  account: CloudAccount;
  panel: PlusPanel;
  /** A rule went into a list (the wizard moves on). */
  onAdded?: () => void;
  /** Open the rule dialog (an item not found, "Düzenle" on the card). */
  onPick: (rule: RuleDraft) => void;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const id = useId();
  const queryClient = useQueryClient();
  const save = usePlusInstantSave(account.id);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<TypedFailure>(null);
  const [draft, setDraft] = useState<PlusParseDraft | null>(null);
  const [round, setRound] = useState(0);
  const config = panel.settings.config;
  const catalog = panel.catalog;
  const max = listMaxOf(catalog);

  const blankRule = (side: Side): RuleDraft => {
    const rowId = newRowId(config);
    return side === "buy"
      ? { side: "buy", row: newBuyRow(rowId, "") }
      : { side: "sell", row: newSellRow(rowId, "") };
  };

  const read = async (event?: FormEvent) => {
    event?.preventDefault();
    const words = text.trim();
    if (!words || busy) return;
    setBusy(true);
    setFailure(null);
    try {
      const outcome = await plusApi.parse(account.id, words);
      if (outcome.kind === "draft") {
        if (!outcome.draft.item && !outcome.draft.candidates.length) {
          setFailure({ text: t("afkPlus.lists.parse.no_item"), pick: true });
        } else {
          setDraft(outcome.draft);
          setRound((n) => n + 1);
        }
      } else if (outcome.kind === "failed") {
        const known = ["no_item", "no_amount", "many_items", "unsafe"].includes(outcome.reason);
        setFailure({
          text: t(`afkPlus.lists.parse.${known ? outcome.reason : "unsafe"}`),
          pick: outcome.reason === "no_item",
        });
      } else if (outcome.kind === "limited") {
        setFailure({
          text: t(outcome.daily ? "afkPlus.lists.parse.daily" : "afkPlus.lists.parse.limited"),
          pick: outcome.daily,
        });
      } else {
        setFailure({ text: t("afkPlus.lists.parse.offline"), pick: true });
      }
    } catch (error) {
      afkFailed(error, t("afkPlus.lists.parse.failed"));
    } finally {
      setBusy(false);
    }
  };

  const add = async (item: string) => {
    if (!draft) return;
    const latest =
      queryClient.getQueryData<PlusPanel>(plusKey(account.id))?.settings.config ?? config;
    const rule = rowFromDraft(draft, item, newRowId(latest));
    setDraft(null);
    setText("");
    const done = await save((current) => putRule(current, rule));
    if (!done) return;
    // The wizard says it in place; Listeler shows the new row and confirms it in a toast.
    if (onAdded) onAdded();
    else toast.success(t("afkPlus.lists.added", { item: itemName(catalog, item) }));
  };

  const edit = (item: string | null) => {
    if (!draft) return;
    const rule = rowFromDraft(draft, item ?? "", newRowId(config));
    onPick(rule);
    setDraft(null);
  };

  const side: Side = draft?.side === "sell" ? "sell" : "buy";
  const full = (side === "buy" ? config.buy.length : config.sell.length) >= max;

  return (
    <motion.section layout={layout ? "position" : false} aria-labelledby={`${id}-label`}>
      <form onSubmit={(event) => void read(event)} className="flex flex-col gap-1.5">
        <label htmlFor={id} id={`${id}-label`} className="text-[13px] font-medium text-bone-100">
          {t("afkPlus.lists.typedLabel")}
        </label>
        <div
          className={cn(
            "flex items-center gap-1 rounded-md border bg-obsidian-800/80 p-[3px] pl-0 transition-[border-color,box-shadow] duration-150",
            "border-white/10 hover:border-white/18 focus-within:border-gold-400/60 focus-within:ring-2 focus-within:ring-gold-400/20",
            failure &&
              "border-flare-400/55 focus-within:border-flare-400/70 focus-within:ring-flare-400/20",
          )}
        >
          <input
            id={id}
            value={text}
            maxLength={PLUS_LIMITS.parseMax}
            autoComplete="off"
            spellCheck={false}
            placeholder={t("afkPlus.lists.typedPlaceholder")}
            aria-invalid={failure ? true : undefined}
            aria-describedby={`${id}-line`}
            onChange={(event) => {
              setText(event.target.value);
              setFailure(null);
            }}
            className="h-[30px] min-w-0 flex-1 bg-transparent pl-3 text-[13.5px] text-bone-50 outline-none placeholder:text-ash-500"
          />
          <Button
            type="submit"
            size="sm"
            variant={text.trim() ? "primary" : "secondary"}
            disabled={!text.trim() || busy}
          >
            <SwapLabel
              labels={[t("afkPlus.lists.make"), t("afkPlus.lists.making")]}
              active={busy ? 1 : 0}
            />
          </Button>
        </div>
        <p
          id={`${id}-line`}
          role={failure ? "alert" : undefined}
          className="flex min-h-[18px] flex-wrap items-center gap-x-2 text-[12px] leading-[18px]"
        >
          {failure ? (
            <>
              <span className="text-flare-300">{failure.text}</span>
              {failure.pick ? (
                <button
                  type="button"
                  onClick={() => {
                    setFailure(null);
                    onPick(blankRule("buy"));
                  }}
                  className="cursor-pointer rounded-sm font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
                >
                  {t("afkPlus.lists.parse.pick")}
                </button>
              ) : null}
            </>
          ) : null}
        </p>
      </form>
      <AnimatePresence initial={false} mode="popLayout">
        {draft ? (
          <motion.div
            key={`draft-${round}`}
            layout={layout ? "position" : false}
            className="mt-1"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.15) }}
          >
            <DraftCard
              draft={draft}
              catalog={catalog}
              limits={config.limits}
              full={full}
              onAdd={(item) => void add(item)}
              onEdit={edit}
              onDiscard={() => setDraft(null)}
            />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </motion.section>
  );
}

// --- The lists ------------------------------------------------------------------------------------

type AnyRow = PlusBuyRow | PlusSellRow;

function RuleRow({
  side,
  row,
  catalog,
  limits,
  onEdit,
  onToggle,
  onRemove,
}: {
  side: Side;
  row: AnyRow;
  catalog: PlusCatalog | null;
  limits: PlusLimits;
  onEdit: () => void;
  onToggle: (enabled: boolean) => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const ask = useAskPhrase();
  const name = itemName(catalog, row.item);
  const perItem = row.unit !== "total" && row.qty > 1;
  const limit = side === "buy" ? (row as PlusBuyRow).max : (row as PlusSellRow).min;
  const style = ["patient", "balanced", "quick"].includes(row.style) ? row.style : "balanced";
  const items: MenuItem[] = [
    { label: t("afkPlus.lists.edit"), icon: <PencilSimple size={16} />, onSelect: onEdit },
    {
      label: t("afkPlus.lists.remove"),
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
            onClick={onEdit}
            aria-label={t("afkPlus.lists.editLabel", { item: name })}
            className={cn(
              "flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-md py-1 text-left outline-none",
              "transition-[opacity,transform,scale] duration-150 ease-out active:scale-[0.99] focus-visible:ring-2 focus-visible:ring-gold-300",
              !row.enabled && "opacity-55",
            )}
          >
            <PlusItemIcon item={row.item} size={32} />
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13.5px] font-medium text-bone-50">{name}</span>
              <span className="block truncate text-[12px] text-ash-300">
                {row.enabled
                  ? [
                      t("afkPlus.lists.qty", { count: row.qty }),
                      t("afkPlus.lists.styleLine", { style: t(`afkPlus.lists.styles.${style}`) }),
                      ask(side, row.approval, limits),
                    ].join(", ")
                  : t("afkPlus.lists.off")}
              </span>
            </span>
            <span className="flex shrink-0 gap-5 pr-2">
              <Amount label={t("afkPlus.lists.target")} value={row.target} perItem={perItem} />
              <Amount
                label={side === "buy" ? t("afkPlus.lists.max") : t("afkPlus.lists.min")}
                value={limit}
                perItem={perItem}
              />
            </span>
          </button>
          <Switch
            label={t("afkPlus.lists.use", { item: name })}
            hideLabel
            checked={row.enabled}
            onCheckedChange={onToggle}
          />
          <Menu
            trigger={
              <IconButton
                label={t("afkPlus.lists.more")}
                icon={<DotsThreeVertical size={18} weight="bold" />}
                size="sm"
              />
            }
            items={items}
          />
        </div>
      </ContextMenu>
    </motion.li>
  );
}

function RuleList({
  side,
  rows,
  catalog,
  limits,
  max,
  notice,
  onAdd,
  onEdit,
  onToggle,
  onRemove,
}: {
  side: Side;
  rows: AnyRow[];
  catalog: PlusCatalog | null;
  limits: PlusLimits;
  max: number;
  notice?: ReactNode;
  onAdd: () => void;
  onEdit: (row: AnyRow) => void;
  onToggle: (row: AnyRow, enabled: boolean) => void;
  onRemove: (row: AnyRow, index: number) => void;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const id = useId();
  const full = rows.length >= max;
  const noCatalog = !catalog?.items.length;
  return (
    <motion.section layout={layout ? "position" : false} aria-labelledby={id}>
      <GroupTitle
        id={id}
        aside={
          <Button
            size="sm"
            variant="ghost"
            icon={<Plus size={14} weight="bold" />}
            disabled={full || noCatalog}
            onClick={onAdd}
          >
            {t(side === "buy" ? "afkPlus.lists.addBuy" : "afkPlus.lists.addSell")}
          </Button>
        }
      >
        {t(side === "buy" ? "afkPlus.lists.buyTitle" : "afkPlus.lists.sellTitle")}
      </GroupTitle>
      {notice}
      {rows.length ? (
        <ul className="-mx-1.5 flex flex-col">
          <AnimatePresence initial={false}>
            {rows.map((row, index) => (
              <RuleRow
                key={row.id}
                side={side}
                row={row}
                catalog={catalog}
                limits={limits}
                onEdit={() => onEdit(row)}
                onToggle={(enabled) => onToggle(row, enabled)}
                onRemove={() => onRemove(row, index)}
              />
            ))}
          </AnimatePresence>
        </ul>
      ) : (
        <QuietEmpty icon="pack">
          {t(side === "buy" ? "afkPlus.lists.emptyBuy" : "afkPlus.lists.emptySell")}
        </QuietEmpty>
      )}
      {full || noCatalog ? (
        <p className="mt-2 text-[12px] text-ash-400">
          {noCatalog ? t("afkPlus.lists.noCatalog") : t("afkPlus.lists.full", { count: max })}
        </p>
      ) : null}
    </motion.section>
  );
}

/** "Alım yapabilmesi için harcama sınırı ver": a quiet panel with its one action. */
function SpendNotice() {
  const { t } = useTranslation();
  const setSection = usePlusUi((s) => s.setSection);
  const setFocusGroup = usePlusUi((s) => s.setFocusGroup);
  return (
    <div className="mb-2 flex items-start gap-3 rounded-[var(--radius-panel)] bg-white/[0.04] px-3.5 py-3">
      <WarningCircle size={16} weight="fill" className="mt-px shrink-0 text-gold-300" aria-hidden />
      <p className="min-w-0 flex-1 text-[12.5px] text-bone-100">{t("afkPlus.lists.notifyOnly")}</p>
      <Button
        size="sm"
        variant="secondary"
        className="-my-0.5 shrink-0"
        onClick={() => {
          setFocusGroup("limits");
          setSection("settings");
        }}
      >
        {t("afkPlus.lists.setLimits")}
      </Button>
    </div>
  );
}

export function ListsSection({ account, panel }: { account: CloudAccount; panel: PlusPanel }) {
  const { t } = useTranslation();
  const features = usePlusFeatures();
  const save = usePlusInstantSave(account.id);
  const [editing, setEditing] = useState<{ rule: RuleDraft; isNew: boolean } | null>(null);
  const config = panel.settings.config;
  const catalog = panel.catalog;
  const max = listMaxOf(catalog);

  const openNew = (side: Side) => {
    const id = newRowId(config);
    setEditing({
      isNew: true,
      rule:
        side === "buy"
          ? { side: "buy", row: newBuyRow(id, "") }
          : { side: "sell", row: newSellRow(id, "") },
    });
  };

  const remove = (side: Side, row: AnyRow, index: number) => {
    void save((current) => removeRule(current, side, row.id)).then((done) => {
      if (!done) return;
      const rule: RuleDraft =
        side === "buy"
          ? { side: "buy", row: row as PlusBuyRow }
          : { side: "sell", row: row as PlusSellRow };
      toast(t("afkPlus.lists.removed", { item: itemName(catalog, row.item) }), {
        action: {
          label: t("afkPlus.lists.undo"),
          onClick: () => void save((current) => putRule(current, rule, index)),
        },
      });
    });
  };

  const list = (side: Side) => {
    const rows: AnyRow[] = side === "buy" ? config.buy : config.sell;
    return (
      <RuleList
        side={side}
        rows={rows}
        catalog={catalog}
        limits={config.limits}
        max={max}
        notice={side === "buy" && !canSpend(config.limits) ? <SpendNotice /> : undefined}
        onAdd={() => openNew(side)}
        onEdit={(row) =>
          setEditing({
            isNew: false,
            rule:
              side === "buy"
                ? { side: "buy", row: row as PlusBuyRow }
                : { side: "sell", row: row as PlusSellRow },
          })
        }
        onToggle={(row, enabled) =>
          void save((current) => setRuleEnabled(current, side, row.id, enabled))
        }
        onRemove={(row, index) => remove(side, row, index)}
      />
    );
  };

  return (
    <div className="flex flex-col gap-7">
      {features.parse ? (
        <TypedRule
          account={account}
          panel={panel}
          onPick={(rule) => setEditing({ rule, isNew: true })}
        />
      ) : null}
      {list("buy")}
      {list("sell")}
      <RuleEditor
        open={editing !== null}
        initial={editing?.rule ?? null}
        catalog={catalog}
        isNew={editing?.isNew ?? false}
        saving={false}
        onSave={(rule) => {
          setEditing(null);
          void save((current) => putRule(current, rule)).then((done) => {
            if (done && editing?.isNew) {
              toast.success(t("afkPlus.lists.added", { item: itemName(catalog, rule.row.item) }));
            }
          });
        }}
        onClose={() => setEditing(null)}
      />
    </div>
  );
}
