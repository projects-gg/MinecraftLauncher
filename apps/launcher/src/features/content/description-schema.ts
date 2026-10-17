// What a Modrinth project description (Markdown with some HTML, written by the mod's author) may
// show: the sanitize schema and the image filter of features/content/description.tsx.
import { defaultSchema } from "rehype-sanitize";

/** Hosts the launcher may load images from (see the CSP `img-src`). Others are left out. */
const IMAGE_HOSTS = new Set(["cdn.modrinth.com", "raw.githubusercontent.com"]);

export function allowedImage(src: unknown): src is string {
  if (typeof src !== "string") return false;
  try {
    const url = new URL(src);
    return url.protocol === "https:" && IMAGE_HOSTS.has(url.hostname);
  } catch {
    return false;
  }
}

/**
 * GitHub's sanitize schema plus `align` (centered blocks are common on Modrinth). Scripts,
 * styles, iframes, forms and event handlers never survive; unknown tags like `<center>` are
 * unwrapped (their content stays). `<source>` goes too: in a `<picture>` its `srcset` would load
 * a picture from any host past the image filter (a tracking pixel would learn the player's
 * address); the `<img>` inside still shows.
 */
export const descriptionSchema = {
  ...defaultSchema,
  tagNames: (defaultSchema.tagNames ?? []).filter((name) => name !== "source"),
  attributes: {
    ...defaultSchema.attributes,
    "*": [...(defaultSchema.attributes?.["*"] ?? []), "align"],
    img: [...(defaultSchema.attributes?.img ?? []), "width", "height"],
  },
};
