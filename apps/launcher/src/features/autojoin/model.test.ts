import { describe, expect, it } from "vitest";
import type { SubServerView } from "@/bindings";
import { currentRoute, routeLabel, routeRows } from "./model";

const subs: SubServerView[] = [
  { id: "skyblock", name: "SkyBlock", testersOnly: false },
  { id: "smp", name: "SMP", testersOnly: false },
  { id: "testserver", name: "TestServer", testersOnly: true },
];

describe("the address picker (AUTOJOIN)", () => {
  it("offers the lobby first, then the sub-servers as the owner wrote them", () => {
    expect(routeLabel("play.projects.gg", "SkyBlock")).toBe("play.projects.gg → SkyBlock");
    const rows = routeRows("play.projects.gg", subs, false);
    expect(rows.map((row) => row.label)).toEqual([
      "play.projects.gg",
      "play.projects.gg → SkyBlock",
      "play.projects.gg → SMP",
    ]);
    expect(rows.map((row) => row.short)).toEqual([null, "SkyBlock", "SMP"]);
    expect(rows[0]?.id).toBeNull();
  });

  it("shows testers' rows only to the beta program", () => {
    expect(routeRows("play.projects.gg", subs, true).map((row) => row.id)).toEqual([
      null,
      "skyblock",
      "smp",
      "testserver",
    ]);
    // Only a testers' row and no tester: nothing to pick, the bar shows the address as before.
    const onlyTesters = subs.filter((sub) => sub.testersOnly);
    expect(routeRows("play.projects.gg", onlyTesters, false)).toEqual([]);
    expect(routeRows(undefined, subs, true)).toEqual([]);
    expect(routeRows("play.projects.gg", [], true)).toEqual([]);
  });

  it("keeps the saved pick while it is offered, else the lobby", () => {
    const rows = routeRows("play.projects.gg", subs, false);
    expect(currentRoute("smp", rows)?.label).toBe("play.projects.gg → SMP");
    expect(currentRoute(undefined, rows)?.id).toBeNull();
    // Studio took it out, or it is a testers' row this player does not see.
    expect(currentRoute("hg", rows)?.id).toBeNull();
    expect(currentRoute("testserver", rows)?.id).toBeNull();
    expect(currentRoute("smp", [])).toBeNull();
  });
});
