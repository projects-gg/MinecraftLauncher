const number1 = new Intl.NumberFormat("tr-TR", { maximumFractionDigits: 1 });
const integer = new Intl.NumberFormat("tr-TR");
const relative = new Intl.RelativeTimeFormat("tr", { numeric: "auto" });

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${integer.format(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${number1.format(value)} ${units[unit]}`;
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
