import { CaretUpDown, Check, FolderOpen, Plus } from "@phosphor-icons/react";
import { cn, PlayButton, type PlayState, PlayerHead, Progress, Switch, toast } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { DropdownMenu } from "radix-ui";
import { useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { api, type InstanceSummary } from "@/lib/api";
import { formatCount, formatDuration, formatRelative, formatSpeed } from "@/lib/format";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { InstanceIcon } from "./instance-icon";

function loaderLine(i: InstanceSummary, loaderName: string) {
  return i.loader === "vanilla" ? `Minecraft ${i.mcVersion}` : `${loaderName} ${i.mcVersion}`;
}

function InstancePicker({
  instances,
  selected,
  onSelect,
}: {
  instances: InstanceSummary[];
  selected: InstanceSummary | undefined;
  onSelect: (id: string) => void;
}) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button
          type="button"
          aria-label={t("instances.pickerLabel")}
          className="group flex h-[60px] w-[290px] shrink-0 cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-2.5 text-left transition-colors duration-150 hover:bg-white/6 data-[state=open]:bg-white/8"
        >
          {selected ? <InstanceIcon icon={selected.icon} size={44} /> : <span className="size-11" />}
          <span className="min-w-0 flex-1">
            <span className="block truncate font-display text-[16px] font-semibold text-bone-50">
              {selected?.name ?? t("common.loading")}
            </span>
            <span className="block truncate text-[12.5px] text-ash-400">
              {selected ? loaderLine(selected, t(`loaders.${selected.loader}`)) : ""}
            </span>
          </span>
          <CaretUpDown size={16} className="shrink-0 text-ash-400 transition-colors group-hover:text-bone-100" />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          side="top"
          align="start"
          sideOffset={10}
          className="glass-strong z-50 w-[320px] rounded-[var(--radius-panel)] p-1.5 shadow-[0_24px_60px_rgb(34_6_6/0.55)]"
        >
          {instances.map((i) => (
            <DropdownMenu.Item
              key={i.id}
              onSelect={() => onSelect(i.id)}
              className="flex cursor-pointer items-center gap-3 rounded-md px-2 py-2 outline-none data-[highlighted]:bg-white/8"
            >
              <InstanceIcon icon={i.icon} size={36} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13.5px] font-semibold text-bone-50">{i.name}</span>
                <span className="block truncate text-[12px] text-ash-400">
                  {loaderLine(i, t(`loaders.${i.loader}`))}
                  {i.managed ? ` / ${t("instances.managed")}` : ""}
                </span>
              </span>
              {selected?.id === i.id ? <Check size={16} className="text-gold-400" /> : null}
            </DropdownMenu.Item>
          ))}
          <DropdownMenu.Separator className="my-1.5 h-px bg-white/8" />
          <DropdownMenu.Item
            onSelect={() => void navigate({ to: "/library" })}
            className="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-2 text-[13px] text-ash-300 outline-none data-[highlighted]:bg-white/8 data-[highlighted]:text-bone-50"
          >
            <FolderOpen size={17} /> {t("instances.openLibrary")}
          </DropdownMenu.Item>
          <DropdownMenu.Item
            onSelect={() => void navigate({ to: "/library", search: { create: true } })}
            className="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-2 text-[13px] text-ash-300 outline-none data-[highlighted]:bg-white/8 data-[highlighted]:text-bone-50"
          >
            <Plus size={17} /> {t("instances.create")}
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

