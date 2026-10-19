// Preview switch `?fresh=1` (or localStorage `pl.mock.fresh = "1"`): the official instance was
// just added and never downloaded. No game, mods, packs, worlds or screenshots yet, a folder of a
// few hundred bytes, and its pack's mods waiting for the first start (POLISH4). `?fresh=own`:
// the same with one mod the player added before the first start. Add `&resume=none` for a never
// played instance's overview.
function freshFlag(): string {
  try {
    return (
      new URLSearchParams(window.location.search).get("fresh") ??
      localStorage.getItem("pl.mock.fresh") ??
      ""
    );
  } catch {
    return "";
  }
}

export function mockFreshOfficial(): boolean {
  const flag = freshFlag();
  return flag === "1" || flag === "own";
}

/** `?fresh=own`: the player already added a mod of their own. */
export function mockFreshOwnMod(): boolean {
  return freshFlag() === "own";
}
