import type { TFunction } from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import { type PlanDay, planDays } from "./crew-plans";
import { buildBackup, readBackup } from "./model";
import {
  BUILT_IN,
  builtInTemplates,
  cleanTemplateName,
  cleanTemplates,
  MAX_TEMPLATES,
  type PlanTemplate,
  plainTemplates,
  sameFill,
  templateDayOf,
  templateFrom,
  templateTime,
  usePlanTemplates,
} from "./plan-templates";

/** Local wall time in October 2026 (1 October was a Thursday). */
const local = (day: number, hour: number, minute = 0) =>
  new Date(2026, 9, day, hour, minute).getTime();
const sec = (ms: number) => ms / 1000;
const t = ((key: string, values?: Record<string, unknown>) =>
  values ? `${key} ${JSON.stringify(values)}` : key) as unknown as TFunction;

const template = (day: PlanTemplate["day"], time: number, note = ""): PlanTemplate => ({
  name: "Şablon",
  day,
  time,
  note,
});
/** Where a template puts the form opened at `now`. */
const fills = (one: PlanTemplate, now: number) => templateTime(one, planDays(now), now);

describe("plan templates (CREW-PLUS)", () => {
  it("knows the weekdays the tests use", () => {
    expect(new Date(local(6, 12)).getDay()).toBe(2); // Tuesday
    expect(new Date(local(9, 12)).getDay()).toBe(5); // Friday
    expect(new Date(local(10, 12)).getDay()).toBe(6); // Saturday
  });

  it("puts each built-in one on its own day and time", () => {
    const tuesday = local(6, 18);
    const [tonight, friday, weekend] = BUILT_IN.map((one) =>
      fills(template(one.day, one.time), tuesday),
    );
    expect(tonight).toBe(sec(local(6, 21)));
    expect(friday).toBe(sec(local(9, 21)));
    expect(weekend).toBe(sec(local(10, 15)));
    expect(builtInTemplates(t).map((one) => one.name)).toEqual([
      "crewPlus.templates.builtIn.tonight.name",
      "crewPlus.templates.builtIn.friday.name",
      "crewPlus.templates.builtIn.weekend.name",
    ]);
  });

  it("takes the closest time left on its day once its own time is gone", () => {
    // Tonight after 21:00: the next quarter, still tonight.
    expect(fills(template("today", 21 * 60), local(6, 22, 10))).toBe(sec(local(6, 22, 15)));
    // Friday night, on Friday at 22:10.
    expect(fills(template("fri", 21 * 60), local(9, 22, 10))).toBe(sec(local(9, 22, 15)));
    // A weekday's time still ahead today: today, not next week.
    expect(fills(template("sat", 15 * 60), local(10, 9))).toBe(sec(local(10, 15)));
  });

  it("means the form's first day when today offers nothing more", () => {
    expect(fills(template("today", 21 * 60), local(6, 23, 56))).toBe(sec(local(7, 21)));
    expect(fills(template("tomorrow", 20 * 60 + 30), local(6, 12))).toBe(sec(local(7, 20, 30)));
  });

  it("keeps the form's day as the chip says it", () => {
    const now = local(6, 12);
    const days = planDays(now);
    const dayAt = (offset: number) => days.find((one) => one.offset === offset) as PlanDay;
    expect(templateDayOf(dayAt(0))).toBe("today");
    expect(templateDayOf(dayAt(1))).toBe("tomorrow");
    expect(templateDayOf(dayAt(2))).toBe("thu");
    expect(templateDayOf(dayAt(4))).toBe("sat");
    const kept = templateFrom(sec(local(10, 15, 30)), dayAt(4), "  Ada   turu ", now, t);
    expect(kept).toEqual({ name: "Ada turu", day: "sat", time: 15 * 60 + 30, note: "Ada turu" });
    // Without a note it is named after its day and time.
    const plain = templateFrom(sec(local(6, 21)), dayAt(0), "", now, t);
    expect(plain.day).toBe("today");
    expect(plain.time).toBe(21 * 60);
    expect(plain.note).toBe("");
    expect(plain.name.length).toBeGreaterThan(0);
    expect([...plain.name].length).toBeLessThanOrEqual(24);
  });

  it("cleans names and keeps only valid templates, five at most", () => {
    expect(cleanTemplateName("  Gece\n\tOyunu  ")).toBe("Gece Oyunu");
    expect([...cleanTemplateName("x".repeat(40))].length).toBe(24);
    const valid = { name: "Gece", day: "fri", time: 1290, note: "Sesli" };
    expect(
      cleanTemplates([
        valid,
        { ...valid, day: "someday" },
        { ...valid, time: 1291 },
        { ...valid, time: 24 * 60 },
        { ...valid, name: "   " },
        { ...valid, note: "x".repeat(81) },
        { ...valid, name: "eyJhbGciOiJIUzI1NiJ9" },
        { ...valid, note: "pls_abcdefghijklmnopqrstuvwxyz" },
        "Gece",
        null,
      ]),
    ).toEqual([valid]);
    expect(cleanTemplates("x")).toEqual([]);
    expect(cleanTemplates(Array.from({ length: 8 }, () => valid))).toHaveLength(MAX_TEMPLATES);
  });

  it("travels with the settings backup, never more than the rules allow", () => {
    const own = [template("sat", 15 * 60, "Uzun oyun"), template("today", 21 * 60)];
    const document = buildBackup(
      {},
      {
        favorites: [],
        notifyFavoriteOnline: true,
        notifyFavoriteProjects: true,
        planTemplates: [...own, { name: "", day: "today", time: 5, note: "" }],
      },
    );
    expect(document.friends.planTemplates).toEqual(own);
    expect(readBackup(document).friends.planTemplates).toEqual(own);
    // None: the key stays out (an older backup's shape).
    const empty = buildBackup(
      {},
      {
        favorites: [],
        notifyFavoriteOnline: true,
        notifyFavoriteProjects: true,
        planTemplates: [],
      },
    );
    expect("planTemplates" in empty.friends).toBe(false);
    expect(
      readBackup({ friends: { planTemplates: [{ day: "x" }] } }).friends.planTemplates,
    ).toEqual([]);
  });
});

