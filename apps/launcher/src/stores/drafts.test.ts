import { describe, expect, it } from "vitest";
import type { InstanceSettings } from "@/lib/api";
import { settingsChanged, settleSettingsDraft, useDrafts } from "./drafts";

const saved: InstanceSettings = {
  javaMode: "auto",
  javaPath: undefined,
  minMemoryMb: 1024,
  maxMemoryMb: 4096,
  memoryAuto: true,
  jvmArgs: "",
  width: 854,
  height: 480,
  fullscreen: false,
  preLaunch: "",
  postExit: "",
  wrapper: "",
  notes: "",
};

describe("settingsChanged", () => {
  it("sees a changed field", () => {
    expect(settingsChanged({ ...saved }, saved)).toBe(false);
    expect(settingsChanged({ ...saved, fullscreen: true }, saved)).toBe(true);
    expect(settingsChanged({ ...saved, notes: "x" }, saved)).toBe(true);
  });

  it("ignores a Java path while Java is automatic", () => {
    expect(settingsChanged({ ...saved, javaPath: "C:\\Java\\bin\\javaw.exe" }, saved)).toBe(false);
    expect(
      settingsChanged(
        { ...saved, javaMode: "custom", javaPath: "C:\\Java\\bin\\javaw.exe" },
        saved,
      ),
    ).toBe(true);
    const custom = { ...saved, javaMode: "custom" as const, javaPath: "C:\\Java\\bin\\javaw.exe" };
    expect(settingsChanged({ ...custom }, custom)).toBe(false);
    expect(settingsChanged({ ...custom, javaPath: "D:\\Other\\bin\\javaw.exe" }, custom)).toBe(
      true,
    );
  });

  it("ignores memory values while memory is automatic", () => {
    // A detour through "Özel" (it copies the automatic value) and back is no change.
    expect(settingsChanged({ ...saved, maxMemoryMb: 6144 }, saved)).toBe(false);
    expect(settingsChanged({ ...saved, memoryAuto: false }, saved)).toBe(true);
    const manual = { ...saved, memoryAuto: false };
    expect(settingsChanged({ ...manual, maxMemoryMb: 6144 }, manual)).toBe(true);
  });
});

describe("settleSettingsDraft", () => {
  it("drops the draft that was saved", () => {
    const sent = { ...saved, notes: "yeni" };
    useDrafts.getState().setSettings("a", sent);
    settleSettingsDraft("a", sent);
    expect(useDrafts.getState().settings.a).toBeUndefined();
  });

  it("keeps what was typed while the save ran", () => {
    // Ctrl+S, then the player keeps typing while the instance is read again (CR1-F).
    const sent = { ...saved, notes: "yeni" };
    useDrafts.getState().setSettings("b", sent);
    const typedOn = { ...sent, notes: "yeni not" };
    useDrafts.getState().setSettings("b", typedOn);
    settleSettingsDraft("b", sent);
    expect(useDrafts.getState().settings.b).toBe(typedOn);
  });
});
