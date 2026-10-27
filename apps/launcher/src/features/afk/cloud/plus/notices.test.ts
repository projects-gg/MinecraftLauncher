import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import type { AfkNotice } from "@/lib/api/social-types";
import { noticeWords } from "./notice-words";

/** A `t` that shows the key and its values, so the test reads what the words would say. */
const t = ((key: string, values?: Record<string, unknown>) =>
  values ? `${key} ${JSON.stringify(values)}` : key) as unknown as TFunction;

function notice(
  event: string,
  data: Record<string, string>,
  extra: Partial<AfkNotice> = {},
): AfkNotice {
  return {
    id: "01JBXK4V7Q2M9R3T5W8Y0ZA1BC",
    event,
    kind: "afk_deal",
    priority: "normal",
    at: 1_792_521_000,
    quiet: false,
    data,
    ...extra,
  };
}

describe("AFK notices in words", () => {
  it("title a notice with the bot account's name, or AFK+ / the cloud's name", () => {
    expect(noticeWords(notice("stopped", { name: "Ahmet_AFK" }), t, "tr")?.title).toBe("Ahmet_AFK");
    // The assistant's own events say whose assistant it is, as its panel does.
    expect(noticeWords(notice("paused", { name: "Ahmet_AFK" }), t, "tr")?.title).toBe(
      'afkPlus.notices.titleOf {"name":"Ahmet_AFK"}',
    );
    expect(noticeWords(notice("paused", {}), t, "tr")?.title).toBe("afkPlus.notices.title");
    // A plain account's events never say "AFK+" (AFK+ may be off).
    for (const event of ["stopped", "expired", "renewal", "night"]) {
      expect(noticeWords(notice(event, {}), t, "tr")?.title, event).toBe("cloudAfk.title");
    }
  });

  it("say what an approval is about, with the amount in the language's grouping", () => {
    const words = noticeWords(
      notice(
        "approval",
        { name: "Ahmet_AFK", acct: "a_1", item: "minecraft:elytra", qty: "1", price: "37500" },
        { itemName: { tr: "Elytra", en: "Elytra" } },
      ),
      t,
      "tr",
    );
    expect(words?.body).toBe('afkPlus.notices.approval {"deal":"1 Elytra = 37.500"}');
    expect(words?.target).toEqual({ panel: "feed" });
    const english = noticeWords(
      notice("approval", { item: "minecraft:elytra", price: "37500" }),
      t,
      "en",
    );
    // English names are singular: the count reads "×", never "64 Diamond".
    expect(english?.body).toBe('afkPlus.notices.approval {"deal":"Elytra = 37,500"}');
  });

  it("say an order's handoff without the counterpart and a first sight with the item", () => {
    const handoff = noticeWords(
      notice("approval", {
        name: "Ahmet_AFK",
        approvalKind: "handoff",
        item: "custom:c1",
        qty: "2",
      }),
      t,
      "tr",
    );
    expect(handoff).toMatchObject({ body: "afkPlus.notices.handoff", target: { panel: "orders" } });
    const seen = noticeWords(
      notice(
        "approval",
        { approvalKind: "confirm_item", item: "custom:c2" },
        { itemName: { tr: "Ejder Kılıcı", en: "Ejder Kılıcı" } },
      ),
      t,
      "en",
    );
    expect(seen?.body).toBe('afkPlus.notices.confirmItem {"item":"Ejder Kılıcı"}');
    // Its name not read yet: never the key.
    const unread = noticeWords(
      notice("approval", { approvalKind: "confirm_item", item: "custom:c2" }),
      t,
      "tr",
    );
    expect(unread?.body).toBe('afkPlus.notices.confirmItem {"item":"Özel eşya"}');
    // Paid with an item: what changes hands, never the count as money.
    const swap = noticeWords(
      notice("approval", {
        item: "minecraft:netherite_ingot",
        qty: "2",
        price: "9",
        pay: "minecraft:diamond",
      }),
      t,
      "tr",
    );
    expect(swap?.body).toBe('afkPlus.notices.approval {"deal":"2 Netherite ingot"}');
  });

  it("read an item without a known name from its id", () => {
    const words = noticeWords(notice("deal_failed", { item: "minecraft:diamond_sword" }), t, "tr");
    expect(words?.body).toBe('afkPlus.notices.dealFailed {"item":"Diamond sword"}');
  });

  it("tell a sale from a buy", () => {
    const sold = noticeWords(
      notice("deal_done", { side: "sell", item: "minecraft:diamond", qty: "64", price: "7680" }),
      t,
      "tr",
    );
    expect(sold?.body).toBe('afkPlus.notices.dealSold {"deal":"64 Diamond = 7.680"}');
  });

  it("count new messages and open the messages", () => {
    expect(noticeWords(notice("message", { count: "3" }), t, "tr")?.body).toBe(
      'afkPlus.notices.messages {"count":3}',
    );
    expect(noticeWords(notice("message", {}), t, "tr")?.body).toBe("afkPlus.notices.message");
    expect(noticeWords(notice("message", {}), t, "tr")?.target).toEqual({ panel: "messages" });
  });

  it("word the allowance by which window ran out", () => {
    expect(noticeWords(notice("quota", { which: "80" }), t, "tr")?.body).toBe(
      "afkPlus.notices.quota80",
    );
    expect(noticeWords(notice("quota", { which: "full" }), t, "tr")?.body).toBe(
      "afkPlus.notices.quotaFull",
    );
    expect(noticeWords(notice("quota", { which: "bought" }), t, "tr")?.body).toBe(
      "afkPlus.notices.boughtInUse",
    );
    expect(noticeWords(notice("quota", { which: "odd" }), t, "tr")).toBeNull();
  });

  it("show nothing for an event it does not know, or a deal without an item", () => {
    expect(noticeWords(notice("later", {}), t, "tr")).toBeNull();
    expect(noticeWords(notice("approval", { price: "100" }), t, "tr")).toBeNull();
  });

  it("send account notices to the AFK page", () => {
    expect(noticeWords(notice("renewal", { days: "7" }), t, "tr")).toMatchObject({
      body: 'afkPlus.notices.renewal {"count":7}',
      target: { page: true },
    });
    expect(noticeWords(notice("renewal", {}), t, "tr")?.body).toBe("afkPlus.notices.renewalSoon");
  });
});
