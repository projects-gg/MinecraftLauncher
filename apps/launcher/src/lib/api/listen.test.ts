import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Callback = (event: { event: string; id: number; payload: unknown }) => void;

const tauri = vi.hoisted(() => ({
  callbacks: new Map<string, Callback>(),
  listens: 0,
  unlistens: 0,
  refuse: false,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: Callback) => {
    tauri.listens += 1;
    if (tauri.refuse) throw new Error("refused");
    tauri.callbacks.set(event, callback);
    return () => {
      tauri.unlistens += 1;
      tauri.callbacks.delete(event);
    };
  }),
}));

import { LISTEN_LINGER_MS, listen } from "./listen";

let counter = 0;
/** A fresh event name per test: the module keeps its listeners between tests. */
const name = () => `test-event-${++counter}`;
const send = (event: string, payload: unknown) =>
  tauri.callbacks.get(event)?.({ event, id: 1, payload });

beforeEach(() => {
  vi.useFakeTimers();
  tauri.listens = 0;
  tauri.unlistens = 0;
  tauri.refuse = false;
});

afterEach(() => {
  vi.useRealTimers();
});

describe("listen", () => {
  it("shares one Tauri listener between subscribers", async () => {
    const event = name();
    const a = vi.fn();
    const b = vi.fn();
    const stopA = await listen<number>(event, (e) => a(e.payload));
    const stopB = await listen<number>(event, (e) => b(e.payload));
    expect(tauri.listens).toBe(1);
    send(event, 7);
    expect(a).toHaveBeenCalledWith(7);
    expect(b).toHaveBeenCalledWith(7);
    stopA();
    send(event, 8);
    expect(a).toHaveBeenCalledTimes(1);
    expect(b).toHaveBeenLastCalledWith(8);
    stopB();
  });

  it("keeps the Tauri listener a moment after the last subscriber, for payloads on their way", async () => {
    const event = name();
    const a = vi.fn();
    const stop = await listen(event, a);
    stop();
    stop();
    // A payload that was on its way lands in a live callback and reaches no one.
    send(event, 1);
    expect(a).not.toHaveBeenCalled();
    expect(tauri.unlistens).toBe(0);
    vi.advanceTimersByTime(LISTEN_LINGER_MS);
    await vi.runAllTimersAsync();
    expect(tauri.unlistens).toBe(1);
    // Listening again asks Tauri again.
    const again = await listen(event, a);
    expect(tauri.listens).toBe(2);
    send(event, 2);
    expect(a).toHaveBeenCalledTimes(1);
    again();
  });

  it("takes the listener up again when a subscriber comes back within the while", async () => {
    const event = name();
    const a = vi.fn();
    const stop = await listen(event, a);
    stop();
    vi.advanceTimersByTime(LISTEN_LINGER_MS - 1);
    const back = await listen(event, a);
    vi.advanceTimersByTime(LISTEN_LINGER_MS * 2);
    expect(tauri.listens).toBe(1);
    expect(tauri.unlistens).toBe(0);
    send(event, 3);
    expect(a).toHaveBeenCalledTimes(1);
    back();
  });

  it("keeps the others going when one subscriber throws", async () => {
    const event = name();
    const thrown: unknown[] = [];
    const spy = vi.spyOn(globalThis, "queueMicrotask").mockImplementation((task) => {
      try {
        task();
      } catch (error) {
        thrown.push(error);
      }
    });
    const b = vi.fn();
    const stopA = await listen(event, () => {
      throw new Error("boom");
    });
    const stopB = await listen(event, b);
    send(event, 4);
    expect(b).toHaveBeenCalledTimes(1);
    expect(thrown).toHaveLength(1);
    spy.mockRestore();
    stopA();
    stopB();
  });

  it("rejects like Tauri when the listener is refused, and asks again next time", async () => {
    const event = name();
    tauri.refuse = true;
    await expect(listen(event, vi.fn())).rejects.toThrow("refused");
    tauri.refuse = false;
    const stop = await listen(event, vi.fn());
    expect(tauri.listens).toBe(2);
    stop();
  });
});
