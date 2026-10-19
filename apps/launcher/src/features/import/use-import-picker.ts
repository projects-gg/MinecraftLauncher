import { useQueries } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { api, type ImportScanView, type SourceMeasure } from "@/lib/api";
import {
  defaultTicks,
  fits,
  measuring,
  type RowKey,
  requestOf,
  rowsOf,
  selectedBytes,
  ticksOf,
} from "./model";

export const importKeys = {
  scan: ["import-scan"] as const,
  measure: (sourceId: string) => ["import-measure", sourceId] as const,
};

/**
 * The rows, ticks and totals of an import picker over `scan`. Each source is measured as soon as
 * the picker shows (sizes, the mods that fit); its rows show at once and fill in.
 */
export function useImportPicker(scan: ImportScanView | undefined) {
  const sources = scan?.sources ?? [];
  const measured = useQueries({
    queries: sources.map((source) => ({
      queryKey: importKeys.measure(source.id),
      queryFn: () => api.importer.measure(source.id),
      staleTime: 60_000,
      retry: false,
    })),
    combine: (results) => results.map((result) => result.data),
  });
  const measures = useMemo(() => {
    const out: Record<string, SourceMeasure | undefined> = {};
    sources.forEach((source, index) => {
      out[source.id] = measured[index];
    });
    return out;
  }, [sources, measured]);

  const rows = useMemo(() => (scan ? rowsOf(scan, measures) : []), [scan, measures]);
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
