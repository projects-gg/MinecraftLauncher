import { describe, expect, it } from "vitest";
import { ownPhoto } from "./own-photo";

describe("ownPhoto", () => {
  it("keeps a Discord picture of the player's own", () => {
    const own = "https://cdn.discordapp.com/avatars/1234/abcd.png?size=128";
    expect(ownPhoto(own)).toBe(own);
  });

  it("drops Discord's stand-in for an account without a picture", () => {
    expect(ownPhoto("https://cdn.discordapp.com/embed/avatars/2.png")).toBeUndefined();
    expect(ownPhoto("https://cdn.discordapp.com/embed/avatars/0.png?size=64")).toBeUndefined();
  });

  it("has nothing to show without an address", () => {
    expect(ownPhoto(undefined)).toBeUndefined();
    expect(ownPhoto("")).toBeUndefined();
  });
});