describe("the player's own templates", () => {
  beforeEach(() => usePlanTemplates.setState({ own: [] }));

  it("adds up to five, never the same fill twice", () => {
    const store = usePlanTemplates.getState();
    expect(store.add(template("fri", 21 * 60, "Bedwars"))).toBe("added");
    expect(store.add({ ...template("fri", 21 * 60, "Bedwars"), name: "Başka ad" })).toBe("exists");
    for (let i = 1; i < MAX_TEMPLATES; i += 1) {
      expect(store.add(template("sat", i * 60))).toBe("added");
    }
    expect(store.add(template("sun", 12 * 60))).toBe("full");
    expect(usePlanTemplates.getState().own).toHaveLength(MAX_TEMPLATES);
    expect(sameFill(template("fri", 21 * 60, "Bedwars"), template("fri", 21 * 60, "Bedwars"))).toBe(
      true,
    );
  });

  it("renames, deletes and puts a deleted one back where it was", () => {
    const store = usePlanTemplates.getState();
    store.add(template("fri", 21 * 60, "Bir"));
    store.add(template("sat", 15 * 60, "İki"));
    store.add(template("sun", 14 * 60, "Üç"));
    const [first, second] = usePlanTemplates.getState().own;
    if (!first || !second) throw new Error("no templates");
    store.rename(first.id, "  Cuma   gecesi ");
    store.rename(second.id, "   ");
    expect(usePlanTemplates.getState().own.map((one) => one.name)).toEqual([
      "Cuma gecesi",
      "Şablon",
      "Şablon",
    ]);
    const gone = store.remove(second.id);
    expect(gone?.index).toBe(1);
    expect(usePlanTemplates.getState().own).toHaveLength(2);
    if (gone) store.restore(gone.template, gone.index);
    expect(usePlanTemplates.getState().own.map((one) => one.id)[1]).toBe(second.id);
    expect(plainTemplates(usePlanTemplates.getState().own)[0]).toEqual({
      name: "Cuma gecesi",
      day: "fri",
      time: 21 * 60,
      note: "Bir",
    });
  });

  it("takes a backup's list in place of this computer's", () => {
    usePlanTemplates.getState().add(template("fri", 21 * 60));
    usePlanTemplates.getState().replace([template("sun", 18 * 60, "Pazar"), template("mon", 7)]);
    expect(plainTemplates(usePlanTemplates.getState().own)).toEqual([
      template("sun", 18 * 60, "Pazar"),
    ]);
  });
});
