// @vitest-environment happy-dom
// SQOL-U3: a conversation shown outside the friends panel (the room panel shows a call's friend's
// messages) counts as on screen for its notices, and its keys answer only from inside its panel;
// the friends panel keeps its own ways.
import { afterEach, describe, expect, it } from "vitest";
import { conversationShown, useSocialUi } from "../store";
import { chatKeyHere } from "./focus";

const CONV = "d:100:302";

afterEach(() => {
  useSocialUi.setState({ shownElsewhere: {}, open: false, page: "list", chatConv: null });
  document.body.innerHTML = "";
});

describe("conversationShown", () => {
  it("counts a conversation shown in another place, as long as one place shows it", () => {
    const ui = () => useSocialUi.getState();
    expect(conversationShown(ui(), CONV)).toBe(false);
    ui().showElsewhere(CONV, true);
    ui().showElsewhere(CONV, true);
    expect(conversationShown(ui(), CONV)).toBe(true);
    ui().showElsewhere(CONV, false);
    expect(conversationShown(ui(), CONV)).toBe(true);
    ui().showElsewhere(CONV, false);
    expect(conversationShown(ui(), CONV)).toBe(false);
    expect(ui().shownElsewhere).toEqual({});
  });

  it("still counts the friends panel's chat page", () => {
    expect(
      conversationShown({ open: true, page: "chat", chatConv: CONV, shownElsewhere: {} }, CONV),
    ).toBe(true);
    expect(conversationShown({ open: false, page: "chat", chatConv: CONV }, CONV)).toBe(false);
  });
});

function build(host: "dialog" | "chat-host") {
  const panel = document.createElement("aside");
  if (host === "dialog") panel.setAttribute("role", "dialog");
  else panel.setAttribute("data-chat-host", "");
  const box = document.createElement("div");
  const field = document.createElement("textarea");
  box.append(field);
  const header = document.createElement("button");
  panel.append(header, box);
  const outside = document.createElement("button");
  document.body.append(panel, outside);
  return { box, field, header, outside };
}

describe("chatKeyHere", () => {
  it("answers in the room panel only from inside it", () => {
    const { box, field, header, outside } = build("chat-host");
    expect(chatKeyHere(box, field)).toBe(true);
    expect(chatKeyHere(box, header)).toBe(true);
    expect(chatKeyHere(box, outside)).toBe(false);
    expect(chatKeyHere(box, document.body)).toBe(false);
  });

  it("answers in the friends panel from inside it and from the page's body, as before", () => {
    const { box, field, header, outside } = build("dialog");
    expect(chatKeyHere(box, field)).toBe(true);
    expect(chatKeyHere(box, header)).toBe(true);
    expect(chatKeyHere(box, document.body)).toBe(true);
    expect(chatKeyHere(box, outside)).toBe(false);
    expect(chatKeyHere(box, null)).toBe(false);
  });
});
