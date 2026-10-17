import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useDropClaim } from "@/features/drop/store";
import { api, type ContentKind } from "@/lib/api";

/**
 * Installed add-ons of an instance with their Modrinth projects and available updates.
 * Shared by the browser (installed state) and the mods/packs tabs ("check for updates").
 */
export function useContentScan(instanceId: string | undefined, kind: ContentKind, enabled = true) {
  return useQuery({
    queryKey: ["content-scan", instanceId, kind],
    queryFn: () => api.content.scan(instanceId ?? "", kind, true),
    enabled: Boolean(instanceId) && enabled,
    staleTime: 5 * 60_000,
    retry: false,
  });
}

/** `value`, settled for `ms` (search-as-you-type). */
export function useDebounced<T>(value: T, ms = 300): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return settled;
}

/**
 * Files dragged onto the window while `enabled` (the window-wide drop layer steps aside then).
 * Returns whether a drag is over the window.
 */
export function useFileDrop(enabled: boolean, onDrop: (paths: string[]) => void): boolean {
  const [over, setOver] = useState(false);
  const handler = useRef(onDrop);
  handler.current = onDrop;
  useDropClaim(enabled);
  useEffect(() => {
    if (!enabled) return;
    const unsubscribe = api.content.onFileDrop((event) => {
      if (event.type === "over") setOver(true);
      else if (event.type === "leave") setOver(false);
      else {
        setOver(false);
        if (event.paths.length > 0) handler.current(event.paths);
      }
    });
    return () => {
      unsubscribe();
      setOver(false);
    };
  }, [enabled]);
  return over;
}
