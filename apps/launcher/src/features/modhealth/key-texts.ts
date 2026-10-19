// Words for the key clash of the official pack's mods (KEYS2): key names, what the bindings do,
// the finding's line and the toast's. Kept free of React so they can be tested.
import type { TFunction } from "i18next";
import type { KeyChange } from "@/lib/api";
import { listFormat } from "@/lib/format";

const UNBOUND = "key.keyboard.unknown";

/** "V", "V ve G", "V, G ve H" / "V, G and H" (the interface's language), each once. */
function joinNames(names: string[]): string {
  const unique = names.filter((name, i) => names.indexOf(name) === i);
  try {
    return listFormat().format(unique);
  } catch {
    return unique.join(", ");
  }
}

/** The bindings the pack's table moves (crates/pl-mc/data/key-bindings.json) → `keyClash.actions`. */
const ACTIONS: Record<string, string> = {
  "key.wi_zoom.zoom": "zoom",
  "key.gammautils.gammaToggle": "gamma",
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

/** The finding's line: "Mod tuşları çakışıyor: V, G, H ve R." */
export function keyClashText(t: TFunction, changes: readonly KeyChange[]): string {
  return t("keyClash.issue", { keys: joinNames(changes.map((c) => keyLabel(t, c.key))) });
}

/**
 * The fix toast's second line, for the bindings that got a key: "Yeni tuşlar: Yakınlaştır C ve
 * Tam parlaklık X." None when every change only took a key away.
 */
export function newKeysLine(t: TFunction, changes: readonly KeyChange[]): string | undefined {
  const moved = changes.filter((c) => c.to);
  if (moved.length === 0) return undefined;
  return t("keyClash.newKeys", {
    keys: joinNames(moved.map((c) => `${actionName(t, c)} ${keyLabel(t, c.to)}`)),
  });
}

/** What a screen reader says for one row, before and after "Düzelt". */
export function changeSpoken(t: TFunction, change: KeyChange, fixed: boolean): string {
  const from = keyLabel(t, change.key);
  const to = keyLabel(t, change.to);
  if (fixed) return change.to ? t("keyClash.sr.changed", { to }) : t("keyClash.sr.changedOff");
  return change.to ? t("keyClash.sr.change", { from, to }) : t("keyClash.sr.changeOff", { from });
}
