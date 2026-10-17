// The details of the mod center's "Sorunlar" strip, kept free of React so they can be tested.
import type { TFunction } from "i18next";
import type { ModIssue } from "@/lib/api";

const folders = new Set(["mods", "config", "resourcepacks", "shaderpacks"]);

/**
 * One line per problem file, never a file name: a recommended mod's file by its title ("Sodium
 * okunamadı."), any other file counted with its kind of file and problem ("Bir mod ayarı dosyası
 * okunamadı.", "3 kaynak paketi başka bir programda açık.").
 */
export function issueLines(t: TFunction, issues: ModIssue[]): string[] {
  const lines: string[] = [];
  const counted = new Map<string, { kind: ModIssue["kind"]; folder: string; count: number }>();
  for (const issue of issues) {
    if (issue.title) {
      lines.push(t(`modCenter.issues.kinds.${issue.kind}`, { name: issue.title }));
      continue;
    }
    const top = issue.folder.split(/[\\/]/)[0] ?? "";
    const folder = folders.has(top) ? top : "other";
    const key = `${issue.kind}|${folder}`;
    const group = counted.get(key) ?? { kind: issue.kind, folder, count: 0 };
    group.count += 1;
    counted.set(key, group);
  }
  for (const group of counted.values()) {
    const what = t(`modCenter.issues.folders.${group.folder}`);
    const name =
      group.count === 1
        ? t("modCenter.issues.one", { what })
        : t("modCenter.issues.many", { count: group.count, what });
    lines.push(t(`modCenter.issues.kinds.${group.kind}`, { name }));
  }
  return [...new Set(lines)];
}
