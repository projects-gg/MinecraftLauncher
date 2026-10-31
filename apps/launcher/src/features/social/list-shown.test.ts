import { describe, expect, it } from "vitest";
import { friendListShown } from "./store";

const chatOn = { chat: {} };
const chatOff = {};

describe("friendListShown", () => {
  it("is the open list page, on Arkadaşlar when chat is on", () => {
    expect(friendListShown({ open: true, page: "list", listTab: "friends", view: chatOn })).toBe(
      true,
    );
    expect(friendListShown({ open: true, page: "list", listTab: "friends", view: chatOff })).toBe(
      true,
    );
  });

  it("is not the Mesajlar tab: requests, invites and calls are toasted then", () => {
    expect(friendListShown({ open: true, page: "list", listTab: "messages", view: chatOn })).toBe(
      false,
    );
  });

  it("is not another page or a closed panel", () => {
    expect(friendListShown({ open: true, page: "chat", listTab: "friends", view: chatOn })).toBe(
      false,
    );
    expect(friendListShown({ open: false, page: "list", listTab: "friends", view: chatOn })).toBe(
      false,
    );
  });

  it("ignores the tab when chat is off (the list has no tabs then)", () => {
    expect(friendListShown({ open: true, page: "list", listTab: "messages", view: chatOff })).toBe(
      true,
    );
  });
});
