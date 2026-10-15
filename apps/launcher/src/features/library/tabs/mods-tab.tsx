import { FilePlus, MagnifyingGlassPlus, Package } from "@phosphor-icons/react";
import { Button, Chip, cn, EmptyState, SearchInput } from "@pl/ui";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type InstanceDetail, type ModEntry } from "@/lib/api";
import { formatBytes } from "@/lib/format";

function ModToggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative h-[22px] w-[38px] shrink-0 cursor-pointer rounded-full ring-1 transition-colors duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
        checked ? "bg-gold-500 ring-gold-400/60" : "bg-obsidian-700 ring-white/10",
      )}
    >
      <span
        className={cn(
          "absolute top-[3px] left-[3px] size-4 rounded-full shadow transition-transform duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]",
          checked ? "translate-x-4 bg-gold-ink" : "bg-bone-100",
        )}
      />
    </button>
  );
}

function ModRow({ mod, onToggle }: { mod: ModEntry; onToggle: (enabled: boolean) => void }) {
  const { t } = useTranslation();
  return (
    <li
      className={cn(
        "flex items-center gap-3.5 rounded-[var(--radius-panel)] px-3 py-2.5 transition-colors hover:bg-white/5",
        !mod.enabled && "opacity-55",
      )}
    >
      <span className="chamfer flex size-10 shrink-0 items-center justify-center bg-obsidian-800 [--cut:6px]">
        {mod.iconUrl ? (
          <img
            src={mod.iconUrl}
            alt=""
            className="size-full object-cover [image-rendering:pixelated]"
          />
        ) : (
          <Package size={20} weight="duotone" className="text-gold-400/80" />
        )}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-[14px] font-semibold text-bone-50">{mod.name}</span>
          <span className="shrink-0 text-[12px] text-ash-500 tabular-nums">{mod.version}</span>
          {mod.fromPack ? <Chip className="shrink-0">{t("instance.mods.fromPack")}</Chip> : null}
        </div>
        <p className="truncate text-[12.5px] text-ash-400">{mod.description}</p>
      </div>
      <span className="hidden w-20 shrink-0 text-right text-[12px] text-ash-500 tabular-nums lg:block">
        {formatBytes(mod.sizeBytes)}
      </span>
      <ModToggle
        checked={mod.enabled}
        onChange={onToggle}
        label={t("instance.mods.toggle", { name: mod.name })}
      />
    </li>
  );
}

export function ModsTab({ instance }: { instance: InstanceDetail }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [query, setQuery] = useState("");

  const toggle = useMutation({
    mutationFn: ({ mod, enabled }: { mod: ModEntry; enabled: boolean }) =>
      api.setModEnabled(instance.id, mod.fileName, enabled),
    onMutate: async ({ mod, enabled }) => {
      queryClient.setQueryData<InstanceDetail>(["instance", instance.id], (prev) =>
        prev
          ? {
              ...prev,
              mods: prev.mods.map((m) => (m.fileName === mod.fileName ? { ...m, enabled } : m)),
            }
          : prev,
      );
    },
  });

  const mods = useMemo(() => {
    const q = query.trim().toLocaleLowerCase("tr");
    return instance.mods
      .filter((m) => !q || m.name.toLocaleLowerCase("tr").includes(q))
      .sort((a, b) => a.name.localeCompare(b.name, "tr"));
  }, [instance.mods, query]);

  if (instance.loader === "vanilla") {
    return <p className="max-w-[64ch] text-ash-300">{t("instance.mods.vanilla")}</p>;
  }

  return (
    <div className="max-w-[980px]">
      <div className="mb-4 flex items-center gap-2.5">
        <SearchInput
          value={query}
          onChange={setQuery}
          placeholder={t("instance.mods.search")}
          label={t("instance.mods.search")}
          className="w-72"
        />
        <span className="flex-1" />
        <Button variant="ghost" icon={<FilePlus size={17} />}>
          {t("instance.mods.addFile")}
        </Button>
        <Button
          variant="secondary"
          icon={<MagnifyingGlassPlus size={17} className="text-gold-300" />}
        >
          {t("instance.mods.browse")}
        </Button>
      </div>
      {mods.length === 0 ? (
        <EmptyState title={t("instance.mods.emptyTitle")} body={t("instance.mods.emptyBody")} />
      ) : (
        <ul className="space-y-0.5">
          {mods.map((mod) => (
            <ModRow
              key={mod.fileName}
              mod={mod}
              onToggle={(enabled) => toggle.mutate({ mod, enabled })}
            />
          ))}
        </ul>
      )}
    </div>
  );
}
