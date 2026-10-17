/**
 * The last part of a file path, Windows or POSIX (`C:\…\screenshots\2026-10-17_14.20.11.png` →
 * `2026-10-17_14.20.11.png`). A trailing separator is ignored.
 */
export function fileNameOf(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? "";
}
