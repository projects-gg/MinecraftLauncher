// A buy or sell rule in a dialog (Listeler's "Alınacak ekle", "Düzenle", a typed rule's
// "Düzenle"): the item from the mode's catalog (search; picture and name), how many, the price per
// item or for all, the target and the limit (in words under each), the haggling style, when it asks
// first, how many a day; for a buy, what the item must be. A limit over ten times the target is
// asked once ("Bu fiyat doğru mu?"): a zero too many costs real game money.
import { MagnifyingGlass } from "@phosphor-icons/react";
import { Button, cn, Dialog, Segmented, Select, Switch } from "@pl/ui";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { PlusBuyRow, PlusCatalog, PlusSellRow } from "./api";
import { AmountField, IntField } from "./fields";
import { itemName, PlusItemIcon } from "./item-icon";
import {
  type ApprovalRule,
  buyRowProblems,
  isProvableSale,
  isWide,
  maxDealOf,
  type RowProblem,
  type Side,
  type Style,
  sellRowProblems,
} from "./model";
import { formatAmount } from "./words";

export type RuleDraft = { side: "buy"; row: PlusBuyRow } | { side: "sell"; row: PlusSellRow };

function ItemPicker({
  catalog,
  value,
  onPick,
}: {
  catalog: PlusCatalog | null;
  value: string;
  onPick: (id: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(!value);
  const items = catalog?.items ?? [];
  const lower = query.trim().toLocaleLowerCase(i18n.language === "en" ? "en" : "tr");
  const shown = lower
    ? items.filter((item) =>
        [item.tr, item.en, item.id].some((name) =>
          name.toLocaleLowerCase(i18n.language === "en" ? "en" : "tr").includes(lower),
        ),
      )
    : items;
  if (!open && value) {
    return (
      <div className="flex items-center gap-3 rounded-[var(--radius-panel)] bg-white/[0.04] px-3 py-2.5">
        <PlusItemIcon item={value} size={32} />
        <span className="min-w-0 flex-1 truncate text-[14px] font-medium text-bone-50">
          {itemName(catalog, value)}
        </span>
        <Button size="sm" variant="ghost" onClick={() => setOpen(true)}>
          {t("afkPlus.editor.change")}
        </Button>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-2">
      <div className="relative">
        <MagnifyingGlass
          size={15}
          className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-ash-400"
          aria-hidden
        />
        <input
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t("afkPlus.editor.itemSearch")}
          aria-label={t("afkPlus.editor.itemSearch")}
          // biome-ignore lint/a11y/noAutofocus: picking the item is the dialog's first step
          autoFocus
          className="h-[34px] w-full rounded-md border border-white/10 bg-obsidian-800/80 pr-3 pl-9 text-[13.5px] text-bone-50 placeholder:text-ash-500 hover:border-white/18 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-400/25 focus:outline-none"
        />
      </div>
      <ul
        aria-label={t("afkPlus.editor.item")}
        className="grid max-h-[212px] grid-cols-2 gap-1 overflow-y-auto pr-1 [scrollbar-width:thin]"
      >
        {shown.map((item) => {
          const on = item.id === value;
          return (
            <li key={item.id}>
              <button
                type="button"
                aria-pressed={on}
                onClick={() => {
                  onPick(item.id);
                  setOpen(false);
                  setQuery("");
                }}
                className={cn(
                  "flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 text-left text-[13px] outline-none",
                  "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/6 active:scale-[0.98] focus-visible:ring-2 focus-visible:ring-gold-300",
                  on && "bg-white/8 text-bone-50",
                )}
              >
                <PlusItemIcon item={item.id} size={24} />
                <span className="min-w-0 truncate text-bone-100">
                  {i18n.language === "en" ? item.en : item.tr}
                </span>
              </button>
            </li>
          );
        })}
        {!shown.length ? (
          <li className="col-span-2 px-2 py-3 text-[12.5px] text-ash-400">
            {t("afkPlus.editor.itemNone")}
          </li>
        ) : null}
      </ul>
    </div>
  );
}

function problemText(
  problem: RowProblem | undefined,
  side: Side,
  maxDeal: number,
  t: (key: string, values?: Record<string, unknown>) => string,
): string | null {
  switch (problem) {
    case "qty":
      return t("afkPlus.editor.errors.qty");
    case "target":
      return t("afkPlus.editor.errors.target");
    case "order":
      return t(
        side === "buy" ? "afkPlus.editor.errors.orderBuy" : "afkPlus.editor.errors.orderSell",
      );
    case "maxDeal":
      return t("afkPlus.editor.errors.maxDeal", { max: formatAmount(maxDeal) });
    case "dailyQty":
      return t("afkPlus.editor.errors.dailyQty");
    case "durability":
      return t("afkPlus.editor.errors.durability");
    default:
      return null;
  }
}

export function RuleEditor({
  open,
  initial,
  catalog,
  isNew,
  saving,
  onSave,
  onClose,
}: {
  open: boolean;
  initial: RuleDraft | null;
  catalog: PlusCatalog | null;
  isNew: boolean;
  saving: boolean;
  onSave: (draft: RuleDraft) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && onClose()}
      title={
        initial?.side === "sell" ? t("afkPlus.editor.titleSell") : t("afkPlus.editor.titleBuy")
      }
      closeLabel={t("common.close")}
      width={580}
      morph
    >
      {initial ? (
        <EditorBody
          key={`${initial.side}-${initial.row.id}-${isNew}`}
          initial={initial}
          catalog={catalog}
          saving={saving}
          onSave={onSave}
          onClose={onClose}
        />
      ) : null}
    </Dialog>
  );
}

function EditorBody({
  initial,
  catalog,
  saving,
  onSave,
  onClose,
}: {
  initial: RuleDraft;
  catalog: PlusCatalog | null;
  saving: boolean;
  onSave: (draft: RuleDraft) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<RuleDraft>(initial);
  const [submitted, setSubmitted] = useState(false);
  const [confirmWide, setConfirmWide] = useState(false);
  const maxDeal = maxDealOf(catalog);
  const side = draft.side;
  const row = draft.row;
  const limit = side === "buy" ? (row as PlusBuyRow).max : (row as PlusSellRow).min;
  const damageable = catalog?.items.find((item) => item.id === row.item)?.damageable ?? false;

  const set = (parts: Partial<PlusBuyRow & PlusSellRow>) =>
    setDraft((current) =>
      current.side === "buy"
        ? { side: "buy", row: { ...current.row, ...(parts as Partial<PlusBuyRow>) } }
        : { side: "sell", row: { ...current.row, ...(parts as Partial<PlusSellRow>) } },
    );

  const problems = useMemo(
    () =>
      draft.side === "buy"
        ? buyRowProblems(draft.row, maxDeal)
        : sellRowProblems(draft.row, maxDeal),
    [draft, maxDeal],
  );
  const shownProblem = (field: RowProblem) =>
    submitted && problems.includes(field) ? problemText(field, side, maxDeal, t) : null;
  const noItem = submitted && !row.item;

  const approvals = (["auto", "above", "always"] as ApprovalRule[]).map((value) => ({
    value,
    label: t(`afkPlus.editor.approvals.${value}`),
  }));
  const styles = (["patient", "balanced", "quick"] as Style[]).map((value) => ({
    value,
    label: t(`afkPlus.lists.styles.${value}`),
  }));

  const save = () => {
    setSubmitted(true);
    if (!row.item || problems.length) return;
    if (side === "buy" && isWide(row as PlusBuyRow) && !confirmWide) {
      setConfirmWide(true);
      return;
    }
    onSave(draft);
  };

  if (confirmWide && side === "buy") {
    const buyRow = row as PlusBuyRow;
    return (
      <div className="flex flex-col gap-4">
        <div>
          <p className="font-display text-[16px] font-semibold text-bone-50">
            {t("afkPlus.editor.wideTitle")}
          </p>
          <p className="mt-1 text-[13px] text-bone-100">
            {t("afkPlus.editor.wide", {
              max: formatAmount(buyRow.max),
              target: formatAmount(buyRow.target),
            })}
          </p>
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setConfirmWide(false)}>
            {t("afkPlus.editor.cancel")}
          </Button>
          <Button variant="primary" disabled={saving} onClick={() => onSave(draft)}>
            {t("afkPlus.editor.wideConfirm")}
          </Button>
        </div>
      </div>
    );
  }

  const unprovable =
    side === "sell" && (!isProvableSale(row.target) || !isProvableSale((row as PlusSellRow).min));

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1.5">
        <span className="text-[13px] font-medium text-bone-100">{t("afkPlus.editor.item")}</span>
        <ItemPicker catalog={catalog} value={row.item} onPick={(item) => set({ item })} />
        {noItem ? (
          <p role="alert" className="text-[12px] text-flare-400">
            {t("afkPlus.editor.errors.item")}
          </p>
        ) : null}
      </div>

      <div className="grid grid-cols-[120px_minmax(0,1fr)] items-start gap-4">
        <IntField
          label={t("afkPlus.editor.qty")}
          value={row.qty}
          onChange={(qty) => set({ qty: qty ?? 0 })}
          error={shownProblem("qty")}
        />
        <div className="flex flex-col gap-1.5">
          <span className="text-[13px] font-medium text-bone-100">{t("afkPlus.editor.unit")}</span>
          <Segmented
            label={t("afkPlus.editor.unit")}
            value={row.unit === "total" ? "total" : "each"}
            onValueChange={(unit) => set({ unit })}
            options={[
              { value: "each", label: t("afkPlus.editor.unitEach") },
              { value: "total", label: t("afkPlus.editor.unitTotal") },
            ]}
            className="self-start"
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <AmountField
          label={t("afkPlus.editor.target")}
          value={row.target > 0 ? row.target : null}
          onChange={(value) => set({ target: value ?? 0 })}
          error={shownProblem("target")}
        />
        <AmountField
          label={side === "buy" ? t("afkPlus.editor.max") : t("afkPlus.editor.min")}
          value={limit > 0 ? limit : null}
          onChange={(value) =>
            side === "buy" ? set({ max: value ?? 0 }) : set({ min: value ?? 0 })
          }
          error={shownProblem("order") ?? shownProblem("maxDeal")}
        />
      </div>
      {unprovable ? (
        <p className="-mt-2 text-[12.5px] text-gold-200">{t("afkPlus.editor.roundSell")}</p>
      ) : null}

      <div className="flex flex-col gap-1.5">
        <span className="text-[13px] font-medium text-bone-100">{t("afkPlus.editor.style")}</span>
        <Segmented
          label={t("afkPlus.editor.style")}
          value={
            (["patient", "balanced", "quick"].includes(row.style) ? row.style : "balanced") as Style
          }
          onValueChange={(style) => set({ style })}
          options={styles}
          className="self-start"
        />
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)_150px] items-start gap-4">
        <div className="flex flex-col gap-1.5">
          <span className="text-[13px] font-medium text-bone-100">
            {t("afkPlus.editor.approval")}
          </span>
          <Select
            label={t("afkPlus.editor.approval")}
            value={
              (["auto", "above", "always"].includes(row.approval)
                ? row.approval
                : "above") as ApprovalRule
            }
            onValueChange={(approval) => set({ approval })}
            options={approvals}
          />
        </div>
        <IntField
          label={t("afkPlus.editor.daily")}
          value={row.dailyQty}
          onChange={(dailyQty) => set({ dailyQty: dailyQty ?? 0 })}
          error={shownProblem("dailyQty")}
        />
      </div>

      {side === "buy" ? (
        <div className="flex flex-col gap-1 border-t border-white/7 pt-3">
          {damageable ? (
            <IntField
              label={t("afkPlus.editor.durability")}
              value={(row as PlusBuyRow).req.minDurabilityPct}
              unit="%"
              className="mb-2 w-[160px]"
              onChange={(pct) =>
                set({
                  req: {
                    ...(row as PlusBuyRow).req,
                    minDurabilityPct: Math.min(100, pct ?? 0),
                  },
                })
              }
              error={shownProblem("durability")}
            />
          ) : null}
          {(
            [
              ["allowCustomName", "customName"],
              ["allowCurses", "curses"],
            ] as const
          ).map(([field, key]) => (
            <Switch
              key={field}
              label={t(`afkPlus.editor.${key}`)}
              checked={(row as PlusBuyRow).req[field]}
              onCheckedChange={(checked) =>
                set({ req: { ...(row as PlusBuyRow).req, [field]: checked } })
              }
              className="py-1.5"
            />
          ))}
          <Switch
            label={t("afkPlus.editor.notify")}
            checked={(row as PlusBuyRow).notify}
            onCheckedChange={(notify) => set({ notify })}
            className="py-1.5"
          />
        </div>
      ) : null}

      <div className="flex justify-end gap-2 border-t border-white/7 pt-3">
        <Button variant="ghost" onClick={onClose} disabled={saving}>
          {t("afkPlus.editor.cancel")}
        </Button>
        <Button variant="primary" onClick={save} disabled={saving}>
          {t("afkPlus.editor.save")}
        </Button>
      </div>
      {submitted && problems.length ? (
        <p className="sr-only" role="alert">
          {t("afkPlus.settings.fixFirst")}
        </p>
      ) : null}
    </div>
  );
}
