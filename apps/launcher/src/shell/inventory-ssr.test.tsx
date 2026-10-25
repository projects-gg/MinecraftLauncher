// INV-UI: the cloud inventory renders on a server (the site prerenders pages that show the
// feature): no window or document while rendering, and `static` is a plain picture.
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  SAMPLE_CHEST,
  SAMPLE_INVENTORY,
} from "../../../../packages/ui/src/components/inventory/fixtures";
import { InventoryView } from "../../../../packages/ui/src/components/inventory/inventory-view";

describe("the inventory on a server", () => {
  it("renders a static picture: named, no stops, no live regions", () => {
    expect(typeof document).toBe("undefined");
    const html = renderToString(
      <InventoryView
        inventory={SAMPLE_INVENTORY}
        window={SAMPLE_CHEST}
        mode="static"
        label="Envanter örneği"
      />,
    );
    expect(html).toContain('aria-label="Envanter örneği"');
    expect(html).toContain("Sandık");
    expect(html).toContain('data-slot="win:0"');
    expect(html).not.toContain("tabindex");
    expect(html).not.toContain('role="status"');
    expect(html).not.toContain("aria-live");
  });

  it("renders the live view too, before any script runs", () => {
    const html = renderToString(
      <InventoryView inventory={SAMPLE_INVENTORY} onOp={async () => ({ ok: true })} lang="en" />,
    );
    expect(html).toContain('aria-label="Hotbar"');
    expect(html).toContain('tabindex="0"');
    expect(html).toContain('role="status"');
  });
});
