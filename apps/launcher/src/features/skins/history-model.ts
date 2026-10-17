// The "Önceki skinler" grid without React: pages, keyboard moves, and an order that holds still
// while the panel is open (a tile never jumps away from under the pointer).

export const COLUMNS = 5;
export const ROWS = 2;
export const PAGE_SIZE = COLUMNS * ROWS;

export function pageCount(total: number): number {
  return Math.max(1, Math.ceil(total / PAGE_SIZE));
}

export function clampPage(page: number, total: number): number {
  return Math.min(Math.max(0, page), pageCount(total) - 1);
}

export function pageOf(index: number): number {
  return Math.floor(Math.max(0, index) / PAGE_SIZE);
}

export function pageItems<T>(items: readonly T[], page: number): T[] {
  return items.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
}

/**
 * Rows the grid keeps room for: always two once there is more than one page, so turning pages
 * never changes its height; otherwise just the rows the skins fill.
 */
export function reservedRows(total: number): number {
  return total > PAGE_SIZE ? ROWS : Math.min(ROWS, Math.max(1, Math.ceil(total / COLUMNS)));
}

/**
 * Where the focus goes for a key, as an index into the whole list; `null` when it stays.
 * Arrows move within the page and across its ends to the neighbouring page, Home and End go to
 * the first and last skin of the page, Page Up and Page Down turn the page (same slot, or the
 * last skin of a shorter page).
 */
export function moveFocus(index: number, key: string, total: number): number | null {
  if (total <= 0) return null;
  const first = pageOf(index) * PAGE_SIZE;
  const last = Math.min(total, first + PAGE_SIZE) - 1;
  let next: number;
  switch (key) {
    case "ArrowRight":
      next = index + 1;
      break;
    case "ArrowLeft":
      next = index - 1;
      break;
    case "ArrowDown":
      next = index + COLUMNS <= last ? index + COLUMNS : index;
      break;
    case "ArrowUp":
      next = index - COLUMNS >= first ? index - COLUMNS : index;
      break;
    case "Home":
      next = first;
      break;
    case "End":
      next = last;
      break;
    case "PageDown":
      next = first + PAGE_SIZE < total ? Math.min(index + PAGE_SIZE, total - 1) : index;
      break;
    case "PageUp":
      next = first > 0 ? index - PAGE_SIZE : index;
      break;
    default:
      return null;
  }
  if (next < 0 || next >= total || next === index) return null;
  return next;
}

/**
 * The ids in display order while the panel stays open: known ids keep their places, ids that
 * are new (a skin just uploaded, say) come first, ids that left are dropped.
 */
export function steadyOrder(previous: readonly string[] | null, next: readonly string[]): string[] {
  if (!previous) return [...next];
  const present = new Set(next);
  const known = new Set(previous);
  return [...next.filter((id) => !known.has(id)), ...previous.filter((id) => present.has(id))];
}

/** Puts `id` back at `index` of an order (an undone removal returns to its place). */
export function insertAt(order: readonly string[], id: string, index: number): string[] {
  const rest = order.filter((other) => other !== id);
  const at = Math.min(Math.max(0, index), rest.length);
  return [...rest.slice(0, at), id, ...rest.slice(at)];
}

/**
 * Whether the section has anything to offer: a skin other than the one worn now. Once shown
 * during a visit it stays (the panel does not jump when the last old skin is removed).
 */
export function hasPreviousSkins(ids: readonly string[], currentId: string | null): boolean {
  return ids.some((id) => id !== currentId);
}
