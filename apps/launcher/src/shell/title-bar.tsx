import { Check, CopySimple, MagnifyingGlass, Minus, Square, X } from "@phosphor-icons/react";
import { cn, easeOutExpo, SignalBars, Tooltip, useReduceMotion } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AccountSwitcher } from "@/features/accounts/account-switcher";
import { AccountsHost } from "@/features/accounts/accounts-host";
import { FriendsPill } from "@/features/social/friends-pill";
import { SocialHost } from "@/features/social/social-host";
import { UpdatePill } from "@/features/updates/update-ui";
import { api } from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { formatCount } from "@/lib/format";
import { detectOs, isTauri } from "@/lib/platform";
import { ariaKeyShortcuts } from "@/lib/shortcuts";
import { useLaunch } from "@/stores/launch";
import { useUi } from "@/stores/ui";
import { useUpdate } from "@/stores/update";
import { ShortcutKeys } from "./shortcut-keys";

/** The address the pill copies until the status says otherwise. */
const DEFAULT_SERVER_ADDRESS = "play.projects.gg";

function ServerPill() {
  const { t } = useTranslation();
  const reduce = useReduceMotion();
  // Every open launcher pings the game server, so keep it light: once a minute, only while the
  // window is on screen (react-query pauses intervals in the tray or minimized, see main.tsx)
  // and never while the player is already in the game.
  const inGame = useLaunch((s) => s.phase.kind === "running");
  const { data, isPending } = useQuery({
    queryKey: ["server-status"],
    queryFn: () => api.serverStatus(),
    refetchInterval: inGame ? false : 60_000,
    refetchOnWindowFocus: true,
    staleTime: 30_000,
  });
  // A click copies the address; the pill says so in place for a moment.
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const address = data?.address || DEFAULT_SERVER_ADDRESS;
  const copy = async () => {
    try {
      await copyText(address);
    } catch {
      return;
    }
    setCopied(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), 1600);
  };

  const online = data?.online ?? false;
  const label = isPending
    ? t("server.checking")
    : online
      ? t("server.online", { players: formatCount(data?.players ?? 0) })
      : t("server.offline");

  return (
    <Tooltip
      content={
        <span className="flex items-center gap-1.5">
          <CopySimple size={13} aria-hidden className="text-ash-300" />
          {data?.latencyMs ? `${address}, ${t("server.latency", { ms: data.latencyMs })}` : address}
        </span>
      }
    >
      <button
        type="button"
        onClick={() => void copy()}
        aria-label={t("server.copy", { address })}
        className="relative flex h-8 cursor-pointer items-center rounded-full bg-obsidian-950/45 px-3 text-[12.5px] ring-1 ring-white/8 backdrop-blur-md transition-[background-color,box-shadow,transform,scale] duration-150 ease-out hover:bg-obsidian-950/65 hover:ring-white/14 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none active:scale-[0.97]"
      >
        <motion.span
          className="flex items-center gap-2"
          initial={false}
          animate={{
            opacity: copied ? 0 : 1,
            transition: { duration: copied ? 0.1 : 0.2, ease: easeOutExpo },
          }}
        >
          <SignalBars
            state={isPending ? "pending" : online ? "online" : "offline"}
            latencyMs={data?.latencyMs}
          />
          <span className="font-display font-semibold text-bone-50">
            {data?.name ?? "Projects"}
          </span>
          <span className="text-ash-300 tabular-nums">{label}</span>
        </motion.span>
        <AnimatePresence>
          {copied ? (
            <motion.span
              key="copied"
              className="absolute inset-0 flex items-center justify-center gap-1.5 font-medium text-jade-400"
              initial={{ opacity: 0, transform: reduce ? "none" : "translateY(5px)" }}
              animate={{
                opacity: 1,
                transform: reduce ? "none" : "translateY(0px)",
                transition: { duration: 0.22, ease: easeOutExpo, delay: 0.05 },
              }}
              exit={{
                opacity: 0,
                transform: reduce ? "none" : "translateY(-4px)",
                transition: { duration: 0.14, ease: easeOutExpo },
              }}
            >
              <Check size={14} weight="bold" aria-hidden />
              {t("server.copied")}
            </motion.span>
          ) : null}
        </AnimatePresence>
        <span className="sr-only" aria-live="polite">
          {copied ? t("server.copied") : ""}
        </span>
      </button>
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
    "flex h-titlebar w-[46px] items-center justify-center text-ash-300 transition-colors duration-150 hover:text-bone-50";
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

function PaletteButton() {
  const { t } = useTranslation();
  const setOpen = useUi((s) => s.setPaletteOpen);
  return (
    <button
      type="button"
      onClick={() => setOpen(true)}
      aria-keyshortcuts={ariaKeyShortcuts("palette", detectOs())}
      className="flex h-8 cursor-pointer items-center gap-2 rounded-md px-2.5 text-[12.5px] text-ash-300 transition-[background-color,color,transform,scale] duration-150 ease-out hover:bg-white/8 hover:text-bone-50 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none active:scale-[0.97]"
    >
      <MagnifyingGlass size={14} aria-hidden />
      {t("palette.open")}
      <ShortcutKeys id="palette" />
    </button>
  );
}

/**
 * Frameless title bar over the art. macOS keeps its native traffic lights (overlay style).
 * The main window's bar (`search`) also carries the account switcher and hosts the account
 * dialogs; the first-run flow's bar does not.
 */
export function TitleBar({ search = true }: { search?: boolean }) {
  const mac = detectOs() === "macos";
  // The mandatory update screen sits below the bar so the window can still be moved, minimized
  // or closed; everything else on the bar steps out of the way meanwhile.
  const updateLocked = useUpdate((s) => s.mandatory && s.phase !== "idle");
  const aside = cn(
    "flex items-center transition-opacity duration-200",
    updateLocked && "pointer-events-none opacity-0",
  );
  return (
    <header
      data-tauri-drag-region
      className="relative z-30 flex h-titlebar shrink-0 items-center justify-between pl-3"
    >
      <div data-tauri-drag-region inert={updateLocked} className={cn(aside, "gap-3")}>
        <ServerPill />
      </div>
      <div className="flex items-center gap-2.5">
        <div inert={updateLocked} className={cn(aside, "gap-2.5")}>
          {search ? (
            <>
              <FriendsPill />
              <AccountSwitcher />
              <AccountsHost />
              <SocialHost />
            </>
          ) : null}
          {search ? <PaletteButton /> : null}
          <UpdatePill />
        </div>
        {mac ? null : <WindowControls />}
      </div>
    </header>
  );
}
