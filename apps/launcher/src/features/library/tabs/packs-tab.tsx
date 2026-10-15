import { Images, Plus } from "@phosphor-icons/react";
import { Button, cn } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { PackEntry } from "@/lib/api";
import { formatBytes } from "@/lib/format";

export function PacksTab({ packs, emptyText }: { packs: PackEntry[]; emptyText: string }) {
  const { t } = useTranslation();
  return (
    <div className="max-w-[980px]">
      <div className="mb-4 flex justify-end">
        <Button variant="secondary" icon={<Plus size={16} weight="bold" className="text-gold-300" />}>
          {t("instance.packs.add")}
        </Button>
      </div>
      {packs.length === 0 ? (
        <p className="text-ash-400">{emptyText}</p>
      ) : (
        <ul className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-3">
          {packs.map((p) => (
            <li
              key={p.fileName}
              className={cn("glass flex items-center gap-3 rounded-[var(--radius-panel)] p-3", !p.enabled && "opacity-60")}
            >
              <span className="chamfer flex size-12 shrink-0 items-center justify-center bg-obsidian-800 [--cut:6px]">
                {p.iconUrl ? (
                  <img src={p.iconUrl} alt="" className="size-full object-cover [image-rendering:pixelated]" />
                ) : (
                  <Images size={22} weight="duotone" className="text-gold-400/80" />
                )}
              </span>
              <div className="min-w-0 flex-1">
                <p className="truncate text-[14px] font-semibold text-bone-50">{p.name}</p>
                <p className="truncate text-[12.5px] text-ash-400">{p.description}</p>
              </div>
              <span className="shrink-0 text-[12px] text-ash-500 tabular-nums">{formatBytes(p.sizeBytes)}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
