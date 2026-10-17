import type { TFunction } from "i18next";
import { type DropPlan, dropHint } from "./model";
import type { ZoneState } from "./zones";

/** "Buraya bırak", or that these files cannot go here. */
export function zoneTitle(t: TFunction, state: ZoneState, count: number): string {
  return state === "refuse" ? t("drop.zone.refuse", { count }) : t("drop.zone.title");
}

/**
 * The window's line on what the drop will do (null while the files are still being read).
 * `here`: the zone sits on the target's own page and says "bu kuruluma" instead of its name.
 */
export function windowHint(
  t: TFunction,
  plan: DropPlan | null,
  targetName: string | undefined,
  here: boolean,
): string | null {
  if (!plan) return null;
  const hint = dropHint(plan, here);
  return t(hint.key, { count: hint.count ?? 1, name: targetName ?? "" });
}

/** The polite status line: "Buraya bırak: …", or the refusal and its reason. */
export function announcementOf(state: ZoneState, title: string, hint: string | null): string {
  if (!hint) return title;
  return state === "refuse" ? `${title}. ${hint}` : `${title}: ${hint}`;
}
