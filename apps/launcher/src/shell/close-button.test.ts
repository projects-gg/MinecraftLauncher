import { describe, expect, it } from "vitest";
import { create } from "zustand";
import type { CloseBehavior } from "@/stores/prefs";
import { syncCloseChoice, trayHintKey } from "./close-button";

function prefsStore(closeBehavior: CloseBehavior) {
  return create<{ closeBehavior: CloseBehavior; sounds: boolean }>()(() => ({
    closeBehavior,
    sounds: true,
  }));
}

describe("the close choice", () => {
  it("reaches the shell at start and after each change", () => {
    const store = prefsStore("quit");
    const sent: CloseBehavior[] = [];
    const stop = syncCloseChoice(store, (choice) => sent.push(choice));
    expect(sent).toEqual(["quit"]);
    store.setState({ closeBehavior: "tray" });
    store.setState({ closeBehavior: "quit" });
    expect(sent).toEqual(["quit", "tray", "quit"]);
    stop();
    store.setState({ closeBehavior: "tray" });
    expect(sent).toEqual(["quit", "tray", "quit"]);
  });

  it("is not sent again for other settings", () => {
    const store = prefsStore("tray");
    const sent: CloseBehavior[] = [];
    syncCloseChoice(store, (choice) => sent.push(choice));
    store.setState({ sounds: false });
    store.setState({ closeBehavior: "tray" });
    expect(sent).toEqual(["tray"]);
  });
});

describe("the tray notice", () => {
  it("explains the tray once, and the AFK accounts when they keep the launcher", () => {
    expect(trayHintKey({ hint: "tray" }, false)).toBe("tray.hint");
    expect(trayHintKey({ hint: "afk" }, false)).toBe("tray.hintAfk");
    expect(trayHintKey({ hint: "tray" }, true)).toBeNull();
    expect(trayHintKey({ hint: "afk" }, true)).toBeNull();
  });

  it("stays quiet when the launcher quits after its jobs or on macOS", () => {
    expect(trayHintKey({ hint: null }, false)).toBeNull();
  });
});
