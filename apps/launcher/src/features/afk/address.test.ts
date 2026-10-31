// SHEET-UX: the server's address and port in one field, as Minecraft's server list writes them.
import { describe, expect, it } from "vitest";
import { joinAddress, shownAddress, splitAddress } from "./address";

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
    expect(splitAddress("a.net:9999999")).toEqual({ host: "a.net", port: "9999999" });
  });

  it("takes the address out of a pasted link", () => {
    expect(splitAddress("https://mc.sunucu.net:25565")).toEqual({
      host: "mc.sunucu.net",
      port: "25565",
    });
    expect(splitAddress(" http://play.projects.gg/ ")).toEqual({
      host: "play.projects.gg",
      port: "",
    });
    expect(splitAddress("minecraft://mc.sunucu.net:25570/join?x=1")).toEqual({
      host: "mc.sunucu.net",
      port: "25570",
    });
    expect(splitAddress("mc.sunucu.net:25570/")).toEqual({ host: "mc.sunucu.net", port: "25570" });
  });

  it("leaves a link without an address empty for the host's own check", () => {
    expect(splitAddress("https://")).toEqual({ host: "", port: "" });
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

describe("shownAddress", () => {
  it("keeps what is typed while it reads as the same address", () => {
    expect(shownAddress("mc.sunucu.net:", "mc.sunucu.net", 0)).toBe("mc.sunucu.net:");
    expect(shownAddress("https://mc.sunucu.net:25565", "mc.sunucu.net", 25565)).toBe(
      "https://mc.sunucu.net:25565",
    );
    expect(shownAddress("http://play.projects.gg/", "play.projects.gg", "")).toBe(
      "http://play.projects.gg/",
    );
  });

  it("shows another address that came from outside", () => {
    expect(shownAddress("mc.eski.net", "mc.yeni.net", 25570)).toBe("mc.yeni.net:25570");
    expect(shownAddress("mc.eski.net:25570", "mc.eski.net", 0)).toBe("mc.eski.net");
  });

  it("writes a saved link back as the address that was kept", () => {
    expect(shownAddress("https://mc.sunucu.net:25565", "mc.sunucu.net", 25565, true)).toBe(
      "mc.sunucu.net:25565",
    );
    // The 7/24 Bulut draft keeps the port as text.
    expect(
      shownAddress("minecraft://mc.sunucu.net:25570/join", "mc.sunucu.net", "25570", true),
    ).toBe("mc.sunucu.net:25570");
  });

  it("writes the port back only when one was given", () => {
    expect(shownAddress(" http://play.projects.gg/ ", "play.projects.gg", 0, true)).toBe(
      "play.projects.gg",
    );
    expect(shownAddress("https://mc.sunucu.net", "mc.sunucu.net", "", true)).toBe("mc.sunucu.net");
    expect(shownAddress("mc.sunucu.net:", "mc.sunucu.net", 0, true)).toBe("mc.sunucu.net");
  });

  it("leaves a clean address as it is when saved", () => {
    expect(shownAddress("mc.sunucu.net:25565", "mc.sunucu.net", 25565, true)).toBe(
      "mc.sunucu.net:25565",
    );
    expect(shownAddress("", "", 0, true)).toBe("");
  });
});
