// BETA-13's smoke (beta.1.14, Settings → Güncellemeler): after a pick of "Beta" the settings panel
// grew a horizontal scrollbar. `Segmented`'s gliding plate (packages/ui glide.ts) places its two
// clipping layers a control's width past the control, and `clip-path` clips only what is drawn:
// the boxes still widened the scroller around it. The window over the row hides its overflow.
import { Segmented } from "@pl/ui";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";

describe("Segmented's gliding plate", () => {
  it("keeps its layers inside the control", () => {
    const html = renderToString(
      <Segmented
        value="stable"
        onValueChange={() => {}}
        label="Güncelleme kanalı"
        options={[
          { value: "stable", label: "Kararlı" },
          { value: "beta", label: "Beta" },
        ]}
      />,
    );
    const glide = /<span[^>]*\bdata-glide=""[^>]*>/.exec(html)?.[0];
    expect(glide).toBeDefined();
    expect(glide).toMatch(/\bclass="[^"]*\boverflow-hidden\b[^"]*"/);
    expect(glide).toMatch(/\bclass="[^"]*\babsolute\b[^"]*"/);
  });
});
