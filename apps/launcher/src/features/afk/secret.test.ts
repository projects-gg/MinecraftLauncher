import { describe, expect, it } from "vitest";
import { splitCommand } from "./secret";

describe("a secret line's command in front (AFK-CLEAR)", () => {
  it("takes the command and leaves the rest masked", () => {
    expect(splitCommand("/login hunter2")).toEqual({ command: "/login", rest: "hunter2" });
    // The space typed after the command already moves it in front; the rest is still empty.
    expect(splitCommand("/login ")).toEqual({ command: "/login", rest: "" });
    // Spaces inside the password stay its own.
    expect(splitCommand("/register a b")).toEqual({ command: "/register", rest: "a b" });
  });

  it("leaves a line without a command (or without its space) whole", () => {
    expect(splitCommand("/login")).toBeNull();
    expect(splitCommand("merhaba /login x")).toBeNull();
    expect(splitCommand("")).toBeNull();
  });
});
