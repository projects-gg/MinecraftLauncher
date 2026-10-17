import { useQuery } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import { useMemo } from "react";
import { api, type DroppedFile, type InstanceSummary } from "@/lib/api";
import { usePrefs } from "@/stores/prefs";
import { type DropPlan, type DropTarget, planDrop } from "./model";

export const toTarget = (instance: InstanceSummary | undefined): DropTarget | null =>
  instance ? { id: instance.id, modded: instance.loader !== "vanilla" } : null;

/** The instance a drop goes into: the open instance page, else the command bar's choice. */
export function useDropTarget(): InstanceSummary | undefined {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const selectedId = usePrefs((s) => s.selectedInstanceId);
  const { data: instances = [] } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const match = /^\/library\/([^/]+)$/.exec(pathname)?.[1];
  const pageId = match ? decodeURIComponent(match) : undefined;
  // The open page's own query (cached by the page) covers an instance the list does not know yet.
  const { data: page } = useQuery({
    queryKey: ["instance", pageId],
    queryFn: () => api.instance(pageId ?? ""),
    enabled: Boolean(pageId),
  });
  return useMemo(() => {
    const onPage = pageId ? (instances.find((i) => i.id === pageId) ?? page) : undefined;
    return onPage ?? instances.find((i) => i.id === selectedId) ?? instances[0];
  }, [instances, page, pageId, selectedId]);
}

/** What the window would do with the dragged files (null while they are still being read). */
export function useDropPlan(
  files: DroppedFile[] | null | undefined,
  target: InstanceSummary | undefined,
): DropPlan | null {
  return useMemo(() => (files ? planDrop(files, toTarget(target)) : null), [files, target]);
}
