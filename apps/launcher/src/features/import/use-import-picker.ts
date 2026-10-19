import { type QueryClient, useQueries, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { api, type ImportScanView, type ImportSource, type SourceMeasure } from "@/lib/api";
import {
  defaultTicks,
  fits,
  measureKeys,
  measuring,
  type RowKey,
  requestOf,
  rowsKey,
  rowsOf,
  selectedBytes,
  ticksOf,
} from "./model";

export const importKeys = {
  scan: ["import-scan"] as const,
  /** One source's sizes for the rows it has (`rowsKey`): a profile that turns up is measured. */
  measure: (sourceId: string, rows: string) => ["import-measure", sourceId, rows] as const,
};

/**
 * How long a source's changed rows must hold still before they are measured again: a burst of
 * changes makes one measurement, and none is dropped.
 */
export const REMEASURE_AFTER_MS = 400;

/** The rows each source is measured for (`measureKeys`), with changes settled by a debounce. */
function useMeasureKeys(sources: ImportSource[]): Record<string, string> {
  const current = useMemo(
    () => Object.fromEntries(sources.map((source) => [source.id, rowsKey(source)])),
    [sources],
  );
  const [settled, setSettled] = useState<Record<string, string>>({});
  useEffect(() => {
    if (Object.entries(current).every(([id, key]) => settled[id] === key)) return;
    const timer = setTimeout(
      () => setSettled((done) => ({ ...done, ...current })),
      REMEASURE_AFTER_MS,
    );
    return () => clearTimeout(timer);
  }, [current, settled]);
  return useMemo(() => measureKeys(current, settled), [current, settled]);
}

/** The newest measurement of a source, whatever rows it was made for. */
function lastMeasure(client: QueryClient, sourceId: string): SourceMeasure | undefined {
  let newest: { at: number; data: SourceMeasure } | undefined;
  for (const query of client.getQueryCache().findAll({ queryKey: ["import-measure", sourceId] })) {
    const data = query.state.data as SourceMeasure | undefined;
    if (data && (!newest || query.state.dataUpdatedAt > newest.at)) {
      newest = { at: query.state.dataUpdatedAt, data };
    }
  }
  return newest?.data;
}

/**
 * The rows, ticks and totals of an import picker over `scan`. Each source is measured as soon as
 * the picker shows (sizes, the mods that fit); its rows show at once and fill in. When a profile
 * turns up later, its source is measured again; the sizes already known stay meanwhile.
 */
export function useImportPicker(scan: ImportScanView | undefined) {
  const client = useQueryClient();
  const sources = scan?.sources ?? [];
  const keys = useMeasureKeys(sources);
  const measured = useQueries({
    queries: sources.map((source) => ({
      queryKey: importKeys.measure(source.id, keys[source.id] ?? rowsKey(source)),
      queryFn: () => api.importer.measure(source.id),
      staleTime: 60_000,
      retry: false,
    })),
    combine: (results) => results.map((result) => result.data),
  });
  const { measures, upToDate } = useMemo(() => {
    const out: Record<string, SourceMeasure | undefined> = {};
    const current = new Set<string>();
    sources.forEach((source, index) => {
      const fresh = measured[index];
      out[source.id] = fresh ?? lastMeasure(client, source.id);
      if (fresh && keys[source.id] === rowsKey(source)) current.add(source.id);
    });
    return { measures: out, upToDate: current };
  }, [sources, measured, keys, client]);

  const rows = useMemo(
    () => (scan ? rowsOf(scan, measures, upToDate) : []),
    [scan, measures, upToDate],
  );
  const defaults = useMemo(
    () => defaultTicks(rows, scan?.freeBytes, scan?.reserveBytes ?? 0),
    [rows, scan],
  );
  const [choices, setChoices] = useState<ReadonlyMap<RowKey, boolean>>(() => new Map());
  const ticks = useMemo(() => ticksOf(rows, defaults, choices), [rows, defaults, choices]);

  const total = selectedBytes(rows, ticks);
  const request = useMemo(() => requestOf(rows, ticks), [rows, ticks]);
  return {
    rows,
    ticks,
    toggle: (key: RowKey, on: boolean) => setChoices((current) => new Map(current).set(key, on)),
    total,
    /** A ticked row is still being measured. */
    pending: measuring(rows, ticks),
    enough: fits(total, scan?.freeBytes, scan?.reserveBytes ?? 0),
    freeBytes: scan?.freeBytes ?? undefined,
    request,
    empty: request.sources.length === 0,
  };
}

export type ImportPicker = ReturnType<typeof useImportPicker>;
