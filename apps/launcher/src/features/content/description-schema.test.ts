import rehypeSanitize from "rehype-sanitize";
import { describe, expect, it } from "vitest";
import { allowedImage, descriptionSchema } from "./description-schema";

/** A hast element, as rehype-raw makes from a description's HTML. */
const element = (
  tagName: string,
  properties: Record<string, unknown>,
  children: unknown[] = [],
) => ({
  type: "element",
  tagName,
  properties,
  children,
});

const sanitize = (tree: unknown) =>
  (rehypeSanitize as unknown as (schema: unknown) => (tree: unknown) => unknown)(descriptionSchema)(
    tree,
  );

describe("Modrinth descriptions", () => {
  it("never loads a picture from a host past the image filter", () => {
    // <picture><source srcset="https://tracker…"><img src="https://cdn.modrinth.com/…"></picture>:
    // the browser picks the source, so the tracker would learn the player's address (CR1-F).
    const tree = {
      type: "root",
      children: [
        element("picture", {}, [
          element("source", { srcSet: ["https://tracker.example/pixel.png"] }),
          element("img", { src: "https://cdn.modrinth.com/data/AANobbMI/images/a.png" }),
        ]),
      ],
    };
    const text = JSON.stringify(sanitize(tree));
    expect(text).not.toContain("tracker.example");
    expect(text).toContain("cdn.modrinth.com");
  });

  it("keeps centered blocks and image sizes", () => {
    const tree = {
      type: "root",
      children: [
        element("p", { align: "center" }, [
          element("img", { src: "https://cdn.modrinth.com/a.png", width: 64, onError: "x()" }),
        ]),
      ],
    };
    const text = JSON.stringify(sanitize(tree));
    expect(text).toContain('"align":"center"');
    expect(text).toContain('"width":64');
    expect(text).not.toContain("onError");
  });

  it("shows images only from the allowed hosts", () => {
    expect(allowedImage("https://cdn.modrinth.com/data/x/icon.png")).toBe(true);
    expect(allowedImage("https://raw.githubusercontent.com/a/b/main/c.png")).toBe(true);
    for (const src of [
      "http://cdn.modrinth.com/x.png",
      "https://i.imgur.com/x.png",
      "https://cdn.modrinth.com.evil.example/x.png",
      "data:image/png;base64,AAAA",
      undefined,
    ]) {
      expect(allowedImage(src)).toBe(false);
    }
  });
});
