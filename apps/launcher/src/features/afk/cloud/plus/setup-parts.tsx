// The parts of the AFK+ questions that the first setup and "Ayarlar" share (SETUP-UX-L): how far the
// spending limits go with the first buy order (the game's slots), the three authorities with their
// items, the private message as another player reads it, and the setup's list of active orders.
import { RadioGroup } from "radix-ui";
import { type ReactNode, useId } from "react";
import { useTranslation } from "react-i18next";
import {
  AUTHORITY_ITEMS,
  type Authority,
  chatAmount,
  fitCounts,
  fitOrder,
  soleBuy,
  talkSample,
} from "../setup/answers";
import { type ChatMessage, GameChat } from "../setup/game-chat";
import { ItemSlot } from "../setup/item-slot";
import { SlotRow } from "../setup/slot-row";
import { PlusItemIcon } from "./item-icon";
import { baseInfo, customOf, itemLabel, type Namer } from "./items";
import { moneyText } from "./orders";
import type { PlusOrder, PlusServer } from "./types";
import { formatAmount, type Lang, trWith } from "./words";

/** The server's money name after amounts ("ripple"), when its profile names one. */
export function moneyNameOf(server: Pick<PlusServer, "money">, lang: Lang): string | null {
  const label = server.money.label;
  return label ? (lang === "en" ? label.en : label.tr) || null : null;
}

/** The caption over a part that shows a result ("Bu sınırlarla", "Özel mesajda böyle görünür"). */
export function Caption({ children }: { children: ReactNode }) {
  return <p className="mb-2 text-[12.5px] text-ash-400">{children}</p>;
}

/**
 * "Bu sınırlarla": the first buy order's item at its highest price, in the game's slots, for one deal
 * and for all of the budget (never more than the order asks for). Nothing without a price per piece
 * or without both limits.
 */
export function MoneyFits({
  limits,
  orders,
  namer,
  lang,
  money,
}: {
  limits: { perDealMax: number | null; totalBudget: number | null };
  orders: readonly PlusOrder[];
  namer: Namer;
  lang: Lang;
  /** The money's name after the price, when the profile names one. */
  money: string | null;
}) {
  const { t } = useTranslation();
  const order = fitOrder(orders);
  const fit = fitCounts(limits, order);
  const sprite = order
    ? order.item.startsWith("custom:")
      ? (customOf(order.item, namer.custom)?.base ?? null)
      : order.item
    : null;
  if (!order || !fit || !sprite) return null;
  const stack = baseInfo(order.item, namer.data, namer.custom)?.stack ?? 1;
  const item = itemLabel(order.item, namer, lang);
  // One deal takes all the order asks for: one row says it (two equal rows would say nothing). The
  // total limit is shared by every buy order: its row only while this is the only one.
  const whole = fit.deal >= order.qty;
  const total = !whole && soleBuy(orders);
  const line = (text: string) => <p className="text-[13px] text-bone-100">{text}</p>;
  return (
    <div className="flex flex-col">
      <Caption>{t("setup.plus.fits")}</Caption>
      <div className="flex flex-col gap-3">
        <div className="flex flex-col gap-1.5">
          <SlotRow item={sprite} count={fit.deal} stack={stack} />
          {line(t(whole ? "setup.plus.fitWhole" : "setup.plus.fitDeal", { count: fit.deal, item }))}
        </div>
        {total ? (
          <div className="flex flex-col gap-1.5">
            <SlotRow item={sprite} count={fit.total} stack={stack} />
            {line(
              t(fit.total >= order.qty ? "setup.plus.fitTotalWhole" : "setup.plus.fitTotal", {
                count: fit.total,
                item,
              }),
            )}
          </div>
        ) : null}
        <p className="text-[12.5px] text-ash-400">
          {t("setup.plus.fitAt", {
            price: money
              ? `${formatAmount(fit.price, lang)} ${money}`
              : formatAmount(fit.price, lang),
          })}
        </p>
      </div>
    </div>
  );
}

/**
 * "ProBot AI ne yapsın?": the three authorities as rows, each with its item and one line under it
 * (the choice's result is not plain otherwise). What works only under one of them (the approval
 * limits, under "Pazarlık et ve tamamla") comes after the rows, under that one.
 */
