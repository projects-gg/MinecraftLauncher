// Words for the key clash of the official pack's mods (KEYS2): key names, what the bindings do,
// the finding's line and the toast's. Kept free of React so they can be tested.
import type { TFunction } from "i18next";
import type { KeyChange } from "@/lib/api";
import { listFormat } from "@/lib/format";

const UNBOUND = "key.keyboard.unknown";

/** No key: the change gives the binding one (an unbound push to talk, KEYS3), or takes it away. */
export function isUnbound(key: string | undefined): boolean {
  return !key || key === UNBOUND;
}

/** "V", "V ve G", "V, G ve H" / "V, G and H" (the interface's language), each once. */
function joinNames(names: string[]): string {
  const unique = names.filter((name, i) => names.indexOf(name) === i);
  try {
    return listFormat().format(unique);
  } catch {
    return unique.join(", ");
  }
}

/** The bindings the pack's table changes (crates/pl-mc/data/key-bindings.json) → `keyClash.actions`. */
const ACTIONS: Record<string, string> = {
  "key.wi_zoom.zoom": "zoom",
  "key.gammautils.gammaToggle": "gamma",
  "key.push_to_talk": "pushToTalk",
  "key.hide_icons": "hideIcons",
  "iris.keybind.reload": "reloadShaders",
};

/**
 * A key as the keyboard shows it: `key.keyboard.v` → "V", `key.keyboard.caps.lock` → "Caps Lock",
 * `key.mouse.4` → "Fare 4"; no key → "Atanmamış" (the game's word).
 */
export function keyLabel(t: TFunction, key: string | undefined): string {
  if (!key || key === UNBOUND) return t("keyClash.unbound");
  const mouse = /^key\.mouse\.(.+)$/.exec(key);
  if (mouse) return t("keyClash.keyNames.mouse", { button: mouse[1] });
  const name = key.replace(/^key\.keyboard\./, "");
  if (name === "caps.lock") return t("keyClash.keyNames.capsLock");
  if (name.length === 1) return name.toUpperCase();
  return name
    .split(".")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

/** What a binding does, in the interface's language (the table's English for any other). */
export function actionName(t: TFunction, change: KeyChange): string {
  const key = ACTIONS[change.name];
  return key ? t(`keyClash.actions.${key}`) : change.action;
}

/**
 * The finding's line: "Mod tuşları çakışıyor: V, G, H ve R." Only the clashes: a push to talk
 * the fix binds along shares no key.
 */
export function keyClashText(t: TFunction, changes: readonly KeyChange[]): string {
  const clashes = changes.filter((c) => c.partner);
  return t("keyClash.issue", { keys: joinNames(clashes.map((c) => keyLabel(t, c.key))) });
}

/**
 * The fix toast's line for the clashing bindings that got a key: "Yeni tuşlar: Yakınlaştır C ve
 * Tam parlaklık X." None when every clash only took a key away.
 */
export function newKeysLine(t: TFunction, changes: readonly KeyChange[]): string | undefined {
  const moved = changes.filter((c) => c.partner && c.to);
  if (moved.length === 0) return undefined;
  return t("keyClash.newKeys", {
    keys: joinNames(moved.map((c) => `${actionName(t, c)} ${keyLabel(t, c.to)}`)),
  });
}

/**
 * The fix toast's lines under its title: the clashing bindings' new keys, then a line for each
 * unbound binding it gave a key along (KEYS3: "Bas-konuş: Caps Lock.").
 */
export function fixedLines(t: TFunction, changes: readonly KeyChange[]): string[] {
  const lines: string[] = [];
  const moved = newKeysLine(t, changes);
  if (moved) lines.push(moved);
  for (const change of changes) {
    if (change.partner || !change.to) continue;
    lines.push(t("keyClash.bound", { action: actionName(t, change), key: keyLabel(t, change.to) }));
  }
  return lines;
}

/** What a screen reader says for one row, before and after "Düzelt". */
export function changeSpoken(t: TFunction, change: KeyChange, fixed: boolean): string {
  const from = keyLabel(t, change.key);
  const to = keyLabel(t, change.to);
  if (fixed) return change.to ? t("keyClash.sr.changed", { to }) : t("keyClash.sr.changedOff");
  if (isUnbound(change.key) && change.to) return t("keyClash.sr.bind", { to });
  return change.to ? t("keyClash.sr.change", { from, to }) : t("keyClash.sr.changeOff", { from });
}
