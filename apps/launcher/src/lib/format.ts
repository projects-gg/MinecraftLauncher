const number1 = new Intl.NumberFormat("tr-TR", { maximumFractionDigits: 1 });
const integer = new Intl.NumberFormat("tr-TR");
const relative = new Intl.RelativeTimeFormat("tr", { numeric: "auto" });

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/**
 * "512 B", "1,5 MB", "1 GB". Never four digits before the unit: from what would read as 1000 of
 * a unit (1000–1023 MB, or 999,96 MB rounding up) it switches to the next one.
 */
export function formatBytes(bytes: number): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  const shown = () => (unit === 0 ? Math.round(value) : Math.round(value * 10) / 10);
  while (shown() >= 1000 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${(unit === 0 ? integer : number1).format(shown())} ${BYTE_UNITS[unit]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  return `${formatBytes(bytesPerSec)}/sn`;
}

export function formatCount(n: number): string {
  return integer.format(n);
}

export function formatRelative(iso: string, now = Date.now()): string {
  const diff = new Date(iso).getTime() - now;
  const abs = Math.abs(diff);
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (abs < hour) return relative.format(Math.round(diff / minute), "minute");
  if (abs < day) return relative.format(Math.round(diff / hour), "hour");
  if (abs < 30 * day) return relative.format(Math.round(diff / day), "day");
  return new Date(iso).toLocaleDateString("tr-TR", { day: "numeric", month: "long" });
}

/** "2 sa 14 dk", "38 dk", "45 sn". */
export function formatDuration(totalSecs: number): string {
  const s = Math.max(0, Math.round(totalSecs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return m > 0 ? `${h} sa ${m} dk` : `${h} sa`;
  if (m > 0) return `${m} dk`;
  return `${s} sn`;
}
