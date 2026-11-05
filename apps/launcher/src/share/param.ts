// A preview switch from the page URL's query or the route's (`#/…?picker=1`), read once.
export function param(key: string): string | null {
  if (typeof window === "undefined") return null;
  const fromSearch = new URLSearchParams(window.location.search).get(key);
  const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
  return fromSearch ?? fromHash;
}
