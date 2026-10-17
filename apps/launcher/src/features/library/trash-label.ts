// What the trash lists say about an item instead of its file name.
import type { TrashKind } from "@/lib/api";
import i18n from "@/lib/i18n";
import { formatWhen } from "./when";

/** The game's screenshot names: `2026-10-14_19.22.11.png`, `_2` when two share a second. */
const SCREENSHOT = /^(\d{4})-(\d{2})-(\d{2})_(\d{2})\.(\d{2})\.(\d{2})(?:_\d+)?\.png$/i;
/** Our (and the game's) backup names: `2026-10-15_22-44-12_My World.zip`. */
const BACKUP = /^(\d{4})-(\d{2})-(\d{2})_(\d{2})-(\d{2})-(\d{2})_(.+)\.zip$/i;

function localDate(parts: string[]): Date | undefined {
  const [y, mo, d, h, mi, s] = parts.map(Number) as [
    number,
    number,
    number,
    number,
    number,
    number,
  ];
  const date = new Date(y, mo - 1, d, h, mi, s);
  return Number.isNaN(date.getTime()) || date.getMonth() !== mo - 1 ? undefined : date;
}

/** When a screenshot was taken, read from its file name. */
export function screenshotTakenAt(fileName: string): Date | undefined {
  const match = SCREENSHOT.exec(fileName);
  return match ? localDate(match.slice(1, 7)) : undefined;
}

/**
 * When a screenshot was taken: the time in its name (the game writes it), else the file's last
 * write. The name survives copies and a moved data folder; the write time may not.
 */
export function shotTime(shot: { fileName: string; takenAt?: string }): string | undefined {
  return screenshotTakenAt(shot.fileName)?.toISOString() ?? shot.takenAt;
}

/** The world a backup holds and when it was made, read from its file name. */
export function backupParts(fileName: string): { world?: string; createdAt?: Date } {
  const match = BACKUP.exec(fileName);
  if (match) return { world: match[7], createdAt: localDate(match.slice(1, 7)) };
  const plain = fileName.replace(/\.zip$/i, "").trim();
  return plain ? { world: plain } : {};
}

/** A trash entry as the lists show it (the kind, its name and its file name). */
export interface TrashLabelInput {
  kind: TrashKind;
  name: string;
  fileName: string;
}

/**
 * "Yeni Dünya", "Yeni Dünya yedeği", "Ekran görüntüsü, Bugün 19:22", "Eski modlar (26.1.2)":
 * never a file name.
 */
export function trashLabel(item: TrashLabelInput, now = new Date()): string {
  const t = i18n.t.bind(i18n);
  switch (item.kind) {
    case "backup": {
      const { world } = backupParts(item.fileName || item.name);
      return world
        ? t("instance.trash.labels.backup", { world })
        : t("instance.trash.labels.backupUnknown");
    }
    case "screenshot": {
      const taken = screenshotTakenAt(item.fileName || item.name);
      return taken
        ? t("instance.trash.labels.screenshot", { when: formatWhen(taken.toISOString(), now) })
        : t("instance.trash.labels.screenshotUnknown");
    }
    case "modSet": {
      // "26.1.2 mods" / "26.1.2 config" (the set's version and the folder set aside).
      const [version = "", folder = ""] = item.name.split(" ");
      return folder === "config"
        ? t("instance.trash.labels.modSetConfig", { version })
        : t("instance.trash.labels.modSetMods", { version });
    }
    case "versionSet":
      // A parked set of a Minecraft version (its name is the version).
      return t("instance.trash.labels.versionSet", { version: item.name });
    default:
      return item.name;
  }
}
