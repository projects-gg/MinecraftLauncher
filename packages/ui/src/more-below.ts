import { useCallback, useRef, useState } from "react";

/**
 * Whether more of a scrolling box waits below its visible part, for the box given to the returned
 * ref (it may mount and unmount with a dialog). Measured on scroll, when the box or one of its
 * children changes size, and when children come or go; no polling. The box then fades its last
 * 28 px (`MORE_BELOW_MASK`), so content cut at its edge reads as "scroll", not as the end: a
 * dialog's body, the settings' sections in a short window (QOL-209).
 */
export function useMoreBelow(): [boolean, (el: HTMLElement | null) => void] {
  const [more, setMore] = useState(false);
  const teardown = useRef<(() => void) | null>(null);
  const attach = useCallback((el: HTMLElement | null) => {
    teardown.current?.();
    teardown.current = null;
    if (!el) {
      setMore(false);
      return;
    }
    const measure = () => setMore(el.scrollTop + el.clientHeight < el.scrollHeight - 2);
    const sizes = new ResizeObserver(measure);
    const watch = () => {
      sizes.disconnect();
      sizes.observe(el);
      for (const child of el.children) sizes.observe(child);
      measure();
    };
    const children = new MutationObserver(watch);
    children.observe(el, { childList: true });
    el.addEventListener("scroll", measure, { passive: true });
    watch();
    teardown.current = () => {
      sizes.disconnect();
      children.disconnect();
      el.removeEventListener("scroll", measure);
    };
  }, []);
  return [more, attach];
}

/** The fade over a scrolling box's last 28 px while more waits below: a mask on the box itself. */
export const MORE_BELOW_MASK =
  "[mask-image:linear-gradient(to_bottom,black_calc(100%-28px),transparent)]";
