import { CopySimple, Minus, Square, X } from "@phosphor-icons/react";
import { cn, Tooltip } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { UpdatePill } from "@/features/updates/update-ui";
import { api } from "@/lib/api";
import { formatCount } from "@/lib/format";
import { detectOs, isTauri } from "@/lib/platform";

function ServerPill() {
  const { t } = useTranslation();
  const { data, isPending } = useQuery({
    queryKey: ["server-status"],
    queryFn: () => api.serverStatus(),
    refetchInterval: 30_000,
  });

  const online = data?.online ?? false;
  const label = isPending
    ? t("server.checking")
    : online
      ? t("server.online", { players: formatCount(data?.players ?? 0) })
      : t("server.offline");

  return (
    <Tooltip
      content={
        data?.latencyMs
          ? `${data.address} · ${t("server.latency", { ms: data.latencyMs })}`
          : (data?.address ?? "")
      }
    >
      <div className="flex h-7 items-center gap-2 rounded-full bg-obsidian-950/45 px-3 text-[12.5px] ring-1 ring-white/8 backdrop-blur-md">
        <span
          className={cn(
            "size-2 rounded-full",
            isPending
              ? "bg-ash-500"
              : online
                ? "bg-jade-400 shadow-[0_0_8px_var(--color-jade-400)]"
                : "bg-flare-400",
          )}
        />
        <span className="font-display font-semibold text-bone-50">{data?.name ?? "Projects"}</span>
        <span className="text-ash-300 tabular-nums">{label}</span>
      </div>
    </Tooltip>
  );
}

function WindowControls() {
  const { t } = useTranslation();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    void win.isMaximized().then(setMaximized);
    void win
      .onResized(() => {
        void win.isMaximized().then(setMaximized);
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, []);

  const call = (fn: (w: ReturnType<typeof getCurrentWindow>) => Promise<void>) => () => {
    if (isTauri()) void fn(getCurrentWindow());
  };

  const base =
    "flex h-10 w-12 items-center justify-center text-ash-300 transition-colors duration-150 hover:text-bone-50";
  return (
    <div className="flex">
      <button
        type="button"
        aria-label={t("window.minimize")}
        className={cn(base, "hover:bg-white/8")}
        onClick={call((w) => w.minimize())}
      >
        <Minus size={16} />
      </button>
      <button
        type="button"
        aria-label={maximized ? t("window.restore") : t("window.maximize")}
        className={cn(base, "hover:bg-white/8")}
        onClick={call((w) => w.toggleMaximize())}
      >
        {maximized ? <CopySimple size={15} /> : <Square size={14} />}
      </button>
      <button
        type="button"
        aria-label={t("window.close")}
        className={cn(base, "hover:bg-flare-400/85 hover:text-white")}
        onClick={call((w) => w.close())}
      >
        <X size={16} />
      </button>
    </div>
  );
}

/** Frameless title bar over the art. macOS keeps its native traffic lights (overlay style). */
export function TitleBar() {
  const mac = detectOs() === "macos";
  return (
    <header
      data-tauri-drag-region
      className="relative z-30 flex h-10 shrink-0 items-center justify-between pl-4"
    >
      <div data-tauri-drag-region className="flex items-center gap-3">
        <ServerPill />
      </div>
      <div className="flex items-center gap-3">
        <UpdatePill />
        {mac ? null : <WindowControls />}
      </div>
    </header>
  );
}
