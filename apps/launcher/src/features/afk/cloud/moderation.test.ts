import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import { en } from "@/locales/en";
import { tr } from "@/locales/tr";
import type { CloudAccount } from "./api";
import {
  appealFirst,
  categoryText,
  dayWords,
  moderationLine,
  moderationNoticeText,
  pausedRows,
  refusedCategory,
  refusedText,
  stopHint,
} from "./moderation";
import { noticeWords } from "./plus/notice-words";

// A dictionary looked up like i18next does.
function translator(strings: Record<string, unknown>): TFunction {
  return ((key: string, values?: Record<string, unknown>) => {
    let value: unknown = strings;
    for (const part of key.split("."))
      value = (value as Record<string, unknown> | undefined)?.[part];
    if (typeof value !== "string") throw new Error(`no text for ${key}`);
    return value.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(values?.[name]));
  }) as TFunction;
}
const t = translator(tr);
const te = translator(en);

const CASE = "mc_01JBQ7XK2M9RPV4TNW8HD3CFGZ";
// 15 January 2027, 08:00 UTC: the appeal's deadline and a suspension's end below.
const JAN15 = 1_800_000_000;
const NBSP = String.fromCharCode(0xa0);

const step = (
  patch: Partial<NonNullable<CloudAccount["moderation"]>> = {},
  hold: CloudAccount["hold"] = null,
) => ({
  hold,
  moderation: {
    case: CASE,
    step: "warn",
    category: "advertising",
    at: JAN15 - 86_400,
    until: null,
    appeal: { can: true, until: JAN15, state: "none" },
    ...patch,
  },
});

const suspended: CloudAccount["hold"] = {
  kind: "suspended",
  until: null,
  reason: "abuse",
  case: CASE,
  category: "scam",
  auto: true,
};

describe("cloud AFK moderation on the card", () => {
  it("says the step in force in one line and offers the appeal while it can be sent", () => {
    expect(moderationLine({ moderation: null, hold: null }, t)).toBe(null);
    expect(moderationLine(step(), t)).toEqual({
      text: "Uyarı aldı: Reklam",
      canAppeal: true,
      appealOpen: false,
    });
    // A suspension the hold does not name yet reads as the step itself.
    expect(moderationLine(step({ step: "suspend", category: "automation" }), te)?.text).toBe(
      "Stopped: Automated clicking",
    );
    const open = moderationLine(step({ appeal: { can: false, until: null, state: "open" } }), t);
    expect(open).toMatchObject({ canAppeal: false, appealOpen: true });
    // A step or category this launcher does not know: nothing, or a rule violation (never a key).
    expect(moderationLine(step({ step: "brand_new" }), t)).toBe(null);
    expect(categoryText("brand_new", t)).toBe("Kural ihlali");
  });

  it("says a stop once, on the state line: its hint says when, the appeal, the days", () => {
    const stop = (patch: Partial<NonNullable<CloudAccount["moderation"]>>) =>
      step({ step: "suspend", category: "scam", ...patch }, suspended);
    // The card's main button offers the appeal then, and there is no line of its own.
    expect(appealFirst(stop({}))).toBe(true);
    expect(appealFirst(step({ step: "suspend" }))).toBe(false);
    expect(moderationLine(stop({}), t)).toBe(null);
    expect(stopHint(stop({}), t, "tr")).toBe(
      `15${NBSP}Ocak'a kadar itiraz edebilirsin. Karar kaldırılırsa durduğu günler hesabına eklenir.`,
    );
    expect(stopHint(stop({ until: JAN15 }), te, "en")).toBe(
      `The account opens again on 15${NBSP}January. You can appeal until 15${NBSP}January. If the decision is lifted, the stopped days are added back.`,
    );
    expect(stopHint(stop({ appeal: { can: false, until: null, state: "open" } }), t, "tr")).toBe(
      "İtirazın inceleniyor. Karar kaldırılırsa durduğu günler hesabına eklenir.",
    );
    expect(
      stopHint(
        stop({ until: JAN15, appeal: { can: false, until: null, state: "upheld" } }),
        t,
        "tr",
      ),
    ).toBe(`Hesap 15${NBSP}Ocak'ta yeniden açılır. İtirazını inceledik, karar değişmedi.`);
    expect(stopHint(stop({ appeal: { can: false, until: JAN15, state: "none" } }), t, "tr")).toBe(
      null,
    );
    expect(stopHint(step(), t, "tr")).toBe(null);
    expect(dayWords(JAN15 + 230 * 86_400, "tr-TR", "to")).toBe(`2${NBSP}Eylül'e`);
    expect(dayWords(JAN15 + 81 * 86_400, "tr-TR", "at")).toBe(`6${NBSP}Nisan'da`);
  });

  it("finds paused rows and says a refused save in the player's words", () => {
    const config = {
      commands: [
        { id: "c1", paused: { case: "mc_1", category: "advertising" } },
        { id: "c2", paused: null },
      ],
      rules: [{ id: "r1", paused: { case: "mc_1", category: "spam" } }],
    } as unknown as CloudAccount["config"];
    expect([...pausedRows({ config })]).toEqual([
      ["c1", "advertising"],
      ["r1", "spam"],
    ]);
    expect(refusedCategory("content:scam")).toBe("scam");
    expect(refusedCategory("variable")).toBe(null);
    expect(refusedText("rmt", t, "tr")).toBe(
      "Bu satır gerçek parayla ticaret içeriyor gibi görünüyor. Düzelt ya da sil.",
    );
  });

  it("writes the moderation notice under the account's name", () => {
    expect(moderationNoticeText({ step: "suspend", category: "scam" }, t)).toBe(
      "Durduruldu: Dolandırıcılık. Yanlışlık olduğunu düşünüyorsan itiraz et.",
    );
    expect(moderationNoticeText({ step: "overturned", category: "other" }, te)).toBe(
      "Your appeal was accepted. The decision was lifted.",
    );
    expect(moderationNoticeText({ step: "note" }, t)).toBe(null);
    const words = noticeWords(
      {
        id: "01JBXK4V7Q2M9R3T5W8Y0ZA1BC",
        event: "moderation",
        kind: "afk_alert",
        priority: "normal",
        at: 1_800_000_000,
        tag: "moderation:mc_1",
        quiet: false,
        data: {
          acct: "a_1",
          name: "Madenci07",
          step: "warn",
          category: "advertising",
          case: "mc_1",
        },
      },
      t,
      "tr",
    );
    expect(words).toEqual({
      title: "Madenci07",
      body: "Uyarı: Reklam. Bir daha olursa hesap durdurulur.",
      target: { page: true },
    });
  });
});
