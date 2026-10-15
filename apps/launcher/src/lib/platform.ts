/** True inside the Tauri webview; false in a plain browser (design previews, tests). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export type DesktopOs = "windows" | "macos" | "linux";

export function detectOs(): DesktopOs {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes("mac")) return "macos";
  if (ua.includes("linux") || ua.includes("x11")) return "linux";
  return "windows";
}