function StatusArea({ selected }: { selected: InstanceSummary | undefined }) {
  const { t } = useTranslation();
  const phase = useLaunch((s) => s.phase);
  const autoJoin = usePrefs((s) => s.autoJoin);
  const setAutoJoin = usePrefs((s) => s.setAutoJoin);

  if (phase.kind === "working") {
    return (
      <div className="min-w-0 flex-1" aria-live="polite">
        <div className="flex items-baseline justify-between gap-4 text-[12.5px]">
          <span className="truncate font-medium text-bone-100">{t(`play.stages.${phase.stage}`)}</span>
          <span className="flex shrink-0 gap-4 text-ash-400 tabular-nums">
            {phase.totalFiles > 0 ? (
              <span>
                {t("play.files", { done: formatCount(phase.doneFiles), total: formatCount(phase.totalFiles) })}
              </span>
            ) : null}
            {phase.bytesPerSec > 0 ? <span>{formatSpeed(phase.bytesPerSec)}</span> : null}
            {phase.etaSecs ? <span>{t("play.eta", { time: formatDuration(phase.etaSecs) })}</span> : null}
          </span>
        </div>
        <Progress value={phase.percent} label={t(`play.stages.${phase.stage}`)} className="mt-2.5" />
      </div>
    );
  }

  return (
    <div className="flex min-w-0 flex-1 items-center justify-between gap-6">
      <div className="min-w-0 text-[12.5px] text-ash-400">
        {selected ? (
          <>
            <span className="block truncate">
              {selected.lastPlayedAt
                ? t("play.lastPlayed", { when: formatRelative(selected.lastPlayedAt) })
                : t("play.neverPlayed")}
            </span>
            {selected.playTimeSecs > 0 ? (
              <span className="block truncate">{t("play.playTime", { time: formatDuration(selected.playTimeSecs) })}</span>
            ) : null}
          </>
        ) : null}
      </div>
      {selected?.autoJoin ? (
        <Switch
          checked={autoJoin}
          onCheckedChange={setAutoJoin}
          label={t("play.autoJoin")}
          description={selected.autoJoin}
          className="shrink-0"
        />
      ) : null}
    </div>
  );
}

function AccountChip() {
  const { t } = useTranslation();
  const { data: accounts } = useQuery({ queryKey: ["accounts"], queryFn: () => api.accounts() });
  const active = accounts?.find((a) => a.active);
  if (!active) return null;
  return (
    <div className="flex shrink-0 items-center gap-2.5 border-l border-white/8 pl-5">
      <PlayerHead name={active.username} src={active.headUrl} size={34} />
      <div className="leading-tight">
        <span className="block text-[13.5px] font-semibold text-bone-50">{active.username}</span>
        <span className="block text-[11.5px] text-ash-400">
          {active.kind === "microsoft" ? t("accounts.microsoft") : t("accounts.offline")}
        </span>
      </div>
    </div>
  );
}

/** Bottom command bar: what to play, as whom, progress, and the OYNA button. */
export function CommandBar() {
  const { t } = useTranslation();
  const { data: instances = [] } = useQuery({ queryKey: ["instances"], queryFn: () => api.instances() });
  const selectedId = usePrefs((s) => s.selectedInstanceId);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const { phase, play, cancel } = useLaunch();

  const selected = useMemo(
    () => instances.find((i) => i.id === selectedId) ?? instances[0],
    [instances, selectedId],
  );

  useEffect(() => {
    if (phase.kind === "error") toast.error(phase.message);
  }, [phase]);

  const state: PlayState = (() => {
    switch (phase.kind) {
      case "working":
        return { kind: "working", percent: phase.percent, label: t(`play.stages.${phase.stage}`) };
      case "launching":
        return { kind: "launching", label: t("play.launching") };
      case "running":
        return { kind: "running" };
      default:
        return selected?.state === "ready" || !selected ? { kind: "ready" } : { kind: "update" };
    }
  })();

  return (
    <footer className="glass-strong relative z-20 flex h-[88px] shrink-0 items-center gap-5 border-x-0 border-b-0 pr-6 pl-4">
      <InstancePicker instances={instances} selected={selected} onSelect={selectInstance} />
      <AccountChip />
      <div className={cn("flex min-w-0 flex-1 items-center border-l border-white/8 pl-5")}>
        <StatusArea selected={selected} />
      </div>
      <PlayButton
        state={state}
        onPlay={() => {
          if (selected) void play(selected.id);
        }}
        onCancel={() => void cancel()}
        labels={{
          play: t("play.play"),
          update: t("play.update"),
          cancel: t("play.cancel"),
          running: t("play.running"),
        }}
      />
    </footer>
  );
}
