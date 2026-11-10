// Crew plan templates (CREW-PLUS, a player's suggestion): the plan form's quick fills. Three
// built-in ones and up to five of the player's own, each a day relative to the day it is used
// ("bugün", "cumartesi"; never a date), a time of day and a note. Pressing one fills the form's day,
// hour, minute and note; the player may still change anything before "Planla". The rules are pure
// here (plan-templates.test.ts); the player's own live in a small store kept with the launcher's
// preferences and its settings backup, never in the plan API. The game's overlay shows the same
// chips: its built-in three in its own words (`PlanTemplates.java`), the player's own over the
// bridge (`OverlayConfig.planTemplates`).
import type { TFunction } from "i18next";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { clockOf, type PlanDay, planTimes, planWhen, timeOn } from "./crew-plans";
import { cleanPlanNote, PLAN_NOTE_MAX } from "./plan-note";

/** The days a template may name: today, tomorrow, or the next of a weekday. */
export const TEMPLATE_DAYS = [
  "today",
  "tomorrow",
  "mon",
  "tue",
  "wed",
  "thu",
  "fri",
  "sat",
  "sun",
] as const;
export type TemplateDay = (typeof TEMPLATE_DAYS)[number];

/** Weekdays in `Date.getDay()` order (Sunday first). */
const WEEKDAYS: readonly TemplateDay[] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/** The player keeps at most this many of their own. */
export const MAX_TEMPLATES = 5;
/** A template's name on its chip at most (characters). */
export const TEMPLATE_NAME_MAX = 24;

export interface PlanTemplate {
  /** Its name on the chip. */
  name: string;
  day: TemplateDay;
  /** Minutes after midnight, a whole quarter of an hour. */
  time: number;
  /** The plan's note ("" none). */
  note: string;
}

/** One of the player's own, as this computer keeps it (`id` for the list only). */
export interface OwnTemplate extends PlanTemplate {
  id: string;
}

/**
 * The built-in three, for real evenings of school-age players: tonight after homework, Friday's
 * long night, Saturday afternoon. The overlay keeps the same three (`PlanTemplates.BUILT_IN`).
 */
export const BUILT_IN = [
  { key: "tonight", day: "today", time: 21 * 60 },
  { key: "friday", day: "fri", time: 21 * 60 },
  { key: "weekend", day: "sat", time: 15 * 60 },
] as const satisfies ReadonlyArray<{ key: string; day: TemplateDay; time: number }>;

/** The built-in templates in the player's language. */
export function builtInTemplates(t: TFunction): Array<PlanTemplate & { key: string }> {
  return BUILT_IN.map(({ key, day, time }) => ({
    key,
    name: t(`crewPlus.templates.builtIn.${key}.name`),
    day,
    time,
    note: t(`crewPlus.templates.builtIn.${key}.note`),
  }));
}

/** The form's days a template's day names (today, tomorrow or each such weekday). */
function daysOf(day: TemplateDay, days: readonly PlanDay[]): PlanDay[] {
  if (day === "today") return days.filter((one) => one.offset === 0);
  if (day === "tomorrow") return days.filter((one) => one.offset === 1);
  const weekday = WEEKDAYS.indexOf(day);
  return days.filter((one) => new Date(one.start).getDay() === weekday);
}

/** That day's time (Unix seconds) when the form still offers it exactly, else null. */
function exactly(day: PlanDay, hour: number, minute: number, nowMs: number): number | null {
  const wanted = new Date(day.start);
  wanted.setHours(hour, minute, 0, 0);
  const at = wanted.getTime() / 1000;
  return planTimes(day, nowMs).includes(at) ? at : null;
}

/**
 * Where a template puts the form (Unix seconds): its time on the first day it names that still
 * offers it; else that day's closest time (tonight after 21:00, the next quarter); "today" late at
 * night, when today offers nothing more, means the form's first day. Null only when nothing fits.
 */
export function templateTime(
  template: Pick<PlanTemplate, "day" | "time">,
  days: readonly PlanDay[],
  nowMs: number,
): number | null {
  const hour = Math.floor(template.time / 60);
  const minute = template.time % 60;
  const named = daysOf(template.day, days);
  for (const day of named) {
    const at = exactly(day, hour, minute, nowMs);
    if (at !== null) return at;
  }
  const first = named[0] ?? (template.day === "today" ? days[0] : undefined);
  return first ? timeOn(first, hour, minute, nowMs) : null;
}

/** The day a form's day chip stands for in a template: "Bugün", "Yarın", else its weekday. */
export function templateDayOf(day: PlanDay): TemplateDay {
  if (day.offset === 0) return "today";
  if (day.offset === 1) return "tomorrow";
  return WEEKDAYS[new Date(day.start).getDay()] ?? "today";
}

/** A name of at most `TEMPLATE_NAME_MAX` characters, on one line ("" when nothing is left). */
export function cleanTemplateName(value: string): string {
  const name = value
    .replace(/[\p{Cc}\p{Cf}\p{Zl}\p{Zp}]/gu, " ")
    .replace(/\s+/g, " ")
    .trim();
  return [...name].slice(0, TEMPLATE_NAME_MAX).join("").trim();
}

