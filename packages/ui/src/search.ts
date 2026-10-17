// Search folding shared by the launcher and Studio (`fold`, `matchesQuery`; also importable alone
// as `@pl/ui/search`, without the components). Tests: apps/launcher/src/lib/search.test.ts.

/**
 * Search folding for Turkish players: lower case in Turkish, "I", "İ" and "ı" all become "i",
 * and marks go ("ş" → "s", "ğ" → "g", "ö" → "o"). With plain `toLocaleLowerCase("tr")`, "Iris"
 * becomes "ıris" and the "iris" a player types never finds it.
 */
export function fold(text: string): string {
  return text.toLocaleLowerCase("tr").normalize("NFD").replace(/\p{M}/gu, "").replace(/ı/g, "i");
}

/** Whether `text` contains `query` (both folded); an empty query matches everything. */
export function matchesQuery(text: string, query: string): boolean {
  const needle = fold(query.trim());
  return !needle || fold(text).includes(needle);
}
