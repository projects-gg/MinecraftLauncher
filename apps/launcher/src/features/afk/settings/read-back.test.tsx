// @vitest-environment happy-dom
// SMALLS-L3: a save's read-back reaches the AFK settings editors after the save settled (React
// Query tells its observers a tick later): the edits give way to the stored settings then, once,
// and an edit made meanwhile stays.
import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useReadBack } from "./read-back";

const BEFORE = { host: "mc.eski.net", port: 0 };
const AFTER = { host: "mc.sunucu.net", port: 25565 };

function editor() {
  const stored = vi.fn();
  const hook = renderHook(({ base }) => useReadBack(base, stored), {
    initialProps: { base: BEFORE },
  });
  return { stored, ...hook };
}

describe("a save's read-back", () => {
  it("brings the stored settings once they come, also after the save settled", () => {
    const { stored, result, rerender } = editor();
    result.current.expect();
    // The save settled and the editor drew again before its read-back came.
    rerender({ base: BEFORE });
    expect(stored).not.toHaveBeenCalled();
    expect(result.current.waiting()).toBe(true);

    rerender({ base: AFTER });
    expect(stored).toHaveBeenCalledOnce();
    expect(stored).toHaveBeenCalledWith(AFTER);
    expect(result.current.waiting()).toBe(false);

    // A later read (a reload) is no save's.
    rerender({ base: { ...AFTER, port: 25570 } });
    expect(stored).toHaveBeenCalledOnce();
  });

  it("leaves an edit made before the read-back came", () => {
    const { stored, result, rerender } = editor();
    result.current.expect();
    result.current.cancel();
    rerender({ base: AFTER });
    expect(stored).not.toHaveBeenCalled();
    expect(result.current.waiting()).toBe(false);
  });

  it("does nothing without a save", () => {
    const { stored, rerender } = editor();
    rerender({ base: AFTER });
    expect(stored).not.toHaveBeenCalled();
  });
});
