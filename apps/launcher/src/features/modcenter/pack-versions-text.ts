// PACK-EDIT text helpers shared by the mod center rows, the version list and the launch toasts.

/** `0.141.1+26.1.2` → `0.141.1`: a list for one game version, its suffix says nothing. */
export function shortVersion(number: string): string {
  const plus = number.indexOf("+");
  return plus > 0 ? number.slice(0, plus) : number;
}
