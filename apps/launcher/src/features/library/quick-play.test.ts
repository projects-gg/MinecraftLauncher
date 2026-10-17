import { describe, expect, it } from "vitest";
import { canQuickPlay, parseServerAddress, serverRowKeys } from "./quick-play";

describe("canQuickPlay", () => {
  it("knows the versions with quick play", () => {
    expect(canQuickPlay("1.20")).toBe(true);
    expect(canQuickPlay("1.21.4")).toBe(true);
    expect(canQuickPlay("26.1.2")).toBe(true);
    expect(canQuickPlay("23w14a")).toBe(true);
    expect(canQuickPlay("24w33a")).toBe(true);
  });

  it("leaves older versions at the title screen", () => {
    expect(canQuickPlay("1.19.4")).toBe(false);
    expect(canQuickPlay("1.8.9")).toBe(false);
    expect(canQuickPlay("23w13a")).toBe(false);
    expect(canQuickPlay("b1.7.3")).toBe(false);
  });
});

describe("parseServerAddress", () => {
  it("reads hosts with and without a port", () => {
    expect(parseServerAddress("play.projects.gg")).toEqual({
      host: "play.projects.gg",
      port: null,
    });
    expect(parseServerAddress(" mc.example.com:25566 ")).toEqual({
      host: "mc.example.com",
      port: 25566,
    });
    expect(parseServerAddress("[2001:db8::1]:25565")).toEqual({ host: "2001:db8::1", port: 25565 });
    expect(parseServerAddress("2001:db8::1")).toEqual({ host: "2001:db8::1", port: null });
    expect(parseServerAddress("sunucu.örnek.com")).toEqual({
      host: "sunucu.örnek.com",
      port: null,
    });
  });

  it("refuses what cannot be an address", () => {
    expect(parseServerAddress("")).toBeNull();
    expect(parseServerAddress("play projects")).toBeNull();
    expect(parseServerAddress("host:99999")).toBeNull();
    expect(parseServerAddress("host:")).toBeNull();
    expect(parseServerAddress("http://play.projects.gg")).toBeNull();
  });
});

describe("serverRowKeys", () => {
  it("gives a server listed twice its own row (CR1-F)", () => {
    const keys = serverRowKeys(["play.projects.gg", "mc.example.com", "Play.Projects.gg "]);
    expect(keys).toEqual(["play.projects.gg", "mc.example.com", "play.projects.gg#2"]);
    expect(new Set(keys).size).toBe(keys.length);
  });
});
