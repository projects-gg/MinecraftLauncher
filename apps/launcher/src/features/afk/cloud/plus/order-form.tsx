// An order in the form (afkplus-free.md §7.2, §7.6): "Formla ekle", a card's "Düzenle" or one of its
// parts, a row's "Düzenle", a picked item. A second sheet over the AFK+ panel (the account center's
// way), at the field it was opened for. The item comes from the game's own item data (searched by
// either language and our aliases), the account's custom items or the bot's inventory; amounts are
// written and read back in words; the conditions are on what the bot receives (the item when
// buying, the paying item when selling for one), enchantments only those that fit it. A buy's
// limit over ten times its target is asked once: a zero too many costs real game money.
import { MagnifyingGlass, Minus, Plus, X } from "@phosphor-icons/react";
import {
  Button,
  cn,
  EntranceGate,
  focusPanel,
  GatedMotion,
  IconButton,
  Segmented,
  Switch,
  scrimMotion,
  sheetMotion,
  toast,
  useInteractionMotion,
} from "@pl/ui";
import {
  canCombine,
  type EnchantmentInfo,
  enchantmentsFor,
  type ItemInfo,
  levelText,
  searchItems,
} from "@pl/ui/items";
import { AnimatePresence } from "motion/react";
import { Dialog as RadixDialog } from "radix-ui";
import { type ReactNode, useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { looksLikeLink, looksLikePhone } from "@/features/social/chat/text";
import { inputClass } from "../../settings/controls";
import type { PlusIssue, PlusOrder, PlusSettings } from "./api";
import { AmountField, type AmountValue, IntField } from "./fields";
import type { PickedItem } from "./inventory-pick";
import { ItemTile, PlusItemIcon } from "./item-icon";
import { baseInfo, type ItemDataState, itemHint, itemInfo, itemLabel, type Namer } from "./items";
import { breaksForm, hasMarker } from "./model";
import {
  CUSTOM_NAME_MAX,
  type CustomItemLike,
  fittingEnch,
  MAX_COUNT,
  type NewOrder,
} from "./order-draft";
import { newCustomKey } from "./orders";
import { emptyPatch, useOrderWrites } from "./orders-section";
import { currentLang, formatAmount } from "./words";

export type FormField =
  | "side"
  | "item"
  | "name"
  | "qty"
  | "pay"
  | "target"
  | "limit"
  | "style"
  | "days"
  | "trade"
  | "req";

/** What a form starts from: a card's draft, a picked item, a finished order to start again. */
export interface FormSeed {
  side?: "buy" | "sell";
  item?: string;
  qty?: number;
  pay?: string;
  unit?: "each" | "total";
  target?: number;
  limit?: number;
  style?: "patient" | "balanced" | "quick";
  trade?: "auto" | "owner";
  until?: number;
  req?: { ench: Record<string, number>; dur: number | null; named: boolean; cursed: boolean };
  note?: string;
  src?: "text" | "form" | "inv";
  /** New custom items the order names (a pick, a first sight). */
  custom: CustomItemLike[];
}

export type FormRequest =
  | {
      kind: "new";
      seed?: FormSeed;
      focus?: FormField;
      issues?: PlusIssue[];
      /** The order is saved (a card that opened the form goes). */
      onSaved?: () => void;
    }
  | { kind: "edit"; order: PlusOrder; focus?: FormField; issues?: PlusIssue[] };

const DAYS: readonly number[] = [7, 14, 30];

interface FormState {
  side: "buy" | "sell";
  item: string | null;
  qty: number | null;
  pay: string;
  unit: "each" | "total";
  target: number | null;
  limit: number | null;
  style: "patient" | "balanced" | "quick";
  trade: "auto" | "owner";
  days: number | null;
  until: number;
  ench: Record<string, number>;
  dur: number | null;
  named: boolean;
  cursed: boolean;
  custom: CustomItemLike[];
  note: string;
  src: "text" | "form" | "inv";
}

const nowSeconds = () => Math.floor(Date.now() / 1000);

function stateOf(request: FormRequest, defaultDays: number): FormState {
  if (request.kind === "edit") {
    const order = request.order;
    return {
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
      days: null,
      until: order.until,
      ench: order.req?.ench ?? {},
      dur: order.req?.dur ?? null,
      named: order.req?.named ?? false,
      cursed: order.req?.cursed ?? false,
      custom: [],
      note: order.note,
      src: order.src === "inv" ? "inv" : order.src === "text" ? "text" : "form",
    };
  }
  const seed = request.seed;
  // The days the text gave ("10 gün") stay one of the choices; a restarted order starts over.
  const days = seed?.until
    ? Math.max(1, Math.round((seed.until - nowSeconds()) / 86_400))
    : defaultDays;
  return {
    side: seed?.side ?? "buy",
    item: seed?.item ?? null,
    qty: seed?.qty ?? 1,
    pay: seed?.pay ?? "money",
    unit: seed?.unit ?? "total",
    target: seed?.target ?? null,
    limit: seed?.limit ?? null,
    style: seed?.style ?? "balanced",
    trade: seed?.trade ?? "auto",
    days,
    until: seed?.until ?? nowSeconds() + defaultDays * 86_400,
    ench: seed?.req?.ench ?? {},
    dur: seed?.req?.dur ?? null,
    named: seed?.req?.named ?? false,
    cursed: seed?.req?.cursed ?? false,
    custom: seed?.custom ?? [],
    note: seed?.note ?? "",
    src: seed?.src ?? "form",
  };
}

/** The item the bot receives: the item when buying, the paying item when selling for one. */
function receivedOf(state: Pick<FormState, "side" | "item" | "pay">): string | null {
  if (state.side === "buy") return state.item;
  return state.pay !== "money" ? state.pay : null;
}

/** Whether two sets of conditions say the same (enchantments in any order). */
function sameReq(a: NewOrder["req"], b: PlusOrder["req"]): boolean {
  if (a === null || b === null) return a === b;
  const ench = (all: Record<string, number>) =>
    JSON.stringify(Object.entries(all).sort(([x], [y]) => (x < y ? -1 : x > y ? 1 : 0)));
  return (
    a.dur === b.dur && a.named === b.named && a.cursed === b.cursed && ench(a.ench) === ench(b.ench)
  );
}

type Problems = Partial<Record<FormField | "far" | "general", string>>;

/** A small search with its results under it: items of the game (and the account's own). */
function ItemSearch({
  label,
  value,
  namer,
  items,
  exclude,
  canPick,
  onPick,
  onInventory,
  error,
}: {
  label: string;
  value: string | null;
  namer: Namer;
  items: ItemDataState;
  /** A reference not to offer (the item itself as its own payment). */
  exclude?: string | null;
  canPick: boolean;
  onPick: (ref: string) => void;
  onInventory: () => void;
  error?: string | null;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  const id = useId();
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(value === null);
  const [active, setActive] = useState(0);
  useEffect(() => setOpen(value === null), [value]);
  const extras = useMemo(
    () => namer.custom.map((item) => ({ ref: `custom:${item.k}`, names: [item.name] })),
    [namer.custom],
  );
  const hits = useMemo(
    () =>
      searchItems(items.data, query, { lang, extras, limit: 8 }).filter(
        (hit) => hit.ref !== exclude,
      ),
    [items.data, query, lang, extras, exclude],
  );
  const pick = (ref: string) => {
    onPick(ref);
    setQuery("");
    setOpen(false);
  };
  const info = itemInfo(value, items.data);
  const hint = info && items.data ? itemHint(info, items.data, lang) : undefined;

  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-[13px] font-medium text-bone-100">
        {label}
      </label>
      {!open && value ? (
        <div className="flex items-center gap-3 rounded-md bg-obsidian-800/80 p-1.5 pr-2 ring-1 ring-white/8 ring-inset">
          <PlusItemIcon item={value} custom={namer.custom} size={32} />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13.5px] font-medium text-bone-50">
              {itemLabel(value, namer, lang)}
            </span>
            {hint || value.startsWith("custom:") ? (
              <span className="block truncate text-[12px] text-ash-400">
                {value.startsWith("custom:") ? t("afkPlus.form.customItem") : hint}
              </span>
            ) : null}
          </span>
          <Button size="sm" variant="ghost" onClick={() => setOpen(true)}>
            {t("afkPlus.form.change")}
          </Button>
        </div>
      ) : (
        <div className="flex flex-col gap-1.5">
          <div className="relative">
            <MagnifyingGlass
              size={14}
              aria-hidden
              className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-ash-400"
            />
            <input
              id={id}
              role="combobox"
              aria-expanded={hits.length > 0}
              aria-controls={`${id}-list`}
              aria-activedescendant={hits[active] ? `${id}-${active}` : undefined}
              aria-invalid={error ? true : undefined}
              autoComplete="off"
              spellCheck={false}
              value={query}
              placeholder={t("afkPlus.form.search")}
              onChange={(event) => {
                setQuery(event.target.value);
                setActive(0);
              }}
              onKeyDown={(event) => {
                if (event.key === "ArrowDown") {
                  event.preventDefault();
                  setActive((n) => Math.min(hits.length - 1, n + 1));
                } else if (event.key === "ArrowUp") {
                  event.preventDefault();
                  setActive((n) => Math.max(0, n - 1));
                } else if (event.key === "Enter" && hits[active]) {
                  event.preventDefault();
                  pick(hits[active].ref);
                } else if (event.key === "Escape" && value) {
                  event.stopPropagation();
                  setOpen(false);
                }
              }}
              className={cn(
                inputClass,
                "h-[34px] pr-3 pl-8",
                error && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
              )}
            />
          </div>
          {/* Under its own field, not under the links after the results. */}
          {error ? (
            <p role="alert" className="text-[12px] text-flare-400">
              {error}
            </p>
          ) : null}
          {query.trim() ? (
            hits.length ? (
              <div
                id={`${id}-list`}
                role="listbox"
                aria-label={label}
                className="flex max-h-[232px] flex-col overflow-y-auto rounded-md bg-obsidian-900/90 p-1 ring-1 ring-white/8 [scrollbar-width:thin]"
              >
                {hits.map((hit, index) => {
                  const name = itemLabel(hit.ref, namer, lang);
                  const line = hit.item
                    ? (hit.item.desc?.[lang] ??
                      (items.data ? itemHint(hit.item, items.data, lang) : undefined))
                    : t("afkPlus.form.customItem");
                  return (
                    // The focus stays in the field (`aria-activedescendant`); a click picks.
                    <button
                      type="button"
                      key={hit.ref}
                      id={`${id}-${index}`}
                      role="option"
                      aria-selected={index === active}
                      tabIndex={-1}
                      onPointerDown={(event) => event.preventDefault()}
                      onClick={() => pick(hit.ref)}
                      onPointerMove={() => setActive(index)}
                      className={cn(
                        "flex w-full cursor-pointer items-center gap-2.5 rounded-[6px] px-1.5 py-1 text-left outline-none",
                        index === active ? "bg-white/8" : null,
                      )}
                    >
                      <PlusItemIcon item={hit.ref} custom={namer.custom} size={24} />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[13px] text-bone-50">{name}</span>
                        {line ? (
                          <span className="block truncate text-[11.5px] text-ash-400">{line}</span>
                        ) : null}
                      </span>
                    </button>
                  );
                })}
              </div>
            ) : (
              <p className="text-[12.5px] text-ash-300">{t("afkPlus.form.noMatch")}</p>
            )
          ) : null}
          {items.failed ? (
            <p className="flex flex-wrap items-center gap-x-2 text-[12.5px] text-ash-300">
              {t("afkPlus.form.dataFailed")}
              <button
                type="button"
                onClick={items.retry}
                className="cursor-pointer rounded-sm font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
              >
                {t("afkPlus.form.retry")}
              </button>
            </p>
          ) : null}
          <div className="flex items-center gap-4">
            <button
              type="button"
              disabled={!canPick}
              onClick={onInventory}
              className="cursor-pointer rounded-sm text-[12.5px] font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300 disabled:cursor-not-allowed disabled:text-ash-500 disabled:no-underline"
            >
              {t("afkPlus.intake.pick")}
            </button>
            {value ? (
              <button
                type="button"
                onClick={() => setOpen(false)}
                className="cursor-pointer rounded-sm text-[12.5px] text-ash-300 outline-none hover:text-bone-100 focus-visible:ring-2 focus-visible:ring-gold-300"
              >
                {t("afkPlus.form.keep")}
              </button>
            ) : null}
          </div>
        </div>
      )}
      {error && !open && value ? (
        <p role="alert" className="text-[12px] text-flare-400">
          {error}
        </p>
      ) : null}
    </div>
  );
}

