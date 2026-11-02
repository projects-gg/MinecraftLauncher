// @vitest-environment happy-dom
// The incoming call card (VOICE-L3, VOICE-INT's finding "GeceKu…"): the caller's name on a line of
// its own, never cut (a 16-character tag whole; a longer name wraps), the verb under it, "Reddet"
// and "Katıl" as labelled buttons below; a screen reader hears who calls and who else is there.
import { TooltipProvider } from "@pl/ui";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { voiceApi } from "@/lib/api/voice-api";
import { HIDDEN_VOICE_STATE, VOICE_OK } from "@/lib/api/voice-types";
import { RingToast } from "./ring-toast";
import { useVoiceUi } from "./store";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useVoiceUi.setState({ state: HIDDEN_VOICE_STATE });
});

const ring = { room: "f00000000000000c3", from: "911", with: ["909"], until: 0 };

function show() {
  useVoiceUi.setState({
    state: {
      ...HIDDEN_VOICE_STATE,
      profiles: [
        { id: "911", name: "KaranlikSovalye_", avatar: "preset:knight" },
        { id: "909", name: "KumFirtinasi", avatar: "preset:wolf" },
      ],
    },
  });
  return render(
    <TooltipProvider>
      <RingToast toastId="ring" ring={ring} />
    </TooltipProvider>,
  );
}

describe("the incoming call card", () => {
  it("gives the caller's name a line of its own that wraps, never cuts", () => {
    const { container } = show();
    const name = container.querySelector("[data-ring-name]");
    expect(name?.textContent).toBe("KaranlikSovalye_");
    expect(name?.className).not.toContain("truncate");
    expect(name?.className).toContain("[overflow-wrap:anywhere]");
  });

  it("says who calls and who else is in the room to a screen reader", () => {
    show();
    expect(screen.getByText("KaranlikSovalye_ arıyor. Odadakiler: KumFirtinasi")).toBeTruthy();
  });

  it("answers with Katıl and declines with Reddet, both labelled", () => {
    const answer = vi.spyOn(voiceApi, "answer").mockResolvedValue(VOICE_OK);
    const decline = vi.spyOn(voiceApi, "decline").mockResolvedValue(VOICE_OK);
    show();
    fireEvent.click(screen.getByRole("button", { name: "Katıl" }));
    fireEvent.click(screen.getByRole("button", { name: "Reddet" }));
    expect(answer).toHaveBeenCalledWith(ring.room);
    expect(decline).toHaveBeenCalledWith(ring.room);
  });
});