export function AuthorityChoice({
  value,
  onChange,
  children,
}: {
  value: Authority;
  onChange: (value: Authority) => void;
  /** Under the last row while it is chosen. */
  children?: ReactNode;
}) {
  const { t } = useTranslation();
  const id = useId();
  return (
    <div className="flex flex-col gap-1.5">
      <span id={id} className="sr-only">
        {t("afkPlus.settings.authorityLabel")}
      </span>
      <RadioGroup.Root
        aria-labelledby={id}
        value={value}
        onValueChange={(next) => onChange(next as Authority)}
        className="flex flex-col gap-1"
      >
        {(Object.keys(AUTHORITY_ITEMS) as Authority[]).map((choice) => (
          <RadioGroup.Item
            key={choice}
            value={choice}
            className="group flex w-full cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-3 py-2.5 text-left outline-none ring-1 ring-transparent transition-[background-color,box-shadow] duration-150 ring-inset hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-gold-300 data-[state=checked]:bg-white/[0.06] data-[state=checked]:ring-white/12"
          >
            <span
              aria-hidden
              className="flex size-4 shrink-0 items-center justify-center rounded-full border border-white/25 transition-colors duration-150 group-data-[state=checked]:border-bone-100"
            >
              <RadioGroup.Indicator className="size-2 rounded-full bg-bone-50" />
            </span>
            <ItemSlot item={AUTHORITY_ITEMS[choice]} />
            <span className="min-w-0">
              <span className="block text-[13.5px] font-medium text-bone-50">
                {t(`afkPlus.settings.authority.${choice}`)}
              </span>
              <span className="block text-[12.5px] text-pretty text-ash-300">
                {t(`afkPlus.settings.authorityHelp.${choice}`)}
              </span>
            </span>
          </RadioGroup.Item>
        ))}
      </RadioGroup.Root>
      {children}
    </div>
  );
}

/**
 * "Özel mesajda böyle görünür": another player sells what the first buy order buys (an elytra
 * without one) and ProBot AI offers under its price, never past one deal's limit, in the chosen tone
 * (the tag first while it is on), as the game's chat shows a private message. Chat lines carry no
 * money name, as players write them ("9,6k olur mu?").
 */
export function TalkPreview({
  preset,
  playerTalk,
  tag,
  name,
  orders,
  perDealMax,
  namer,
  lang,
}: {
  preset: string;
  playerTalk: boolean;
  /** The tag before its lines, while it is on and reads right. */
  tag: string | null;
  /** The bot account's in-game name. */
  name: string;
  orders: readonly PlusOrder[];
  perDealMax: number | null;
  namer: Namer;
  lang: Lang;
}) {
  const { t } = useTranslation();
  const deal = talkSample(orders, perDealMax);
  const item = deal.item
    ? itemLabel(deal.item, namer, lang).toLocaleLowerCase(lang === "tr" ? "tr-TR" : "en-GB")
    : null;
  const amount = playerTalk ? chatAmount(deal.offer, lang) : formatAmount(deal.offer, lang);
  const offer = t(`setup.plus.${playerTalk ? "samplesPlayer" : "samples"}.${preset}`, {
    amount,
    amountDat: lang === "tr" ? trWith(amount, "dat") : amount,
  });
  const text = tag ? `${tag} ${offer}` : offer;
  const them = item ? t("setup.plus.chatSell", { item }) : t("setup.plus.chatThem");
  const messages: ChatMessage[] = [
    { key: `them:${them}`, who: t("setup.plus.previewPlayer"), text: them, kind: "private" },
    { key: `me:${text}`, who: name, text, kind: "private" },
  ];
  return (
    <div className="flex flex-col">
      <Caption>{t("setup.pmCaption")}</Caption>
      <GameChat messages={messages} />
    </div>
  );
}

/** The setup's list of the orders it will work on: the item, its name, what it does and at what price. */
export function OrderLines({
  orders,
  namer,
  server,
  lang,
}: {
  orders: readonly PlusOrder[];
  namer: Namer;
  server: Pick<PlusServer, "money">;
  lang: Lang;
}) {
  const { t } = useTranslation();
  const shown = orders.filter((order) => order.on);
  if (!shown.length) return null;
  return (
    <div className="flex flex-col">
      <p className="text-[13px] font-semibold text-bone-100">{t("setup.plus.active")}</p>
      <ul className="mt-1 flex flex-col">
        {shown.map((order) => {
          const payName = order.pay !== "money" ? itemLabel(order.pay, namer, lang) : null;
          const price = payName
            ? `${order.limit} ${payName}`
            : moneyText(order.limit, server, lang);
          const limit = t(
            order.side === "buy" ? "afkPlus.orders.metaAtMost" : "afkPlus.orders.metaAtLeast",
            { price },
          );
          const meta = [
            t(order.side === "buy" ? "afkPlus.card.toBuy" : "afkPlus.card.toSell"),
            order.qty > 1
              ? t(order.unit === "each" ? "afkPlus.orders.perPiece" : "afkPlus.orders.inTotal", {
                  terms: limit,
                })
              : limit,
          ].join(", ");
          return (
            <li
              key={order.id}
              className="flex items-center gap-3 border-t border-white/6 py-2.5 first:border-t-0"
            >
              <PlusItemIcon item={order.item} custom={namer.custom} size={32} />
              <span className="min-w-0">
                <span className="block truncate text-[13.5px] font-medium text-bone-50">
                  {itemLabel(order.item, namer, lang)}
                </span>
                <span className="block truncate text-[12.5px] text-ash-300">{meta}</span>
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
