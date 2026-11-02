import { describe, expect, it } from "vitest";
import { SerialQueue } from "./queue";

describe("SerialQueue", () => {
  it("runs steps one at a time in order, a failure not stopping the next", async () => {
    const queue = new SerialQueue();
    const log: string[] = [];
    let release: () => void = () => undefined;
    const slow = queue.run(async () => {
      log.push("join start");
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      log.push("join end");
    });
    const failed = queue.run(async () => {
      log.push("answer");
      throw new Error("bad sdp");
    });
    const after = queue.run(async () => {
      log.push("offer");
      return 7;
    });
    await Promise.resolve();
    expect(log).toEqual(["join start"]);
    release();
    await slow;
    await expect(failed).rejects.toThrow("bad sdp");
    expect(await after).toBe(7);
    await queue.idle();
    expect(log).toEqual(["join start", "join end", "answer", "offer"]);
  });
});
