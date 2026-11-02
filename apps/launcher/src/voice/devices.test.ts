import { describe, expect, it } from "vitest";
import { chooseDevice, clipUtf8, listDevices } from "./devices";

const device = (kind: MediaDeviceKind, deviceId: string, label: string) =>
  ({ kind, deviceId, label, groupId: "g", toJSON: () => ({}) }) as MediaDeviceInfo;

describe("listDevices", () => {
  it("offers real devices once each, without Windows' default and communications entries", () => {
    const devices = listDevices([
      device("audioinput", "default", "Varsayılan - Mikrofon (USB)"),
      device("audioinput", "communications", "İletişim - Mikrofon (USB)"),
      device("audioinput", "a1", "Mikrofon (USB)"),
      device("audioinput", "a2", "  Mikrofon Dizisi (Akıllı Ses)  "),
      device("audioinput", "a1", "Mikrofon (USB)"),
      device("audiooutput", "default", "Varsayılan - Hoparlör"),
      device("audiooutput", "o1", "Kulaklık"),
      device("videoinput", "v1", "Kamera"),
    ]);
    expect(devices).toEqual({
      inputs: [
        { id: "a1", label: "Mikrofon (USB)" },
        { id: "a2", label: "Mikrofon Dizisi (Akıllı Ses)" },
      ],
      outputs: [{ id: "o1", label: "Kulaklık" }],
    });
  });

  it("stays within what the launcher accepts (32 a kind, 200 bytes a name)", () => {
    const many = Array.from({ length: 40 }, (_, i) =>
      device("audioinput", `in${i}`, "ş".repeat(150)),
    );
    const devices = listDevices(many);
    expect(devices.inputs).toHaveLength(32);
    expect(new TextEncoder().encode(devices.inputs[0]?.label).length).toBeLessThanOrEqual(200);
    expect(listDevices([device("audioinput", "x".repeat(300), "Uzun")]).inputs).toEqual([]);
  });
});

describe("clipUtf8", () => {
  it("cuts at whole characters", () => {
    expect(clipUtf8("abc", 2)).toBe("ab");
    expect(clipUtf8("ışık", 5)).toBe("ış");
    expect(clipUtf8("a😀b", 4)).toBe("a");
    expect(clipUtf8("a😀b", 5)).toBe("a😀");
  });
});

describe("chooseDevice", () => {
  const inputs = [{ id: "a1", label: "USB" }];
  it("opens the chosen device while it is there, else the default", () => {
    expect(chooseDevice(null, inputs)).toEqual({ id: undefined, missing: false });
    expect(chooseDevice("a1", inputs)).toEqual({ id: "a1", missing: false });
    expect(chooseDevice("gone", inputs)).toEqual({ id: undefined, missing: true });
  });
});
