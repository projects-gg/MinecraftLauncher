// The overlay mod's client tweaks (DIALOG-FOCUS): small fixes to the game's own screens, each a
// launcher preference of its own name, on by default. Rust gets them as `ClientTweaks`
// (`OverlayConfig.tweaks`, a launch's settings) and passes the ones on to the game on their own
// launch property, so they run without the bridge too. Adding one: its field in Rust's
// `ClientTweaks`, its preference (stores/prefs.ts), its key here, its row (settings-row.tsx) and its
// words (locales/*-tweaks.ts).
import type { ClientTweaks } from "@/bindings";

/** Every tweak's preference, in Rust's order. */
export const TWEAK_KEYS = ["dialogFocus"] as const satisfies readonly (keyof ClientTweaks)[];
export type TweakKey = (typeof TWEAK_KEYS)[number];

/** The tweaks as the page keeps them, for Rust: one never saved is on, its default. */
export function clientTweaks(prefs: Partial<Record<TweakKey, boolean>>): Required<ClientTweaks> {
  return { dialogFocus: prefs.dialogFocus ?? true };
}
