// Cloud AFK notices in words, as the in-game card says them (pl-overlay afk.rs): the bot account's
// name over one line about what happened, and where "Aç" leads. Pure, so tests read it directly.
import type { TFunction } from "i18next";
import type { AfkNotice } from "@/lib/api/social-types";
import { moderationNoticeText } from "../moderation";
import { atPhrase } from "./time";
import type { PlusSection } from "./use-plus";
import { dealText, formatAmount, goodsText, type Lang } from "./words";

/** Where a notice's "Aç" (or a click on its Windows notification) leads. */
export type Target = { panel: PlusSection } | { page: true };

export interface NoticeWords {
  title: string;
  body: string;
  target: Target;
}

/**
 * The item's name: the item data's (or the account's own for a custom item) when the launcher knew
 * it, else its id made readable; a custom item whose name was not read yet is "Özel eşya" (never
 * its key), as on the in-game card.
 */
function itemOf(notice: AfkNotice, lang: Lang): string | null {
  const named = notice.itemName ? (lang === "en" ? notice.itemName.en : notice.itemName.tr) : "";
  if (named.trim()) return named.trim();
  const id = notice.data.item;
  if (!id) return null;
  if (id.startsWith("custom:")) return lang === "en" ? "Custom item" : "Özel eşya";
  const words = (id.split(":").pop() ?? id).replace(/_/g, " ").trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : null;
}

const whole = (text: string | undefined): number | null =>
  text !== undefined && /^\d+$/.test(text.trim()) ? Number(text.trim()) : null;

/**
 * "1 Elytra = 37.500" / "Elytra = 37,500": what a deal or an approval is about, in the deal's
 * notation. A deal paid with an item (`pay` not `money`: `price` counts that item) says only what
 * changes hands.
 */
function dealOf(notice: AfkNotice, lang: Lang): string | null {
  const item = itemOf(notice, lang);
  if (!item) return null;
  const qty = notice.data.qty?.trim() || "1";
  const pay = notice.data.pay?.trim();
  if (pay && pay !== "money") return goodsText(qty, item, lang);
  const price = whole(notice.data.price);
  return price === null
    ? goodsText(qty, item, lang)
    : dealText(qty, item, formatAmount(price, lang), lang);
}

/**
 * Events of every 7/24 Bulut account, AFK+ or not: titled with the account's name (without one,
 * the cloud's name), never "AFK+" (AFK+ that is off shows nowhere). The assistant's own events say
 * whose assistant it is, as its panel does ("Ahmet_AFK için ProBot AI").
 */
const PLAIN_EVENTS = new Set(["stopped", "expired", "renewal", "night", "moderation"]);

/** A notice in words, or null for one this launcher does not show (an unknown event). */
export function noticeWords(notice: AfkNotice, t: TFunction, lang: Lang): NoticeWords | null {
  const plain = PLAIN_EVENTS.has(notice.event);
  const name = notice.data.name?.trim();
  const fallback = plain ? t("cloudAfk.title") : t("afkPlus.notices.title");
  const title = name ? (plain ? name : t("afkPlus.notices.titleOf", { name })) : fallback;
  const at = (body: string | null, target: Target): NoticeWords | null =>
    body ? { title, body, target } : null;
  const data = notice.data;
  switch (notice.event) {
    case "approval": {
      // An order's handoff and first sight wait in its row (never the counterpart's name).
      if (data.approvalKind === "handoff") {
        return at(t("afkPlus.notices.handoff"), { panel: "orders" });
      }
      if (data.approvalKind === "confirm_item") {
        const item = itemOf(notice, lang);
        return at(item && t("afkPlus.notices.confirmItem", { item }), { panel: "orders" });
      }
      const deal = dealOf(notice, lang);
      return at(deal && t("afkPlus.notices.approval", { deal }), { panel: "feed" });
    }
    case "deal_done": {
      const deal = dealOf(notice, lang);
      const key = data.side === "sell" ? "afkPlus.notices.dealSold" : "afkPlus.notices.dealBought";
      return at(deal && t(key, { deal }), { panel: "feed" });
    }
    case "deal_failed": {
      const item = itemOf(notice, lang);
      return at(item && t("afkPlus.notices.dealFailed", { item }), { panel: "feed" });
    }
    case "found": {
      const item = itemOf(notice, lang);
      const price = whole(data.price);
      if (!item) return null;
      return at(
        price === null
          ? t("afkPlus.notices.found", { item })
          : t("afkPlus.notices.foundPrice", { item, price: formatAmount(price, lang) }),
        { panel: "feed" },
      );
    }
    case "stopped":
      return at(t("afkPlus.notices.stopped"), { page: true });
    case "expired":
      return at(t("afkPlus.notices.expired"), { page: true });
    case "renewal": {
      const days = whole(data.days);
      return at(
        days === null
          ? t("afkPlus.notices.renewalSoon")
          : t("afkPlus.notices.renewal", { count: days }),
        { page: true },
      );
    }
    case "quota": {
      if (data.which === "full") {
        const until = whole(data.until);
        return at(
          until === null
            ? t("afkPlus.notices.quotaFull")
            : t("afkPlus.notices.quotaFullUntil", {
                when: atPhrase(until, Date.now(), lang, t("afkPlus.time.tomorrow")),
              }),
          { panel: "feed" },
        );
      }
      if (data.which === "80") return at(t("afkPlus.notices.quota80"), { panel: "feed" });
      if (data.which === "gift") return at(t("afkPlus.notices.giftEnding"), { panel: "feed" });
      // AFK+ extra credits: the windows are full and the bought credits pay now (no surprise).
      if (data.which === "bought") return at(t("afkPlus.notices.boughtInUse"), { panel: "feed" });
      return null;
    }
    case "incident":
      return at(t("afkPlus.notices.incident"), { panel: "messages" });
    case "message": {
      const count = whole(data.count) ?? 1;
      return at(
        count > 1 ? t("afkPlus.notices.messages", { count }) : t("afkPlus.notices.message"),
        { panel: "messages" },
      );
    }
    case "paused":
      return at(t("afkPlus.notices.paused"), { panel: "feed" });
    case "summary": {
      const pms = whole(data.pms) ?? 0;
      const deals = whole(data.deals) ?? 0;
      return at(
        t("afkPlus.notices.summary", {
          pms: t("afkPlus.notices.summaryPms", { count: pms }),
          deals: t("afkPlus.notices.summaryDeals", { count: deals }),
        }),
        { panel: "feed" },
      );
    }
    case "weekly":
      return at(t("afkPlus.notices.weekly", { count: whole(data.deals) ?? 0 }), {
        panel: "report",
      });
    case "night":
      return { title: fallback, body: t("afkPlus.notices.night"), target: { page: true } };
    // Moderation (README "Moderation player API" → Notices): one line, the account's card.
    case "moderation":
      return at(moderationNoticeText(data, t), { page: true });
    default:
      return null;
  }
}