/** The enchantments a received item must carry, each at least its level. */
function EnchantPicker({
  item,
  data,
  value,
  onChange,
}: {
  item: ItemInfo;
  data: NonNullable<ItemDataState["data"]>;
  value: Record<string, number>;
  onChange: (next: Record<string, number>) => void;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  const [adding, setAdding] = useState(false);
  const chosen = Object.entries(value)
    .map(([id, level]) => ({ info: data.enchantmentById.get(id), id, level }))
    .filter((entry): entry is { info: EnchantmentInfo; id: string; level: number } =>
      Boolean(entry.info),
    );
  // Curses are the "Lanetli olabilir" switch; one that clashes with a chosen one is not offered.
  const offered = enchantmentsFor(data, item).filter(
    (ench) =>
      !ench.curse &&
      !(ench.id in value) &&
      chosen.every((entry) => canCombine(data, entry.info, ench)),
  );
  const set = (id: string, level: number | null) => {
    const next = { ...value };
    if (level === null) delete next[id];
    else next[id] = level;
    onChange(next);
  };
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[13px] font-medium text-bone-100">
        {t("afkPlus.form.enchantments")}
      </span>
      {chosen.length ? (
        <ul className="flex flex-col gap-1">
          {chosen.map(({ info, id, level }) => (
            <li
              key={id}
              className="flex items-center gap-2 rounded-md bg-white/[0.04] py-1 pr-1 pl-3"
            >
              <span className="min-w-0 flex-1 truncate text-[13px] text-bone-50">
                {info.name[lang]}
                {info.maxLevel > 1 ? ` ${levelText(level)}` : ""}
              </span>
              {info.maxLevel > 1 ? (
                <span className="flex items-center gap-0.5">
                  <IconButton
                    label={t("afkPlus.form.levelDown", { name: info.name[lang] })}
                    icon={<Minus size={14} weight="bold" />}
                    size="sm"
                    disabled={level <= 1}
                    onClick={() => set(id, level - 1)}
                  />
                  <IconButton
                    label={t("afkPlus.form.levelUp", { name: info.name[lang] })}
                    icon={<Plus size={14} weight="bold" />}
                    size="sm"
                    disabled={level >= info.maxLevel}
                    onClick={() => set(id, level + 1)}
                  />
                </span>
              ) : null}
              <IconButton
                label={t("afkPlus.form.removeEnchantment", { name: info.name[lang] })}
                icon={<X size={14} weight="bold" />}
                size="sm"
                onClick={() => set(id, null)}
              />
            </li>
          ))}
        </ul>
      ) : null}
      {adding ? (
        <ul className="flex flex-wrap gap-1.5">
          {offered.map((ench) => (
            <li key={ench.id}>
              <button
                type="button"
                onClick={() => {
                  set(ench.id, ench.maxLevel);
                  setAdding(false);
                }}
                className="chamfer flex h-[28px] cursor-pointer items-center bg-white/[0.06] px-2.5 text-[12.5px] text-bone-100 outline-none [--cut:5px] transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/10 active:scale-[0.97] focus-visible:ring-2 focus-visible:ring-gold-300"
              >
                {ench.name[lang]}
              </button>
            </li>
          ))}
        </ul>
      ) : offered.length && chosen.length < 8 ? (
        <button
          type="button"
          onClick={() => setAdding(true)}
          className="self-start rounded-sm text-[12.5px] font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          {t("afkPlus.form.addEnchantment")}
        </button>
      ) : null}
    </div>
  );
}

