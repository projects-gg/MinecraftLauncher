import { HardDrives, Plus } from "@phosphor-icons/react";
import { Button, cn } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { SavedServer } from "@/lib/api";
import { formatCount } from "@/lib/format";

export function ServersTab({ servers }: { servers: SavedServer[] }) {
  const { t } = useTranslation();
  return (
    <div className="max-w-[980px]">
      <div className="mb-4 flex justify-end">
        <Button variant="secondary" icon={<Plus size={16} weight="bold" className="text-gold-300" />}>
          {t("instance.servers.add")}
        </Button>
      </div>
      <ul className="space-y-1">
        {servers.map((s) => (
          <li key={s.address} className="flex items-center gap-4 rounded-[var(--radius-panel)] px-3 py-3 hover:bg-white/5">
            <span className="chamfer flex size-11 shrink-0 items-center justify-center bg-obsidian-800 [--cut:6px]">
              <HardDrives size={20} weight="duotone" className="text-gold-400/80" />
            </span>
            <div className="min-w-0 flex-1">
              <p className="truncate text-[14px] font-semibold text-bone-50">{s.name}</p>
              <p className="truncate text-[12.5px] text-ash-400" data-selectable>
                {s.address}
              </p>
            </div>
            <span className={cn("flex items-center gap-2 text-[12.5px] tabular-nums", s.online ? "text-ash-300" : "text-ash-500")}>
              <span className={cn("size-2 rounded-full", s.online ? "bg-jade-400" : "bg-ash-500")} />
              {s.online && s.players !== undefined && s.maxPlayers !== undefined
                ? t("instance.servers.players", { players: formatCount(s.players), max: formatCount(s.maxPlayers) })
                : t("instance.servers.offline")}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
