// SHEET-UX: an AFK account's unsaved settings wait while its panel is closed, and come back over
// whatever was saved meanwhile without undoing it.
import { afterEach, describe, expect, it } from "vitest";
import { draftKey, rebaseDraft, useAfkDrafts } from "./drafts";

afterEach(() => useAfkDrafts.setState({ held: {} }));

describe("useAfkDrafts", () => {
  it("holds one account's edits under its own kind", () => {
    const { hold } = useAfkDrafts.getState();
    hold(draftKey("desktop", "a1"), { draft: { x: 2 }, base: { x: 1 } });
    expect(useAfkDrafts.getState().held["desktop:a1"]).toEqual({ draft: { x: 2 }, base: { x: 1 } });
    expect(useAfkDrafts.getState().held["cloud:a1"]).toBeUndefined();
  });

  it("drops them, and dropping nothing changes nothing", () => {
    const { hold } = useAfkDrafts.getState();
    hold("cloud:c1", { draft: 1, base: 0 });
    hold("cloud:c1", null);
    expect(useAfkDrafts.getState().held).toEqual({});
    const before = useAfkDrafts.getState();
    hold("cloud:c1", null);
    expect(useAfkDrafts.getState()).toBe(before);
  });
});

describe("rebaseDraft", () => {
  const base = { name: "Bot", server: { host: "a.net", port: 0 }, rules: [{ p: "selam" }] };

  it("keeps the draft as it is while nothing was saved meanwhile", () => {
    const draft = { ...base, name: "Bot2" };
    expect(rebaseDraft(draft, base, structuredClone(base))).toBe(draft);
  });

  it("keeps the player's edits and takes the rest from what was saved", () => {
    const draft = { ...base, name: "Bot2" };
    const current = { ...base, server: { host: "b.net", port: 25570 } };
    expect(rebaseDraft(draft, base, current)).toEqual({
      name: "Bot2",
      server: { host: "b.net", port: 25570 },
      rules: [{ p: "selam" }],
    });
  });

  it("lets an edited field win over a saved change to the same field", () => {
    const draft = { ...base, server: { host: "c.net", port: 0 } };
    const current = { ...base, server: { host: "b.net", port: 25570 } };
    expect(rebaseDraft(draft, base, current).server).toEqual({ host: "c.net", port: 25570 });
  });

  it("takes a list whole: an edited list stays, an untouched one follows the save", () => {
    const edited = { ...base, rules: [{ p: "selam" }, { p: "naber" }] };
    const current = { ...base, rules: [{ p: "merhaba" }] };
    expect(rebaseDraft(edited, base, current).rules).toEqual([{ p: "selam" }, { p: "naber" }]);
    const untouched = { ...base, name: "Bot2" };
    expect(rebaseDraft(untouched, base, current).rules).toEqual([{ p: "merhaba" }]);
  });

  it("keeps fields the save added", () => {
    const current = { ...base, movement: true } as typeof base & { movement: boolean };
    const draft = { ...base, name: "Bot2" } as typeof current;
    expect(rebaseDraft(draft, base as typeof current, current)).toMatchObject({
      name: "Bot2",
      movement: true,
    });
  });
});
