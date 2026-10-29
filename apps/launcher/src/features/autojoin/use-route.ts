// AUTOJOIN: the address picker's rows for an instance and its current pick. The sub-servers come
// from Rust (Studio's switch, a Projects address, our overlay mod for the instance's game); the
// beta program is asked only when Studio lists a "Yalnız testçiler" row, at most every ten minutes.
import { useQuery } from "@tanstack/react-query";
import { api, type InstanceSummary } from "@/lib/api";
import { usePrefs } from "@/stores/prefs";
import { currentRoute, type RouteRow, routeRows } from "./model";

export const SUB_SERVERS_KEY = "instance-sub-servers";

export function useRoute(instance: InstanceSummary | undefined): {
  rows: RouteRow[];
  current: RouteRow | null;
} {
  // The overlay's jar goes in for either switch: Rust's answer follows them.
  const overlay = usePrefs((s) => s.inGameOverlay);
  const emoji = usePrefs((s) => s.chatEmoji);
  const saved = usePrefs((s) => (instance ? s.autoJoinRoutes?.[instance.id] : undefined));
  const id = instance?.id;
  const address = instance?.autoJoin;
  const { data: subs = [] } = useQuery({
    queryKey: [SUB_SERVERS_KEY, id, overlay, emoji],
    queryFn: () => api.instanceSubServers(id ?? ""),
    enabled: id !== undefined && address !== undefined,
    staleTime: 5 * 60_000,
  });
  const askTester = subs.some((sub) => sub.testersOnly);
  const { data: tester = false } = useQuery({
    queryKey: ["sub-servers-tester"],
    queryFn: () => api.subServersTester(),
    enabled: askTester,
    staleTime: 10 * 60_000,
    refetchOnWindowFocus: false,
  });
  const rows = routeRows(address, subs, tester);
  return { rows, current: currentRoute(saved, rows) };
}