/**
 * The form as a template: its day as the chip says it, its time, its note; named after the note
 * (what the plan is for), else after its day and time ("Cumartesi 15:00").
 */
export function templateFrom(
  at: number,
  day: PlanDay,
  note: string,
  nowMs: number,
  t: TFunction,
): PlanTemplate {
  const { hour, minute } = clockOf(at);
  const cleanNote = cleanPlanNote(note);
  const name = cleanTemplateName(cleanNote) || cleanTemplateName(planWhen(at, nowMs, t));
  return { name, day: templateDayOf(day), time: hour * 60 + minute, note: cleanNote };
}

/** The same fill: day, time and note alike (the name aside). */
export function sameFill(a: PlanTemplate, b: PlanTemplate): boolean {
  return a.day === b.day && a.time === b.time && a.note === b.note;
}

/** Text the settings backup refuses as a secret (a token's shape): never kept in a template. */
const TOKEN_LIKE = /^(eyJ|pls_)/;

function cleanTemplate(value: unknown): PlanTemplate | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const raw = value as Record<string, unknown>;
  if (typeof raw.name !== "string" || typeof raw.day !== "string") return null;
  const name = cleanTemplateName(raw.name);
  const note = typeof raw.note === "string" ? cleanPlanNote(raw.note) : "";
  const time = raw.time;
  const fits =
    name.length > 0 &&
    (TEMPLATE_DAYS as readonly string[]).includes(raw.day) &&
    typeof time === "number" &&
    Number.isInteger(time) &&
    time >= 0 &&
    time < 24 * 60 &&
    time % 15 === 0 &&
    [...note].length <= PLAN_NOTE_MAX &&
    !TOKEN_LIKE.test(name) &&
    !TOKEN_LIKE.test(note);
  return fits ? { name, day: raw.day as TemplateDay, time: time as number, note } : null;
}

/** Templates as a backup or an old store may hold them: the valid ones, at most five. */
export function cleanTemplates(value: unknown): PlanTemplate[] {
  if (!Array.isArray(value)) return [];
  return value
    .map(cleanTemplate)
    .filter((one): one is PlanTemplate => one !== null)
    .slice(0, MAX_TEMPLATES);
}

/** The player's own without this computer's ids (the backup's and the overlay's shape). */
export function plainTemplates(own: readonly OwnTemplate[]): PlanTemplate[] {
  return own.map(({ name, day, time, note }) => ({ name, day, time, note }));
}

let made = 0;
const newId = () => `t${Date.now().toString(36)}${(made++).toString(36)}`;

export type AddOutcome = "added" | "full" | "exists";

interface TemplateStore {
  own: OwnTemplate[];
  add: (template: PlanTemplate) => AddOutcome;
  rename: (id: string, name: string) => void;
  /** Takes one out; returns it and where it was (for "Geri al"). */
  remove: (id: string) => { template: OwnTemplate; index: number } | null;
  /** Puts a removed one back where it was (while there is room). */
  restore: (template: OwnTemplate, index: number) => void;
  /** A backup's list replaces this computer's. */
  replace: (templates: readonly PlanTemplate[]) => void;
}

export const usePlanTemplates = create<TemplateStore>()(
  persist(
    (set, get) => ({
      own: [],
      add: (template) => {
        const own = get().own;
        if (own.some((one) => sameFill(one, template))) return "exists";
        if (own.length >= MAX_TEMPLATES) return "full";
        set({ own: [...own, { ...template, id: newId() }] });
        return "added";
      },
      rename: (id, name) => {
        const clean = cleanTemplateName(name);
        if (!clean) return;
        set((s) => ({ own: s.own.map((one) => (one.id === id ? { ...one, name: clean } : one)) }));
      },
      remove: (id) => {
        const own = get().own;
        const index = own.findIndex((one) => one.id === id);
        const template = own[index];
        if (!template) return null;
        set({ own: own.filter((one) => one.id !== id) });
        return { template, index };
      },
      restore: (template, index) =>
        set((s) => {
          if (s.own.length >= MAX_TEMPLATES || s.own.some((one) => one.id === template.id)) {
            return s;
          }
          const own = [...s.own];
          own.splice(Math.min(index, own.length), 0, template);
          return { own };
        }),
      replace: (templates) =>
        set({ own: cleanTemplates(templates).map((one) => ({ ...one, id: newId() })) }),
    }),
    {
      name: "pl.planTemplates",
      version: 1,
      // What this computer kept is checked as a backup is (a hand-edited store never breaks it).
      merge: (persisted, current) => {
        const kept = (persisted as { own?: unknown } | undefined)?.own;
        const own = (Array.isArray(kept) ? kept : [])
          .map((raw): OwnTemplate | null => {
            const clean = cleanTemplate(raw);
            if (!clean) return null;
            const id = (raw as { id?: unknown }).id;
            return { ...clean, id: typeof id === "string" && id ? id : newId() };
          })
          .filter((one): one is OwnTemplate => one !== null)
          .slice(0, MAX_TEMPLATES);
        return { ...current, own };
      },
    },
  ),
);
