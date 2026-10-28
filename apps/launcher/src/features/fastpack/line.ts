import type { FastPacksView } from "@/bindings";

/** The second sentence under "Hızlı doku paketi yükleme": only where the switch does nothing. */
export type FastPackLine = "none" | "missing" | "notHere";

/**
 * What the row adds for the install Play opens: an install the launcher does not manage gets
 * nothing ("Bu kurulumda yok."), the launcher's pack on a version Studio lists no mod for gets
 * nothing either ("Bu sürümde yok."). A view that could not be read claims nothing.
 */
export function fastPackLine(view: FastPacksView | undefined): FastPackLine {
  if (!view?.shown) return "none";
  if (!view.official) return "notHere";
  return view.available ? "none" : "missing";
}
