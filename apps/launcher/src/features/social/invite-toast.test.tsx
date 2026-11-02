// @vitest-environment happy-dom
// BETA2-FIX (REL-BETA203 observation 4): a "Birlikte oyna" toast already on screen says its sentence
// again in the language the player switches to (the toast is a component; only its system
// notification and the screen reader's announcement are words made once).
import { Toaster, TooltipProvider } from "@pl/ui";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { setLanguage } from "@/lib/i18n";
import { showInviteToast } from "./invite-toast";

afterEach(async () => {
  cleanup();
  await setLanguage("tr");
});

const frame = () => new Promise((resolve) => setTimeout(resolve, 50));

describe("invite toast", () => {
  it("follows a language switch while it is shown", async () => {
    const view = render(
      <TooltipProvider>
        <Toaster />
      </TooltipProvider>,
    );
    act(() => {
      showInviteToast({ id: "i1", from: "f1", name: "BuzKalesi", at: 1, quiet: false });
    });
    await act(frame);
    expect(view.baseElement.textContent).toContain("BuzKalesi seni oyuna çağırıyor");
    await act(async () => {
      await setLanguage("en");
    });
    await act(frame);
    expect(view.baseElement.textContent).toContain("BuzKalesi invites you to play");
    expect(view.baseElement.textContent).not.toContain("çağırıyor");
  });
});
