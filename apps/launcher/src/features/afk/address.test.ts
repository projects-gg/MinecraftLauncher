// SHEET-UX: the server's address and port in one field, as Minecraft's server list writes them.
import { describe, expect, it } from "vitest";
import { joinAddress, splitAddress } from "./address";

describe("joinAddress", () => {
  it("shows the port only when the server needs one", () => {
    expect(joinAddress("play.projects.gg", 0)).toBe("play.projects.gg");
    expect(joinAddress("play.projects.gg", "")).toBe("play.projects.gg");
    expect(joinAddress("mc.madenciler.net", 25570)).toBe("mc.madenciler.net:25570");
    expect(joinAddress("mc.madenciler.net", "25570")).toBe("mc.madenciler.net:25570");
  });

  it("leaves an empty host empty", () => {
    expect(joinAddress("", 0)).toBe("");
  });
});

describe("splitAddress", () => {
  it("reads a host alone", () => {
    expect(splitAddress(" play.projects.gg ")).toEqual({ host: "play.projects.gg", port: "" });
  });

  it("takes the digits after the colon as the port", () => {
    expect(splitAddress("mc.madenciler.net:25570")).toEqual({
      host: "mc.madenciler.net",
      port: "25570",
    });
  });

  it("keeps a colon still being typed out of the host", () => {
    expect(splitAddress("mc.madenciler.net:")).toEqual({ host: "mc.madenciler.net", port: "" });
  });

  it("leaves words after the colon to the host's own check", () => {
    expect(splitAddress("mc.madenciler.net:abc")).toEqual({
      host: "mc.madenciler.net:abc",
      port: "",
    });
  });

  it("hands an over-long port to the port's own check", () => {
    expect(splitAddress("a.net:999999")).toEqual({ host: "a.net", port: "999999" });
    expect(splitAddress("a.net:9999999").port).toBe("");
  });

  it("finds no port in a bare IPv6 address", () => {
    expect(splitAddress("2001:db8::1")).toEqual({ host: "2001:db8::1", port: "" });
  });

  it("goes back and forth with joinAddress", () => {
    for (const [host, port] of [
      ["play.projects.gg", ""],
      ["mc.madenciler.net", "25570"],
    ] as const) {
      expect(splitAddress(joinAddress(host, port))).toEqual({ host, port });
    }
  });
});
