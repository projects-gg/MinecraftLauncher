// Small pieces the overview's sections share: section heads, icon tiles, the server's players.
import { Button, cn, SignalBars, type SignalState } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { swap } from "@/features/modcenter/motion";
import { api } from "@/lib/api";
import { formatCount } from "@/lib/format";

/**
 * A section's title and, when the section leaves some out, "Tümü" to the tab with the whole
 * list. The row keeps its height either way, so sections line up.
 */
export function SectionHead({
  id,
  title,
  onAll,
}: {
  id: string;
  title: string;
  onAll?: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="mb-1 flex min-h-8 items-center justify-between gap-4 pl-1">
      <h3 id={id} className="font-display text-[15px] font-semibold text-bone-50">
        {title}
      </h3>
      {onAll ? (
        <Button size="sm" variant="ghost" className="-mr-1" aria-describedby={id} onClick={onAll}>
          {t("overview.all")}
        </Button>
      ) : null}
    </div>
  );
}

/**
 * The look of a secondary button inside a larger clickable surface (a whole card is the
 * button; a button inside a button is not allowed). It presses with the surface.
 */
export function FauxButton({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <span
      aria-hidden
      className="relative isolate inline-flex h-[32px] shrink-0 items-center gap-2 px-3.5 text-[13px] whitespace-nowrap transition-transform duration-150 ease-out [--cut:6px] group-active:scale-[0.97] group-disabled:opacity-45 group-disabled:group-active:scale-100"
    >
      <span className="chamfer absolute inset-0 -z-10 bg-white/12 transition-colors group-hover:bg-white/20 group-disabled:bg-white/12" />
      <span className="chamfer absolute inset-px -z-10 bg-obsidian-800/85 transition-colors group-hover:bg-obsidian-700/90 group-disabled:bg-obsidian-800/85" />
      <span className="flex shrink-0 text-gold-300">{icon}</span>
      <span className="leading-none font-medium text-bone-100">{children}</span>
    </span>
  );
}

/**
 * A world's or server's own icon in a chamfered tile, the mark of its kind when it has none or
 * it fails to load. Fades in once loaded (server icons are pixel art, kept sharp).
 */
export function IconTile({
  src,
  fallback,
  pixelated,
  size,
  children,
}: {
  src?: string;
  fallback: ReactNode;
  pixelated?: boolean;
  /** Tailwind size class (`size-9`) and cut. */
  size: string;
  /** Drawn over the picture (the play mark of a card). */
  children?: ReactNode;
}) {
  // Per picture, so a tile that is handed another one starts over.
  const [broken, setBroken] = useState<string>();
  const [loaded, setLoaded] = useState<string>();
  const shown = src && src !== broken ? src : undefined;
  return (
    <span
      className={cn(
        "chamfer relative flex shrink-0 items-center justify-center overflow-hidden bg-obsidian-800",
        size,
      )}
    >
      {shown ? (
        <img
          ref={(img) => {
            // Already decoded (cached): no fade from nothing.
            if (img?.complete && img.naturalWidth > 0) setLoaded(shown);
          }}
          src={shown}
          alt=""
          draggable={false}
          onLoad={() => setLoaded(shown)}
          onError={() => setBroken(shown)}
          className={cn(
            "size-full object-cover transition-opacity duration-200 ease-out",
            loaded === shown ? "opacity-100" : "opacity-0",
            pixelated && "[image-rendering:pixelated]",
          )}
        />
      ) : (
        fallback
      )}
      {children}
    </span>
  );
}

/** One server's ping, shared with the servers tab (same query); asked again at most once a minute. */
export function useServerPing(address: string | null) {
  return useQuery({
    queryKey: ["server-ping", (address ?? "").trim().toLowerCase()],
    queryFn: () => api.library.pingServer(address ?? ""),
    enabled: Boolean(address),
    staleTime: 60_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
}

/**
 * Players of a server, or that it cannot be reached. The bars sweep until the ping answers; the
 * words then fade in beside them, so nothing around them moves.
 */
export function ServerPlayers({ address }: { address: string }) {
  const { t } = useTranslation();
  const ping = useServerPing(address);
  const data = ping.data;
  const state: SignalState = data
    ? data.online
      ? "online"
      : "offline"
    : ping.isError
      ? "offline"
      : "pending";
  return (
    <span className="flex h-5 min-w-0 items-center gap-2 whitespace-nowrap tabular-nums">
      <SignalBars state={state} latencyMs={data?.latencyMs} />
      <span className="relative flex min-w-0">
        <AnimatePresence initial={false} mode="popLayout">
          {state === "pending" ? null : (
            <motion.span key={state} {...swap()} className="truncate">
              {data?.online
                ? t("instance.servers.players", {
                    players: formatCount(data.players),
                    max: formatCount(data.maxPlayers),
                  })
                : t("instance.servers.unreachable")}
            </motion.span>
          )}
        </AnimatePresence>
      </span>
    </span>
  );
}
