import { describe, expect, it } from "vitest";
import { taskbarState } from "./taskbar-model";

describe("taskbarState", () => {
  it("follows the launch first", () => {
    expect(taskbarState({ launch: 41.6, jobs: [90, undefined] })).toEqual({
      status: "normal",
      percent: 42,
    });
    expect(taskbarState({ launch: 140, jobs: [] })).toEqual({ status: "normal", percent: 100 });
  });

  it("stays busy while the game opens, over other jobs", () => {
    expect(taskbarState({ launch: null, opening: true, jobs: [] })).toEqual({
      status: "indeterminate",
    });
    expect(taskbarState({ launch: null, opening: true, jobs: [55] })).toEqual({
      status: "indeterminate",
    });
    expect(taskbarState({ launch: null, opening: false, jobs: [] })).toEqual({ status: "none" });
  });

  it("averages other jobs, indeterminate only when none can tell", () => {
    expect(taskbarState({ launch: null, jobs: [20, 60, undefined] })).toEqual({
      status: "normal",
      percent: 40,
    });
    expect(taskbarState({ launch: null, jobs: [undefined] })).toEqual({
      status: "indeterminate",
    });
    expect(taskbarState({ launch: null, jobs: [] })).toEqual({ status: "none" });
    expect(taskbarState({ launch: Number.NaN, jobs: [] })).toEqual({ status: "none" });
  });
});
