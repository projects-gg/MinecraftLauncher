// "Yer aç" from anywhere: Settings → Veri ve depolama, where the cleanup shows what can go and
// how much room it makes. For toasts raised outside components (content jobs); the router is
// loaded on demand so this module stays out of the page tree's imports.
export function openMakeSpace() {
  void import("@/router").then(({ router }) =>
    router.navigate({ to: "/settings", search: { section: "data" } }),
  );
}
