// @vitest-environment happy-dom
// "Yanlışsa bildir" under a refused row (README "Saving" → Refusal reports): one answer in the
// link's place; "the text saves now" lifts the refusal and stays while the row keeps that text.
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { type CloudReportOutcome, cloudApi } from "./api";
import { ReportLine } from "./moderation-bits";

const AD = "Ucuz rank satıyorum, sunucuma gel: oyna.example";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function answering(kind: CloudReportOutcome["kind"]) {
  return vi.spyOn(cloudApi, "reportRefusal").mockResolvedValue({ kind } as CloudReportOutcome);
}

async function report() {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Yanlışsa bildir" }));
  });
}

describe("Yanlışsa bildir", () => {
  it("sends the refused text and says staff will look", async () => {
    const sent = answering("sent");
    const saveable = vi.fn();
    render(
      <ReportLine accountId="a_7KQ2MZ3P9TW4XB1C" refused={AD} current={AD} onSaveable={saveable} />,
    );
    await report();
    expect(sent).toHaveBeenCalledWith("a_7KQ2MZ3P9TW4XB1C", AD);
    expect(screen.getByRole("status").textContent).toBe(
      "Sağ ol, bir yetkili bakacak. Bu arada satırı düzeltip kaydedebilirsin.",
    );
    expect(screen.queryByRole("button", { name: "Yanlışsa bildir" })).toBeNull();
    expect(saveable).not.toHaveBeenCalled();
  });

  it("says the day's limit was reached and this one did not go", async () => {
    answering("limited");
    render(
      <ReportLine accountId="a_7KQ2MZ3P9TW4XB1C" refused={AD} current={AD} onSaveable={vi.fn()} />,
    );
    await report();
    expect(screen.getByRole("status").textContent).toBe(
      "Bugün 3 kez bildirdin, yarın yeniden bildirebilirsin.",
    );
  });

  it("lifts the refusal when the text saves now, and keeps saying so while the text stays", async () => {
    answering("notRefused");
    const saveable = vi.fn();
    const line = (refused: string | null, current: string | null) => (
      <ReportLine
        accountId="a_7KQ2MZ3P9TW4XB1C"
        refused={refused}
        current={current}
        onSaveable={saveable}
      />
    );
    const { rerender } = render(line(AD, AD));
    await report();
    // The settings drop the row's refusal (its mark, the "!", the bar's line), so Kaydet opens.
    expect(saveable).toHaveBeenCalledTimes(1);
    rerender(line(null, AD));
    expect(screen.getByRole("status").textContent).toBe("Bu satır artık kaydedilebilir.");
    expect(screen.queryByRole("button", { name: "Yanlışsa bildir" })).toBeNull();
    // Another text: the answer was about the old one.
    rerender(line(null, "Selam herkese"));
    expect(screen.queryByRole("status")).toBeNull();
  });
});
