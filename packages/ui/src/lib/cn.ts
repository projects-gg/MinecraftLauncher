import { type ClassValue, clsx } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

/**
 * The theme's own names (styles.css `@theme`), so tailwind-merge files each class in its group.
 * Unknown to it, a size such as `text-meta` read as a text colour: `cn("text-meta text-ash-300")`
 * lost the size and `cn("text-ash-300 text-meta")` the colour. Colours need nothing: any other
 * `text-*`, `bg-*` or `border-*` name already counts as one. A size given after a `leading-*`
 * replaces it, as tailwind-merge does for its own sizes; put the `leading-*` after the size.
 * cn.test.ts (apps/launcher) keeps these lists in step with styles.css.
 */
const theme = {
  text: ["hero", "page", "section", "title", "body", "ui", "meta", "caption", "micro"],
  spacing: ["titlebar", "rail", "commandbar", "page", "news"],
  radius: ["panel", "head"],
  ease: ["out-expo", "in-out-quart", "drawer"],
  animate: ["shimmer", "kenburns", "indeterminate"],
  font: ["display", "sans"],
};

const merge = extendTailwindMerge({ extend: { theme } });

/** Compose class names, letting later Tailwind utilities win over earlier ones. */
export function cn(...inputs: ClassValue[]) {
  return merge(clsx(inputs));
}
