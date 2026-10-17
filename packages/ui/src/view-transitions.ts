/**
 * A page change that starts while the previous one still animates (two rail clicks in a row, a
 * `navigate` with `replace` right after a page change) skips the older view transition: its
 * `ready` promise rejects with "AbortError: Transition was skipped". TanStack Router only awaits
 * `updateCallbackDone`, so that expected rejection surfaced as an uncaught error. Call this once
 * before the router starts: every view transition gets its `ready` and `finished` observed; the
 * page itself changes either way. Calling it twice wraps only once.
 */
export function observeSkippedViewTransitions(): void {
  if (typeof document === "undefined" || typeof document.startViewTransition !== "function") {
    return;
  }
  const marked = document as Document & { __plObservedTransitions?: boolean };
  if (marked.__plObservedTransitions) return;
  marked.__plObservedTransitions = true;
  const start = document.startViewTransition.bind(document);
  document.startViewTransition = ((update?: Parameters<typeof start>[0]) => {
    const transition = start(update);
    transition.ready.catch(() => {});
    transition.finished.catch(() => {});
    return transition;
  }) as typeof document.startViewTransition;
}
