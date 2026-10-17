import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
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
