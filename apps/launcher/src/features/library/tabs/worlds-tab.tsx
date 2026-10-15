import { Archive, Globe } from "@phosphor-icons/react";
import { Button, Chip } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { WorldEntry } from "@/lib/api";
import { formatBytes, formatRelative } from "@/lib/format";

export function WorldsTab({ worlds }: { worlds: WorldEntry[] }) {
  const { t } = useTranslation();
  if (!worlds.length) return <p className="text-ash-400">{t("instance.worlds.empty")}</p>;
  return (
    <ul className="max-w-[980px] space-y-1">
      {worlds.map((w) => (
        <li key={w.folder} className="flex items-center gap-4 rounded-[var(--radius-panel)] px-3 py-3 hover:bg-white/5">
          <span className="chamfer flex size-12 shrink-0 items-center justify-center bg-obsidian-800 [--cut:6px]">
            {w.iconUrl ? (
              <img src={w.iconUrl} alt="" className="size-full object-cover [image-rendering:pixelated]" />
            ) : (
              <Globe size={22} weight="duotone" className="text-gold-400/80" />
            )}
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <span className="truncate text-[14px] font-semibold text-bone-50">{w.name}</span>
              <Chip tone={w.gameMode === "hardcore" ? "flare" : "neutral"}>{t(`instance.worlds.modes.${w.gameMode}`)}</Chip>
            </div>
            <p className="text-[12.5px] text-ash-400">
              {t("instance.worlds.lastPlayed", { when: formatRelative(w.lastPlayedAt) })}
            </p>
          </div>
          <span className="text-[12px] text-ash-500 tabular-nums">{formatBytes(w.sizeBytes)}</span>
          <Button size="sm" variant="ghost" icon={<Archive size={15} />}>
            {t("instance.worlds.backup")}
          </Button>
        </li>
      ))}
    </ul>
  );
}
