/**
 * When a countdown's words change, in ms, `ms` before its moment; null once it has come: once a
 * minute ("2 sa 14 dk kaldı"), and each second only in the last minute ("45 sn"). A ticking seconds
 * cell said nothing the minutes did not, and every change on screen keeps WebView2 drawing for
 * about a second: an idle Home woke every second for it (OPT2, HOME-TASTE).
 */
export function countdownTickMs(ms: number): number | null {
  if (ms <= 0) return null;
  const unit = ms >= 60_000 ? 60_000 : 1000;
  // The words round down, so they change 1 ms past the boundary below.
  return (ms % unit) + 1;
}
