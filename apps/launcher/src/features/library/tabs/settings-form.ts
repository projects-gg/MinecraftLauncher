// The small rules behind the instance settings form: the graphics control and the "İleri düzey"
// part. Pure: settings-form.test.ts checks them.
import type { GraphicsPreset, GraphicsState, InstanceSettings } from "@/lib/api";

/** What the graphics control shows: a preset, or "custom" ("Özel", the file's own values). */
export type GraphicsChoice = GraphicsPreset | "custom";

export const GRAPHICS_PRESETS: readonly GraphicsPreset[] = ["smooth", "balanced", "quality"];

/**
 * The choice the file stands for: its preset (or the one the first launch writes), "custom"
 * for values of its own, `null` for a version presets do not cover (no control then).
 */
export function savedChoice(state: GraphicsState): GraphicsChoice | null {
  switch (state.kind) {
    case "preset":
    case "firstLaunch":
      return state.preset;
    case "custom":
      return "custom";
    default:
      return null;
  }
}

/** The control's options: the presets, plus "Özel" while the file has values of its own. */
export function graphicsChoices(saved: GraphicsChoice | null): GraphicsChoice[] {
  return saved === "custom" ? [...GRAPHICS_PRESETS, "custom"] : [...GRAPHICS_PRESETS];
}

/**
 * The draft's pick once the player chooses `value`: nothing when the file already has it, or
 * for "Özel" (the player's own values stay).
 */
export function pickFor(
  saved: GraphicsChoice | null,
  value: GraphicsChoice,
): GraphicsPreset | undefined {
  return value === "custom" || value === saved ? undefined : value;
}

/** Saving `picked` replaces values the player set in the game: the one case worth a line. */
export function replacesCustom(
  saved: GraphicsChoice | null,
  picked: GraphicsPreset | undefined,
): boolean {
  return saved === "custom" && picked !== undefined;
}

type AdvancedFields = Pick<InstanceSettings, "jvmArgs" | "preLaunch" | "postExit" | "wrapper">;

/** "İleri düzey" starts open when something is set in it, so nothing set stays out of sight. */
export function hasAdvanced(settings: AdvancedFields): boolean {
  return [settings.jvmArgs, settings.preLaunch, settings.postExit, settings.wrapper].some(
    (value) => value.trim() !== "",
  );
}