function Field({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("flex flex-col gap-1.5", className)}>{children}</div>;
}

function Label({ children }: { children: ReactNode }) {
  return <span className="text-[13px] font-medium text-bone-100">{children}</span>;
}

export function OrderForm({
  accountId,
  request,
  settings,
  namer,
  items,
  canPick,
  onPick,
  onClose,
}: {
  accountId: string;
  request: FormRequest | null;
  settings: PlusSettings;
  namer: Namer;
  items: ItemDataState;
  canPick: boolean;
  onPick: (field: "item" | "pay", then: (picked: PickedItem) => void) => void;
  onClose: () => void;
}) {
  const animate = useInteractionMotion();
  const open = request !== null;
  return (
    <RadixDialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <AnimatePresence>
        {request ? (
          <EntranceGate key="afk-plus-order-form">
            <RadixDialog.Portal forceMount>
              <RadixDialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-50 bg-obsidian-950/45"
                  {...scrimMotion(!animate)}
                />
              </RadixDialog.Overlay>
              <RadixDialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  focusPanel(event.currentTarget as HTMLElement);
                }}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-50 flex w-[min(480px,92vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...sheetMotion(!animate)}
                >
                  <FormBody
                    accountId={accountId}
                    request={request}
                    settings={settings}
                    namer={namer}
                    items={items}
                    canPick={canPick}
                    onPick={onPick}
                    onClose={onClose}
                  />
                </GatedMotion.aside>
              </RadixDialog.Content>
            </RadixDialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </RadixDialog.Root>
  );
}

function FormBody({
  accountId,
  request,
  settings,
  namer,
  items,
  canPick,
  onPick,
  onClose,
}: {
  accountId: string;
  request: FormRequest;
  settings: PlusSettings;
  namer: Namer;
  items: ItemDataState;
  canPick: boolean;
  onPick: (field: "item" | "pay", then: (picked: PickedItem) => void) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  const writes = useOrderWrites(accountId);
  const { config, server, rules } = settings;
  const [state, setState] = useState(() => stateOf(request, rules.orderDaysDefault));
  const [problems, setProblems] = useState<Problems>(() =>
    problemsOfIssues(request.issues ?? [], t, sideOfRequest(request)),
  );
  const [farOk, setFarOk] = useState(false);
  const [saving, setSaving] = useState(false);
  const refs = useRef<Partial<Record<FormField, HTMLElement | null>>>({});
  const all: Namer = { ...namer, custom: [...namer.custom, ...state.custom] };
  const editing = request.kind === "edit";
  const itemPay = rules.features.itemPay && config.pay !== "money";
  const moneyOk = config.pay !== "items";
  const counts = state.pay !== "money";
  const payName = counts ? itemLabel(state.pay, all, lang) : null;
  const maxDeal = counts ? MAX_COUNT : server.money.maxDeal;
  const received = receivedOf(state);
  // Enchantments only for a game item (a custom item's are the server's); durability and stacks
  // by a custom item's material too. Unknown (no data yet): the stored durability stays.
  const receivedInfo = itemInfo(received, items.data);
  const receivedBase = baseInfo(received, items.data, all.custom);
  const wears = receivedBase ? receivedBase.maxDamage > 0 : null;
  const stack = baseInfo(state.item, items.data, all.custom)?.stack ?? 64;
  const botTrades = server.trade === "bot";
  const newCustom = state.custom.find(
    (item) => `custom:${item.k}` === state.item || `custom:${item.k}` === state.pay,
  );
  // 7, 14 and 30 days (to Studio's most), and the days the player's words gave.
  const [seeded] = useState(() => (request.kind === "new" ? state.days : null));
  const dayChoices = [
    ...new Set([
      ...DAYS.filter((d) => d <= rules.orderDaysMax),
      ...(seeded !== null && seeded <= rules.orderDaysMax ? [seeded] : []),
    ]),
  ].sort((a, b) => a - b);

  // Opened for one part: that field gets the focus and comes into view.
  // biome-ignore lint/correctness/useExhaustiveDependencies: once, when the form opens
  useEffect(() => {
    const field = request.focus;
    if (!field) return;
    const element = refs.current[field];
    window.requestAnimationFrame(() => {
      element?.scrollIntoView({ block: "center" });
      (element?.matches("input,button")
        ? element
        : element?.querySelector<HTMLElement>("input,button")
      )?.focus();
    });
  }, []);

  const set = (patch: Partial<FormState>) => {
    setState((current) => {
      const next = { ...current, ...patch };
      // Enchantments belong to the item the bot receives: another one starts them again (a picked
      // item brings its own).
      if (!("ench" in patch) && receivedOf(next) !== receivedOf(current)) next.ench = {};
      return next;
    });
    setProblems({});
  };

  // An amount field's text that is not an amount: the form does not save the last good value.
  const [unreadable, setUnreadable] = useState<Partial<Record<"target" | "limit", boolean>>>({});
  const amountInto = (field: "target" | "limit", value: AmountValue | undefined) => {
    setUnreadable((current) => ({ ...current, [field]: value === undefined }));
    if (value === undefined) setProblems({});
    else set(field === "target" ? { target: value } : { limit: value });
  };

  const pickInto = (field: "item" | "pay") =>
    onPick(field, (picked) => {
      if (picked.kind === "vanilla") {
        // A picked item's enchantments and durability are conditions where the bot receives it.
        const receives = field === "item" ? state.side === "buy" : true;
        set({
          [field]: picked.ref,
          ...(receives ? { ench: picked.req.ench, dur: picked.req.dur } : {}),
        });
      } else if (picked.kind === "known") {
        set({ [field]: picked.ref });
      } else {
        setState((current) => {
          const taken = [
            ...namer.custom,
            ...current.custom.filter((c) => `custom:${c.k}` !== current[field]),
          ];
          const custom = taken.some((c) => c.k === picked.custom.k)
            ? { ...picked.custom, k: newCustomKey(taken) }
            : picked.custom;
          return {
            ...current,
            [field]: `custom:${custom.k}`,
            // The one this field named before goes with it.
            custom: [...current.custom.filter((c) => `custom:${c.k}` !== current[field]), custom],
          };
        });
        setProblems({});
      }
    });

  const validate = (farConfirmed: boolean): Problems => {
    const out: Problems = {};
    if (!state.item) out.item = t("afkPlus.form.errors.item");
    if (state.qty === null || state.qty < 1 || state.qty > MAX_COUNT)
      out.qty = t("afkPlus.form.errors.value");
    if (counts && !state.pay) out.pay = t("afkPlus.form.errors.item");
    else if (counts && state.pay === state.item) out.pay = t("afkPlus.form.errors.same");
    if (unreadable.target) out.target = t("afkPlus.form.errors.amount");
    else if (state.target === null) out.target = t("afkPlus.form.errors.price");
    else if (state.target > maxDeal) out.target = t("afkPlus.form.errors.value");
    if (unreadable.limit) out.limit = t("afkPlus.form.errors.amount");
    else if (state.limit === null) out.limit = t("afkPlus.form.errors.price");
    else if (state.limit > maxDeal) out.limit = t("afkPlus.form.errors.value");
    if (state.target !== null && state.limit !== null && !out.target && !out.limit) {
      if (state.side === "buy" && state.limit < state.target)
        out.limit = t("afkPlus.form.errors.atMost");
      if (state.side === "sell" && state.limit > state.target)
        out.limit = t("afkPlus.form.errors.atLeast");
    }
    if (newCustom) {
      const name = newCustom.name.trim();
      if (!name || [...name].length > CUSTOM_NAME_MAX || hasMarker(name)) {
        out.name = t("afkPlus.form.errors.name");
      } else if (breaksForm(name)) {
        out.name = t("afkPlus.form.errors.nameForm");
      } else if (looksLikeLink(name) || looksLikePhone(name)) {
        out.name = t("afkPlus.form.errors.nameLink");
      }
    }
    if (
      state.side === "buy" &&
      !counts &&
      state.target !== null &&
      state.limit !== null &&
      state.limit > 10 * state.target &&
      !farConfirmed &&
      Object.keys(out).length === 0
    ) {
      out.far = t("afkPlus.questions.far", {
        limit: formatAmount(state.limit),
        target: formatAmount(state.target),
      });
    }
    return out;
  };

  const orderOfState = (): NewOrder | null => {
    if (!state.item || state.qty === null || state.target === null || state.limit === null)
      return null;
    const receives = state.side === "buy" || state.pay !== "money";
    return {
      on: true,
      side: state.side,
      item: state.item,
      qty: state.qty,
      req: receives
        ? {
            ench: fittingEnch(state.ench, received, items.data),
            dur: wears === false ? 0 : (state.dur ?? 90),
            named: state.named,
            cursed: state.cursed,
          }
        : null,
      pay: state.pay,
      unit: state.qty > 1 ? state.unit : "total",
      target: state.target,
      limit: state.limit,
      style: state.style,
      trade: botTrades ? state.trade : "auto",
      until: state.days !== null ? nowSeconds() + state.days * 86_400 : state.until,
      note: state.note,
      src: state.src,
    };
  };

  // One write at a time (Enter in a field submits too, before the disabled button renders).
  const busy = useRef(false);
  // "Evet, doğru" saves at once with the far limit confirmed (the state's flag comes a render later).
  const save = async (farConfirmed = farOk) => {
    if (busy.current) return;
    const found = validate(farConfirmed);
    setProblems(found);
    if (Object.keys(found).length) return;
    const order = orderOfState();
    if (!order) return;
    let outcome = null;
    if (request.kind === "edit") {
      // Only what changed is sent: the rest stays as stored (a condition sent again would end the
      // order's open bargains).
      const before = request.order;
      const reqChanged = !sameReq(order.req, before.req);
      const patch = emptyPatch({
        qty: order.qty !== before.qty ? order.qty : null,
        pay: order.pay !== before.pay ? order.pay : null,
        req: reqChanged ? order.req : null,
        noReq: reqChanged && order.req === null,
        unit: order.unit !== before.unit ? order.unit : null,
        target: order.target !== before.target ? order.target : null,
        limit: order.limit !== before.limit ? order.limit : null,
        style: order.style !== before.style ? order.style : null,
        trade: order.trade !== before.trade ? order.trade : null,
        until: state.days !== null ? order.until : null,
      });
      if (JSON.stringify(patch) === JSON.stringify(emptyPatch({}))) {
        onClose();
        return;
      }
      busy.current = true;
      setSaving(true);
      outcome = await writes.patch(before.id, patch);
    } else {
      const custom = state.custom
        .filter((item) => `custom:${item.k}` === order.item || `custom:${item.k}` === order.pay)
        .map((item) => ({
          k: item.k,
          name: item.name.trim(),
          base: item.base,
          fp: item.fp,
          look: item.look ? { cmd: item.look.cmd ?? null, model: item.look.model ?? null } : null,
          from: item.from,
        }));
      busy.current = true;
      setSaving(true);
      outcome = await writes.create(order, custom, {});
    }
    busy.current = false;
    setSaving(false);
    if (!outcome) return;
    if (outcome.kind === "saved") {
      toast.success(
        t(editing ? "afkPlus.orders.changed" : "afkPlus.orders.saved", {
          item: itemLabel(order.item, all, lang),
        }),
      );
      if (request.kind === "new") request.onSaved?.();
      onClose();
    } else if (outcome.kind === "invalid") {
      setProblems(problemsOfIssues(outcome.issues, t, state.side));
    }
  };

  const refOf = (field: FormField) => (element: HTMLElement | null) => {
    refs.current[field] = element;
  };

  const amountLabel = (value: string) =>
    payName ? t("afkPlus.form.countOf", { label: value, item: payName }) : value;

  // A refusal on a part with no line of its own (the days, the conditions, the style, a payment
  // in money) is said under the form, so "Kaydet" never seems to do nothing.
  const lined = new Set<string>(["item", "qty", "target", "limit", "far", "general"]);
  if (counts) lined.add("pay");
  if (newCustom) lined.add("name");
  const generalProblem =
    problems.general ??
    Object.entries(problems).find(([field, text]) => text && !lined.has(field))?.[1];

  return (
    <>
      <header className="flex items-center gap-3 border-b border-white/8 px-5 py-4">
        <RadixDialog.Title className="min-w-0 flex-1 truncate font-display text-[17px] font-semibold text-bone-50">
          {t(editing ? "afkPlus.form.editTitle" : "afkPlus.form.newTitle")}
        </RadixDialog.Title>
        <IconButton label={t("common.close")} icon={<X size={18} />} size="sm" onClick={onClose} />
      </header>
      <form
        className="flex min-h-0 flex-1 flex-col"
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-5 py-5 [scrollbar-gutter:stable] [scrollbar-width:thin]">
          <div ref={refOf("side")}>
            <Segmented
              label={t("afkPlus.form.side")}
              value={state.side}
              disabled={editing}
              onValueChange={(side) => set({ side })}
              options={[
                { value: "buy", label: t("afkPlus.form.buy") },
                { value: "sell", label: t("afkPlus.form.sell") },
              ]}
            />
          </div>

          <div ref={refOf("item")}>
            {editing ? (
              <Field>
                <Label>{t("afkPlus.form.item")}</Label>
                <div className="flex items-center gap-3">
                  <PlusItemIcon item={state.item ?? ""} custom={all.custom} size={32} />
                  <span className="text-[13.5px] font-medium text-bone-50">
                    {state.item ? itemLabel(state.item, all, lang) : ""}
                  </span>
                </div>
              </Field>
            ) : (
              <ItemSearch
                label={t("afkPlus.form.item")}
                value={state.item}
                namer={all}
                items={items}
                canPick={canPick}
                onPick={(ref) => set({ item: ref })}
                onInventory={() => pickInto("item")}
                error={problems.item}
              />
            )}
          </div>

          {newCustom ? (
            <div ref={refOf("name")} className="flex flex-col gap-1.5">
              <label
                className="text-[13px] font-medium text-bone-100"
                htmlFor="afk-plus-custom-name"
              >
                {t("afkPlus.form.customName")}
              </label>
              <div className="flex items-center gap-3">
                <ItemTile
                  id={newCustom.base ?? "custom:unseen"}
                  cmd={newCustom.look?.cmd}
                  model={newCustom.look?.model}
                  size={32}
                />
                <input
                  id="afk-plus-custom-name"
                  value={newCustom.name}
                  maxLength={CUSTOM_NAME_MAX}
                  autoComplete="off"
                  spellCheck={false}
                  aria-invalid={problems.name ? true : undefined}
                  onChange={(event) => {
                    const name = event.target.value;
                    setState((current) => ({
                      ...current,
                      custom: current.custom.map((c) => (c.k === newCustom.k ? { ...c, name } : c)),
                    }));
                    setProblems({});
                  }}
                  className={cn(
                    inputClass,
                    "h-[34px] min-w-0 flex-1 px-3",
                    problems.name &&
                      "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
                  )}
                />
              </div>
              <p className={cn("text-[12px]", problems.name ? "text-flare-400" : "text-ash-400")}>
                {problems.name ??
                  (newCustom.fp
                    ? t("afkPlus.form.customKnown")
                    : t("afkPlus.form.customFirstSight"))}
              </p>
            </div>
          ) : null}

          <div className="grid grid-cols-2 gap-x-4 gap-y-4">
            <div ref={refOf("qty")}>
              <IntField
                label={t("afkPlus.form.qty")}
                value={state.qty ?? 0}
                onChange={(qty) => set({ qty })}
                error={problems.qty}
                inputClassName="w-[120px]"
                after={
                  // Whole stacks only ("64" is "1 yığın"; 70 says nothing).
                  stack > 1 && state.qty && state.qty % stack === 0
                    ? t("afkPlus.form.stacks", { count: state.qty / stack })
                    : undefined
                }
              />
            </div>
            {itemPay && moneyOk ? (
              <div ref={refOf("pay")}>
                <Field>
                  <Label>{t("afkPlus.form.inReturn")}</Label>
                  <Segmented
                    className="self-start"
                    label={t("afkPlus.form.inReturn")}
                    value={counts ? "item" : "money"}
                    onValueChange={(kind) =>
                      set(
                        kind === "money"
                          ? { pay: "money", target: null, limit: null }
                          : { pay: "", target: null, limit: null },
                      )
                    }
                    options={[
                      { value: "money", label: t("afkPlus.form.money") },
                      { value: "item", label: t("afkPlus.form.payItem") },
                    ]}
                  />
                </Field>
              </div>
            ) : null}
          </div>
          {counts ? (
            <ItemSearch
              label={t("afkPlus.form.payWith")}
              value={state.pay || null}
              namer={all}
              items={items}
              exclude={state.item}
              canPick={canPick}
              onPick={(ref) => set({ pay: ref })}
              onInventory={() => pickInto("pay")}
              error={problems.pay}
            />
          ) : null}

          {(state.qty ?? 0) > 1 ? (
            <Field>
              <Label>{t("afkPlus.form.unit")}</Label>
              <Segmented
                className="self-start"
                label={t("afkPlus.form.unit")}
                value={state.unit}
                onValueChange={(unit) => set({ unit })}
                options={[
                  { value: "each", label: t("afkPlus.form.each") },
                  { value: "total", label: t("afkPlus.form.total") },
                ]}
              />
            </Field>
          ) : null}

          <div className="grid grid-cols-2 gap-x-4">
            <div ref={refOf("target")}>
              <AmountField
                label={amountLabel(t("afkPlus.form.target"))}
                value={state.target}
                optional
                onChange={(value) => amountInto("target", value)}
                error={problems.target}
              />
            </div>
            <div ref={refOf("limit")}>
              <AmountField
                label={amountLabel(
                  t(state.side === "buy" ? "afkPlus.form.atMost" : "afkPlus.form.atLeast"),
                )}
                value={state.limit}
                optional
                onChange={(value) => amountInto("limit", value)}
                error={problems.limit}
              />
            </div>
          </div>
          {!counts && !server.money.label ? (
            <p className="-mt-3 text-[12px] text-ash-400">{t("afkPlus.form.moneyBare")}</p>
          ) : null}

          <div ref={refOf("style")}>
            <Field>
              <Label>{t("afkPlus.form.style")}</Label>
              <Segmented
                className="self-start"
                label={t("afkPlus.form.style")}
                value={state.style}
                onValueChange={(style) => set({ style })}
                options={[
                  { value: "patient", label: t("afkPlus.form.styles.patient") },
                  { value: "balanced", label: t("afkPlus.form.styles.balanced") },
                  { value: "quick", label: t("afkPlus.form.styles.quick") },
                ]}
              />
            </Field>
          </div>

          <div ref={refOf("days")}>
            <Field>
              <Label>{t("afkPlus.form.valid")}</Label>
              <Segmented
                className="self-start"
                label={t("afkPlus.form.valid")}
                value={state.days === null ? "keep" : String(state.days)}
                onValueChange={(days) => set({ days: days === "keep" ? null : Number(days) })}
                options={[
                  ...(editing ? [{ value: "keep", label: t("afkPlus.form.validKeep") }] : []),
                  ...dayChoices.map((d) => ({
                    value: String(d),
                    label: t("afkPlus.form.days", { count: d }),
                  })),
                ]}
              />
            </Field>
          </div>

          {botTrades ? (
            <div ref={refOf("trade")}>
              <Field>
                <Label>{t("afkPlus.form.who")}</Label>
                <Segmented
                  className="self-start"
                  label={t("afkPlus.form.who")}
                  value={state.trade}
                  onValueChange={(trade) => set({ trade })}
                  options={[
                    { value: "auto", label: t("afkPlus.form.whoBot") },
                    { value: "owner", label: t("afkPlus.form.whoMe") },
                  ]}
                />
              </Field>
            </div>
          ) : (
            <p className="text-[12.5px] text-ash-300">{t("afkPlus.card.ownerHere")}</p>
          )}

          {received ? (
            <div ref={refOf("req")} className="flex flex-col gap-4 border-t border-white/7 pt-4">
              <p className="font-display text-[14px] font-semibold text-bone-50">
                {t("afkPlus.form.conditions", { item: itemLabel(received, all, lang) })}
              </p>
              {wears ? (
                <IntField
                  label={t("afkPlus.form.durability")}
                  value={state.dur ?? 90}
                  onChange={(dur) => set({ dur: dur === null ? null : Math.min(100, dur) })}
                  unit="%"
                  // Turkish writes the sign first ("%90"), as the cards do.
                  unitFirst={lang === "tr"}
                  className="self-start"
                  inputClassName="w-[96px]"
                />
              ) : null}
              {receivedInfo && items.data && receivedInfo.enchantments.length ? (
                <EnchantPicker
                  item={receivedInfo}
                  data={items.data}
                  value={state.ench}
                  onChange={(ench) => set({ ench })}
                />
              ) : null}
              <Switch
                label={t("afkPlus.form.named")}
                checked={state.named}
                onCheckedChange={(named) => set({ named })}
              />
              <Switch
                label={t("afkPlus.form.cursed")}
                checked={state.cursed}
                onCheckedChange={(cursed) => set({ cursed })}
              />
            </div>
          ) : null}
        </div>
        <footer className="flex flex-col gap-2 border-t border-white/8 px-5 py-3.5">
          {problems.far ? (
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="min-w-0 flex-1 text-[12.5px] text-bone-100">{problems.far}</p>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => {
                  setFarOk(true);
                  setProblems({});
                  void save(true);
                }}
              >
                {t("afkPlus.questions.yesRight")}
              </Button>
            </div>
          ) : generalProblem ? (
            <p role="alert" className="text-[12.5px] text-flare-300">
              {generalProblem}
            </p>
          ) : null}
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={onClose}>
              {t("afkPlus.card.discard")}
            </Button>
            <Button type="submit" variant="primary" disabled={saving}>
              {t("afkPlus.card.save")}
            </Button>
          </div>
        </footer>
      </form>
    </>
  );
}

/** The side a form opens on (for the words of an order refusal). */
function sideOfRequest(request: FormRequest): "buy" | "sell" {
  if (request.kind === "edit") return request.order.side === "sell" ? "sell" : "buy";
  return request.seed?.side ?? "buy";
}

/** The service's (or the launcher's) refusal on the form's fields. */
function problemsOfIssues(
  issues: readonly PlusIssue[],
  t: (key: string, values?: Record<string, unknown>) => string,
  side: "buy" | "sell",
): Problems {
  const out: Problems = {};
  for (const issue of issues) {
    const field = issue.field;
    if (issue.reason === "limit_orders") {
      out.general = t(
        issue.problem === "stored" ? "afkPlus.form.errors.stored" : "afkPlus.form.errors.active",
      );
      continue;
    }
    if (field.startsWith("customItem")) {
      out.name = t(
        issue.problem === "link" || issue.problem === "phone"
          ? "afkPlus.form.errors.nameLink"
          : issue.problem === "form"
            ? "afkPlus.form.errors.nameForm"
            : "afkPlus.form.errors.name",
      );
      continue;
    }
    const part = field.replace(/^order\./, "").split(".")[0] ?? "";
    const key = (
      {
        item: "item",
        qty: "qty",
        pay: "pay",
        target: "target",
        limit: "limit",
        style: "style",
        trade: "trade",
        until: "days",
        req: "req",
      } as Record<string, FormField>
    )[part];
    const text =
      issue.reason === "order"
        ? t(side === "sell" ? "afkPlus.form.errors.atLeast" : "afkPlus.form.errors.atMost")
        : issue.reason === "same"
          ? t("afkPlus.form.errors.same")
          : t("afkPlus.form.errors.value");
    if (key) out[key] = text;
    else out.general = text;
  }
  return out;
}
