import { afterEach, describe, expect, it, vi } from "vitest";
import { TYPING_SHOW_MS, typingWords, useTyping, writersOf } from "./typing";

const t = (key: string, options?: Record<string, unknown>) =>
  `${key}:${Object.values(options ?? {}).join(",")}`;

describe("typingWords", () => {
  it("names one, two, and two with how many more", () => {
    expect(typingWords([], t)).toBeNull();
    expect(typingWords(["Ayşe"], t)).toBe("chat2.typing.one:Ayşe");
    expect(typingWords(["Ayşe", "Mert"], t)).toBe("chat2.typing.two:Ayşe,Mert");
    expect(typingWords(["Ayşe", "Mert", "Bora", "Can"], t)).toBe("chat2.typing.more:Ayşe,Mert,2");
  });
});

describe("the writers", () => {
  afterEach(() => {
    useTyping.getState().forget();
    vi.useRealTimers();
  });

  it("show for six seconds after their last frame, then go by themselves", () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000_000);
    useTyping.getState().saw("d:100:301", "301");
    expect(writersOf(useTyping.getState().writers, "d:100:301", () => true)).toEqual(["301"]);
    vi.advanceTimersByTime(TYPING_SHOW_MS - 500);
    useTyping.getState().saw("d:100:301", "301");
    vi.advanceTimersByTime(TYPING_SHOW_MS - 500);
    expect(writersOf(useTyping.getState().writers, "d:100:301", () => true)).toEqual(["301"]);
    vi.advanceTimersByTime(600);
    expect(useTyping.getState().writers).toEqual({});
  });

  it("end at once when their message comes, and leave out who is offline", () => {
    useTyping.getState().saw("c:x", "301");
    useTyping.getState().saw("c:x", "302");
    useTyping.getState().stop("c:x", "301");
    expect(writersOf(useTyping.getState().writers, "c:x", () => true)).toEqual(["302"]);
    expect(writersOf(useTyping.getState().writers, "c:x", (id) => id !== "302")).toEqual([]);
  });
});
