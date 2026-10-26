// @vitest-environment happy-dom
// SOC-LAG: the friends privacy switches turn at the click, follow the launcher's view without a
// flicker when the service answers, turn back when it refuses, and a newer click always wins.
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { useSwitchNow } from "./switch-now";

interface Call {
  next: boolean;
  settle: () => Promise<void>;
}

function harness() {
  const calls: Call[] = [];
  const save = (next: boolean) =>
    new Promise<void>((resolve) => {
      calls.push({
        next,
        settle: async () => {
          await act(async () => {
            resolve();
            await Promise.resolve();
          });
        },
      });
    });
  function Probe({ kept }: { kept: boolean }) {
    const [checked, change] = useSwitchNow(kept, save);
    return (
      <button type="button" role="switch" aria-checked={checked} onClick={() => change(!checked)}>
        Son görülmemi göster
      </button>
    );
  }
  return { calls, Probe };
}

const shown = () => screen.getByRole("switch").getAttribute("aria-checked");

afterEach(() => cleanup());

describe("a friends setting's switch", () => {
  it("turns at the click, before the launcher answers", () => {
    const { calls, Probe } = harness();
    render(<Probe kept={true} />);
    fireEvent.click(screen.getByRole("switch"));
    expect(shown()).toBe("false");
    expect(calls.map((call) => call.next)).toEqual([false]);
  });

  it("follows the view once it shows the change, and the answer does not flicker it", async () => {
    const { calls, Probe } = harness();
    const { rerender } = render(<Probe kept={true} />);
    fireEvent.click(screen.getByRole("switch"));
    // The launcher shows the change itself (its own view), then the service answers.
    rerender(<Probe kept={false} />);
    expect(shown()).toBe("false");
    await calls[0]?.settle();
    expect(shown()).toBe("false");
  });

  it("turns back when the change is refused", async () => {
    const { calls, Probe } = harness();
    render(<Probe kept={false} />);
    fireEvent.click(screen.getByRole("switch"));
    expect(shown()).toBe("true");
    // Refused: the launcher's view never took it (or took it back).
    await calls[0]?.settle();
    expect(shown()).toBe("false");
  });

  it("keeps the newest click when an older one settles first", async () => {
    const { calls, Probe } = harness();
    const { rerender } = render(<Probe kept={false} />);
    fireEvent.click(screen.getByRole("switch"));
    fireEvent.click(screen.getByRole("switch"));
    fireEvent.click(screen.getByRole("switch"));
    expect(calls.map((call) => call.next)).toEqual([true, false, true]);
    expect(shown()).toBe("true");
    // The first answer comes while the view still shows the second click's value.
    rerender(<Probe kept={false} />);
    await calls[0]?.settle();
    expect(shown()).toBe("true");
    rerender(<Probe kept={true} />);
    await calls[1]?.settle();
    await calls[2]?.settle();
    expect(shown()).toBe("true");
  });
});
