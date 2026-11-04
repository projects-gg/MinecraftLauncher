import { describe, expect, it } from "vitest";
import { fromErrorEvent, fromRejection, PAGE_ERRORS_PER_LOAD, reportLimiter } from "./page-errors";

describe("page errors for usage data", () => {
  it("takes an uncaught error with its place in our scripts", () => {
    const error = new TypeError("Cannot read properties of undefined (reading 'length')");
    expect(
      fromErrorEvent({
        error,
        message: "Uncaught TypeError",
        filename: "http://tauri.localhost/assets/index-B9x2.js",
        lineno: 1,
        colno: 2345,
      }),
    ).toEqual({
      message: "TypeError: Cannot read properties of undefined (reading 'length')",
      source: "http://tauri.localhost/assets/index-B9x2.js",
      line: 1,
      column: 2345,
    });
  });

  it("leaves out what is expected", () => {
    expect(
      fromErrorEvent({ message: "ResizeObserver loop completed with undelivered notifications." }),
    ).toBeNull();
    expect(fromErrorEvent({ message: "Script error." })).toBeNull();
    const abort = new DOMException("Transition was skipped", "AbortError");
    expect(fromRejection(abort)).toBeNull();
    // A command's refusal is the page's to show.
    expect(fromRejection({ code: "AG", message: "No network", details: "x" })).toBeNull();
    expect(fromErrorEvent({ message: "" })).toBeNull();
  });

  it("finds a rejection's first frame in our scripts", () => {
    const error = new Error("boom");
    error.stack = "Error: boom\n    at load (tauri://localhost/assets/index-C1.js:3:77)\n    at x";
    expect(fromRejection(error)).toEqual({
      message: "Error: boom",
      source: "tauri://localhost/assets/index-C1.js",
      line: 3,
      column: 77,
    });
    expect(fromRejection("plain text")).toEqual({
      message: "plain text",
      source: "",
      line: 0,
      column: 0,
    });
  });

  it("sends each text once and only a few per page load", () => {
    const allow = reportLimiter();
    const error = (message: string) => ({ message, source: "", line: 0, column: 0 });
    expect(allow(error("a"))).toBe(true);
    expect(allow(error("a"))).toBe(false);
    for (let i = 1; i < PAGE_ERRORS_PER_LOAD; i += 1) expect(allow(error(`e${i}`))).toBe(true);
    expect(allow(error("one too many"))).toBe(false);
  });
});
