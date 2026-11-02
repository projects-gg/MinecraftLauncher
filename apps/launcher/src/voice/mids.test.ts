import { describe, expect, it } from "vitest";
import { MidMap } from "./mids";

describe("MidMap", () => {
  it("knows each receiver's person from the offers that named it", () => {
    const map = new MidMap();
    expect(map.assign({ "1": "u2", "2": "u3" })).toEqual(["1", "2"]);
    expect(map.personOf("1")).toBe("u2");
    expect(map.personOf("3")).toBeUndefined();
    expect(map.current("1")).toBe(true);
    expect(map.current("3")).toBe(false);
  });

  it("plays a person who came back on their newest receiver only", () => {
    const map = new MidMap();
    map.assign({ "1": "u2" });
    map.assign({ "4": "u2" });
    expect(map.current("1")).toBe(false);
    expect(map.current("4")).toBe(true);
  });

  it("gives a reused mid to whoever the SFU names last", () => {
    const map = new MidMap();
    map.assign({ "1": "u2" });
    expect(map.assign({ "1": "u2" })).toEqual([]);
    expect(map.assign({ "1": "u5" })).toEqual(["1"]);
    expect(map.personOf("1")).toBe("u5");
    expect(map.current("1")).toBe(true);
  });

  it("ignores empty names and forgets everything on clear", () => {
    const map = new MidMap();
    map.assign({ "1": "" });
    expect(map.personOf("1")).toBeUndefined();
    map.assign({ "2": "u2" });
    map.clear();
    expect(map.personOf("2")).toBeUndefined();
  });
});
