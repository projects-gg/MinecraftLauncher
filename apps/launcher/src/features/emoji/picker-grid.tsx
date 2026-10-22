// The picker's grid (CHAT2, docs: chat2.md §3.1, §8.5): sections under a heading, cells in rows,
// drawn only where they are on screen (virtualized: 1,910 emoji, ~50 drawn). One Tab stop, the
// grid itself; the active cell is named to screen readers with aria-activedescendant, so a wheel
// that scrolls it away never drops the focus. Arrows move cell by cell across rows and sections,
// Home and End go to the row's ends, Ctrl+Home and Ctrl+End to the first and the last, Page Up and
// Page Down a page; Enter or Space picks; a letter goes to the search field and ↑ on the first row
// goes back to it. Moving into another section says its name first.
import { defaultRangeExtractor, type Range, useVirtualizer } from "@tanstack/react-virtual";
import {
  type CSSProperties,
  forwardRef,
  type KeyboardEvent,
  type ReactNode,
  type Ref,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";

export interface GridSection<T> {
  id: string;
  /** Its heading; none for a single unnamed section (search results). */
  title: string | null;
  items: readonly T[];
}

export interface PickerGridHandle {
  /** Scrolls the section's heading to the top. */
  scrollToSection: (id: string) => void;
  /** Focuses the grid on its first cell (or the cell it was on). */
  focus: () => void;
}

interface Props<T> {
  id: string;
  label: string;
  sections: readonly GridSection<T>[];
  columns: number;
  cellSize: number;
  headerSize: number;
  /** Visible height of the grid in pixels. */
  height: number;
  itemKey: (item: T) => string;
  itemLabel: (item: T) => string;
  renderItem: (item: T) => ReactNode;
  onPick: (item: T, cell: HTMLElement) => void;
  /** The item under the pointer or the keyboard (the footer shows it). */
  onPreview: (item: T | null) => void;
  /** The section at the top of the view changed (the rail follows). */
  onSection?: (id: string) => void;
  /** ↑ on the first row. */
  onLeaveTop?: () => void;
  /** A printable key while the grid has the focus. */
  onTypeAhead?: () => void;
  /** Shown instead of the grid when there is nothing in it. */
  empty?: ReactNode;
  cellClassName?: string;
}

type Row<T> =
  | { kind: "header"; section: number; title: string }
  | { kind: "cells"; section: number; items: readonly T[]; ariaRow: number };

interface Active {
  row: number;
  col: number;
  /** Its section differs from the one announced before: its name is read first. */
  announce: boolean;
}

function buildRows<T>(sections: readonly GridSection<T>[], columns: number): Row<T>[] {
  const rows: Row<T>[] = [];
  let ariaRow = 0;
  sections.forEach((section, index) => {
    if (!section.items.length) return;
    if (section.title) rows.push({ kind: "header", section: index, title: section.title });
    for (let at = 0; at < section.items.length; at += columns) {
      ariaRow += 1;
      rows.push({
        kind: "cells",
        section: index,
        items: section.items.slice(at, at + columns),
        ariaRow,
      });
    }
  });
  return rows;
}

function PickerGridInner<T>(
  {
    id,
    label,
    sections,
    columns,
    cellSize,
    headerSize,
    height,
    itemKey,
    itemLabel,
    renderItem,
    onPick,
    onPreview,
    onSection,
    onLeaveTop,
    onTypeAhead,
    empty,
    cellClassName,
  }: Props<T>,
  ref: Ref<PickerGridHandle>,
) {
  const rows = useMemo(() => buildRows(sections, columns), [sections, columns]);
  const scroller = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState<Active | null>(null);
  const announced = useRef<number | null>(null);
  const activeRow = useRef<number | null>(null);
  activeRow.current = active?.row ?? null;

  // The active row stays drawn while it is scrolled away, so the screen reader keeps its cell.
  const rangeExtractor = useCallback((range: Range) => {
    const indexes = defaultRangeExtractor(range);
    const keep = activeRow.current;
    if (keep !== null && !indexes.includes(keep)) {
      indexes.push(keep);
      indexes.sort((a, b) => a - b);
    }
    return indexes;
  }, []);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: (index) => (rows[index]?.kind === "header" ? headerSize : cellSize),
    overscan: 3,
    rangeExtractor,
  });

  // New sections (a search, the recents) start the grid over at its top.
  useEffect(() => {
    announced.current = null;
    setActive((current) => {
      if (!current) return null;
      return firstCell(rows) ?? null;
    });
    scroller.current?.scrollTo({ top: 0 });
  }, [rows]);

  const rowAt = (index: number) => rows[index];
  const itemAt = (cell: Active | null) => {
    if (!cell) return undefined;
    const row = rowAt(cell.row);
    return row?.kind === "cells" ? row.items[cell.col] : undefined;
  };

  const activate = (row: number, col: number) => {
    const target = rowAt(row);
    if (target?.kind !== "cells") return;
    const announce = announced.current !== target.section;
    announced.current = target.section;
    const next = { row, col: Math.min(col, target.items.length - 1), announce };
    setActive(next);
    virtualizer.scrollToIndex(row, { align: "auto" });
    onPreview(target.items[next.col] ?? null);
  };

  useImperativeHandle(ref, () => ({
    scrollToSection: (sectionId) => {
      const section = sections.findIndex((s) => s.id === sectionId);
      const index = rows.findIndex((row) => row.section === section);
      if (index >= 0) virtualizer.scrollToIndex(index, { align: "start" });
    },
    focus: () => {
      scroller.current?.focus({ preventScroll: true });
      const start = active ?? firstCell(rows);
      if (start) activate(start.row, start.col);
    },
  }));

  // The section at the top of the view, for the rail.
  const offset = virtualizer.scrollOffset ?? 0;
  const topItem = virtualizer.getVirtualItems().find((item) => item.end > offset + 1);
  const topSection = topItem ? (rows[topItem.index]?.section ?? 0) : 0;
  const topSectionId = sections[topSection]?.id;
  useEffect(() => {
    if (topSectionId) onSection?.(topSectionId);
  }, [topSectionId, onSection]);

  const step = (from: Active, delta: number): [number, number] | null => {
    // Reading order across rows and sections.
    let row = from.row;
    let col = from.col + delta;
    for (;;) {
      const current = rowAt(row);
      if (current?.kind === "cells" && col >= 0 && col < current.items.length) return [row, col];
      row += delta > 0 ? 1 : -1;
      const next = rowAt(row);
      if (!next) return null;
      if (next.kind === "cells") col = delta > 0 ? 0 : next.items.length - 1;
    }
  };
  const vertical = (from: Active, direction: 1 | -1, count = 1): [number, number] | null => {
    let row = from.row;
    let moved = 0;
    let last: [number, number] | null = null;
    for (;;) {
      row += direction;
      const next = rowAt(row);
      if (!next) return last;
      if (next.kind !== "cells") continue;
      last = [row, Math.min(from.col, next.items.length - 1)];
      moved += 1;
      if (moved === count) return last;
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const printable = event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey;
    if (printable && event.key !== " ") {
      onTypeAhead?.();
      return;
    }
    const current = active ?? firstCell(rows);
    if (!current) return;
    const page = Math.max(1, Math.floor(height / cellSize) - 1);
    let target: [number, number] | null = null;
    switch (event.key) {
      case "ArrowRight":
        target = step(current, 1);
        break;
      case "ArrowLeft":
        target = step(current, -1);
        break;
      case "ArrowDown":
        target = active ? vertical(current, 1) : [current.row, current.col];
        break;
      case "ArrowUp":
        target = vertical(current, -1);
        if (!target && onLeaveTop) {
          event.preventDefault();
          onLeaveTop();
          return;
        }
        break;
      case "PageDown":
        target = vertical(current, 1, page);
        break;
      case "PageUp":
        target = vertical(current, -1, page);
        break;
      case "Home":
        target = event.ctrlKey ? cellOf(firstCell(rows)) : [current.row, 0];
        break;
      case "End": {
        if (event.ctrlKey) target = cellOf(lastCell(rows));
        else {
          const row = rowAt(current.row);
          target = row?.kind === "cells" ? [current.row, row.items.length - 1] : null;
        }
        break;
      }
      case "Enter":
      case " ": {
        event.preventDefault();
        const item = itemAt(current);
        const cell = document.getElementById(cellId(id, current.row, current.col));
        if (item !== undefined && cell) onPick(item, cell);
        return;
      }
      default:
        return;
    }
    event.preventDefault();
    if (target) activate(target[0], target[1]);
  };

  if (!rows.length) {
    return (
      <div className="flex items-center justify-center" style={{ height }}>
        {empty}
      </div>
    );
  }

  const activeItem = itemAt(active);
  const activeSectionTitle =
    active?.announce && rows[active.row] ? sections[rows[active.row]?.section ?? 0]?.title : null;

  return (
    // biome-ignore lint/a11y/useSemanticElements: an ARIA grid; no native element is one
    <div
      ref={scroller}
      role="grid"
      tabIndex={0}
      aria-label={label}
      aria-rowcount={rows.filter((row) => row.kind === "cells").length}
      aria-colcount={columns}
      aria-activedescendant={
        active && activeItem !== undefined ? cellId(id, active.row, active.col) : undefined
      }
      onKeyDown={onKeyDown}
      onFocus={(event) => {
        if (event.target !== event.currentTarget || active) return;
        const start = firstCell(rows);
        if (start) activate(start.row, start.col);
      }}
      onPointerLeave={() => onPreview(activeItem ?? null)}
      className="picker-grid relative overflow-y-auto overscroll-contain outline-none [scrollbar-gutter:stable_both-edges]"
      style={{ height }}
    >
      {/* The rows centred in the view, whatever the scroll bar takes. */}
      <div
        className="relative mx-auto"
        style={{ height: virtualizer.getTotalSize(), width: columns * cellSize }}
      >
        {virtualizer.getVirtualItems().map((virtual) => {
          const row = rows[virtual.index];
          if (!row) return null;
          const style: CSSProperties = {
            height: virtual.size,
            transform: `translateY(${virtual.start}px)`,
          };
          if (row.kind === "header") {
            return (
              <div
                key={virtual.key}
                aria-hidden
                className="absolute top-0 left-0 flex w-full items-end px-1.5 pb-1.5 text-[12px] font-semibold text-ash-300"
                style={style}
              >
                {row.title}
              </div>
            );
          }
          return (
            // biome-ignore lint/a11y/useSemanticElements: a row of the ARIA grid above
            // biome-ignore lint/a11y/useFocusableInteractive: the grid holds the focus (aria-activedescendant)
            <div
              key={virtual.key}
              role="row"
              aria-rowindex={row.ariaRow}
              className="absolute top-0 left-0 flex w-full"
              style={style}
            >
              {row.items.map((item, col) => {
                const isActive = active?.row === virtual.index && active.col === col;
                const name = itemLabel(item);
                return (
                  // biome-ignore lint/a11y/useSemanticElements: a cell of the ARIA grid above
                  // biome-ignore lint/a11y/useFocusableInteractive: the grid holds the focus (aria-activedescendant)
                  // biome-ignore lint/a11y/useKeyWithClickEvents: the grid's keys pick the active cell
                  <div
                    key={itemKey(item)}
                    id={cellId(id, virtual.index, col)}
                    role="gridcell"
                    aria-colindex={col + 1}
                    aria-label={
                      isActive && activeSectionTitle ? `${activeSectionTitle}, ${name}` : name
                    }
                    data-active={isActive ? "" : undefined}
                    className={`picker-cell relative flex shrink-0 cursor-pointer items-center justify-center ${cellClassName ?? ""}`}
                    style={{ width: cellSize, height: cellSize }}
                    onPointerEnter={() => onPreview(item)}
                    onClick={(event) => {
                      announced.current = row.section;
                      setActive({ row: virtual.index, col, announce: false });
                      onPick(item, event.currentTarget);
                    }}
                  >
                    <span aria-hidden className="picker-plate" />
                    <span aria-hidden className="picker-ring" />
                    {renderItem(item)}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function firstCell<T>(rows: readonly Row<T>[]): Active | null {
  const row = rows.findIndex((r) => r.kind === "cells");
  return row >= 0 ? { row, col: 0, announce: true } : null;
}

function lastCell<T>(rows: readonly Row<T>[]): Active | null {
  for (let row = rows.length - 1; row >= 0; row--) {
    const r = rows[row];
    if (r?.kind === "cells") return { row, col: r.items.length - 1, announce: true };
  }
  return null;
}

const cellOf = (cell: Active | null): [number, number] | null =>
  cell ? [cell.row, cell.col] : null;

const cellId = (id: string, row: number, col: number) => `${id}-${row}-${col}`;

/** The grid, generic over its items (forwardRef keeps the handle). */
export const PickerGrid = forwardRef(PickerGridInner) as <T>(
  props: Props<T> & { ref?: Ref<PickerGridHandle> },
) => ReturnType<typeof PickerGridInner>;
