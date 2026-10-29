import type { FastPacksView } from "@/bindings";

/** Where the switch does nothing for the install Play opens. */
export type FastPackLine = "none" | "missing" | "notHere";

/**
 * What the row knows about the install Play opens: an install the launcher does not manage gets
 * nothing ("notHere"), the launcher's pack on a version Studio lists no mod for gets nothing
 * either ("missing"). A view that could not be read claims nothing.
 */
export function fastPackLine(view: FastPacksView | undefined): FastPackLine {
  if (!view?.shown) return "none";
  if (!view.official) return "notHere";
  return view.available ? "none" : "missing";
}

/** The line under the title and the switch, for that install. */
export type FastPackRowState = {
  /** The line's i18n key: what it does, or in its place why it does nothing there. */
  help: "fastPack.help" | "fastPack.missing" | "fastPack.notHere";
  /** The Minecraft version "fastPack.missing" names. */
  version?: string;
  checked: boolean;
  disabled: boolean;
};

/**
 * Where it works, the line says what it does and the switch follows the player's preference.
 * Where it does nothing, the reason takes the line's place and names the thing (the Minecraft
 * version, the Projects pack), and the switch is held off and dimmed like the other held switches
 * (lead, 29 Oct, TASTE-SWEEP FASTPACK-L r1). The preference is kept, not changed, so it is back
 * on the installs where it works. A version the row does not know claims nothing.
 */
export function fastPackRow(
  on: boolean,
  line: FastPackLine,
  mcVersion: string | undefined,
): FastPackRowState {
  let help: FastPackRowState["help"] = "fastPack.help";
  if (line === "notHere") help = "fastPack.notHere";
  else if (line === "missing" && mcVersion) help = "fastPack.missing";
  const held = help !== "fastPack.help";
  return { help, version: mcVersion, checked: on && !held, disabled: held };
}
