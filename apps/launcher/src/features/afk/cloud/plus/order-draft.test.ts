import { parseItemData } from "@pl/ui/items";
import { describe, expect, it } from "vitest";
import {
  answer,
  type Choice,
  type DraftContext,
  type OrderDraft,
  type OrderQuestion,
  orderOf,
  questionsOf,
  statusOf,
} from "./order-draft";
import intake from "./vectors/afkplus-intake-v1.json";
import orders from "./vectors/afkplus-order-v1.json";

// The contract's vectors (projects-studio services/cloud-afk/test-vectors, copied as they are):
// the cards' questions, the chips' effects and the orders they save as; the orders' statuses.

const data = parseItemData(intake.data);

interface VectorCtx {
  money: { maxDeal: number; round: [number, number][] | null };
  pay?: "money" | "items" | "both";
  features?: { itemPay?: boolean; customItems?: boolean };
  limits?: { perDealMax?: number | null; totalBudget?: number | null };
  custom?: DraftContext["custom"];
  trades?: "bot" | "owner";
}

function contextOf(ctx: VectorCtx): DraftContext {
  return {
    data,
    custom: ctx.custom ?? [],
    money: { maxDeal: ctx.money.maxDeal, round: ctx.money.round ?? null },
    pay: ctx.pay ?? "money",
    features: {
      itemPay: ctx.features?.itemPay !== false,
      customItems: ctx.features?.customItems !== false,
    },
    limits: {
      perDealMax: ctx.limits?.perDealMax ?? null,
      totalBudget: ctx.limits?.totalBudget ?? null,
    },
    trades: ctx.trades ?? "bot",
  };
}

interface DraftCase {
  name: string;
  ctx: VectorCtx;
  draft?: OrderDraft;
  questions?: OrderQuestion[];
  order?: unknown;
  reject?: string;
}

const drafts = intake.drafts as unknown as DraftCase[];

describe("AFK+ order cards (afkplus-intake-v1.json)", () => {
  it("reads the synthetic item data", () => {
    expect(data?.items.length).toBe(intake.data.items.length);
  });

  it.each(drafts.filter((c) => c.draft).map((c) => [c.name, c] as const))(
    "draft: %s",
    (_name, c) => {
      const ctx = contextOf(c.ctx);
      const draft = c.draft as OrderDraft;
      expect(questionsOf(draft, ctx)).toEqual(c.questions);
      expect(orderOf(draft, ctx)).toEqual(c.order ?? null);
    },
  );

  interface AnswerCase {
    name: string;
    from: string;
    question: OrderQuestion;
    choice: Choice;
    newKey?: string;
    draft?: OrderDraft;
    settings?: unknown;
    questions?: OrderQuestion[];
    order?: unknown;
    form?: string;
    pick?: string;
  }
  const answers = intake.answers as unknown as AnswerCase[];

  it.each(answers.map((c) => [c.name, c] as const))("answer: %s", (_name, c) => {
    const from = drafts.find((d) => d.name === c.from);
    expect(from?.draft, c.from).toBeTruthy();
    if (!from?.draft) return;
    const ctx = contextOf(from.ctx);
    const result = answer(from.draft, c.question, c.choice, ctx, c.newKey);
    if (c.form !== undefined) {
      expect(result).toEqual({ form: c.form });
      return;
    }
    if (c.pick !== undefined) {
      expect(result).toEqual({ pick: c.pick });
      return;
    }
    expect("draft" in result && result.draft).toEqual(c.draft);
    expect("draft" in result ? result.settings : undefined).toEqual(c.settings);
    if (!("draft" in result)) return;
    // The settings change counts for the questions after it.
    const after: DraftContext = {
      ...ctx,
      pay: (c.settings as { pay?: "both" } | undefined)?.pay ?? ctx.pay,
      limits: {
        ...ctx.limits,
        ...((c.settings as { limits?: { perDealMax: number } } | undefined)?.limits ?? {}),
      },
    };
    expect(questionsOf(result.draft, after)).toEqual(c.questions);
    expect(orderOf(result.draft, after)).toEqual(c.order ?? null);
  });
});

describe("AFK+ order status (afkplus-order-v1.json)", () => {
  interface StatusCase {
    name: string;
    order: { side: "buy" | "sell"; qty: number; until: number; on: boolean };
    state: {
      now?: number;
      got: number;
      live: string | null;
      allow: { buy: boolean; sell: boolean };
    };
    status: string;
  }
  const cases = orders.status as unknown as StatusCase[];
  it.each(cases.map((c) => [c.name, c] as const))("%s", (_name, c) => {
    expect(statusOf(c.order, { ...c.state, now: c.state.now ?? orders.now })).toBe(c.status);
  });
});
