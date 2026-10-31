import { describe, expect, it } from "vitest";
import type { LaunchOptions, ModCheck } from "@/lib/api";
import { retryOptions, useModFlow } from "./flow";

const check: ModCheck = {
  instanceId: "pack",
  minecraft: "26.1.2",
  loader: "fabric",
  issues: [],
  blocking: 0,
  checkedFiles: 0,
  repairable: false,
  hasMods: true,
};
const world: LaunchOptions = { quickPlay: { kind: "singleplayer", world: "Ada" } };

describe("retryOptions", () => {
  it("starts the stopped launch again, to its world or server", () => {
    expect(retryOptions(world, false)).toEqual(world);
    expect(retryOptions(world, true)).toEqual({ ...world, ignoreModIssues: true });
  });

  it("is a plain OYNA when the flow came from no launch", () => {
    expect(retryOptions(null, false)).toBeUndefined();
    expect(retryOptions(null, true)).toEqual({ ignoreModIssues: true });
  });
});

describe("useModFlow", () => {
  it("keeps the options of the launch it stopped until the next flow opens", () => {
    useModFlow.getState().open("pack", "blocked", { kind: "keys", check }, world);
    expect(useModFlow.getState().options).toEqual(world);
    useModFlow.getState().close();
    useModFlow.getState().open("pack", "crash", { kind: "crash" });
    expect(useModFlow.getState().options).toBeNull();
  });
});
