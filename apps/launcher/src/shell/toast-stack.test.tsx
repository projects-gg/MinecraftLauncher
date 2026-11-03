// @vitest-environment happy-dom
// TOAST-STACK (BATCH-206 P3): every card in the bottom-right corner is a toast of one list, laid
// out one above the other (newest at the foot), never folded under the front one. Folded, an
// unstyled toast behind a short notice kept its rows and buttons visible under the notice.
import { Toaster, toast } from "@pl/ui";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

afterEach(() => {
  act(() => {
    toast.dismiss();
  });
  cleanup();
});

const frame = () => new Promise((resolve) => setTimeout(resolve, 50));

describe("toast stack", () => {
  it("lays a waiting card out above a newer notice", async () => {
    const view = render(<Toaster />);
    act(() => {
      toast.custom(() => <p>Discord'daki tuşların sesli sohbete kopyalansın mı?</p>, {
        id: "card",
        duration: Number.POSITIVE_INFINITY,
      });
    });
    await act(frame);
    act(() => {
      toast("Sesli sohbet kullanman 21:56'ya kadar kısıtlandı.", { id: "notice" });
    });
    await act(frame);
    const toasts = [...view.baseElement.querySelectorAll<HTMLElement>("[data-sonner-toast]")];
    expect(toasts).toHaveLength(2);
    for (const el of toasts) expect(el.dataset.expanded).toBe("true");
    const front = toasts.find((el) => el.dataset.front === "true");
    const behind = toasts.find((el) => el.dataset.front === "false");
    expect(front?.textContent).toContain("kısıtlandı");
    expect(behind?.textContent).toContain("Discord");
    // Lifted by the newer toast's height plus the 10 px gap (happy-dom measures 0 px heights).
    expect(
      Number.parseFloat(behind?.style.getPropertyValue("--offset") ?? "0"),
    ).toBeGreaterThanOrEqual(10);
  });
});
