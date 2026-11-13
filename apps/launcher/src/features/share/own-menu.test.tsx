// The player's own share's menu (SQOL, sqol.md §1.2): the same rows in the contract's order, a
// row that cannot be chosen dim with one sentence why, each row asking for its own change, and a
// service without live changes leaving "Paylaşımı Durdur" alone.
import type { MenuItem } from "@pl/ui";
import { describe, expect, it, vi } from "vitest";
import i18n from "@/lib/i18n";
import { DEFAULT_SHARE_SETTINGS, type VoiceMyShare } from "@/share/types";
import { type OwnMenuActions, type OwnMenuInput, ownShareItems } from "./own-menu";

const t = i18n.t.bind(i18n);

const share = (patch: Partial<VoiceMyShare> = {}): VoiceMyShare => ({
  n: 1,
  kind: "game",
  label: "",
  phase: "live",
  audio: true,
  preset: "normal",
  fps: 30,
  watchers: ["301"],
  since: 0,
  st: "live",
  hold: null,
  ...patch,
});

const input = (
  patch: Partial<OwnMenuInput> = {},
  mine: Partial<VoiceMyShare> = {},
): OwnMenuInput => ({
  mine: share(mine),
  status: "live",
  edit: true,
  maxPreset: "high",
  hwMax: "high",
  canAudio: true,
  settings: DEFAULT_SHARE_SETTINGS,
  ...patch,
});

const actions = () => ({
  switchSource: vi.fn<OwnMenuActions["switchSource"]>(),
  change: vi.fn<OwnMenuActions["change"]>(),
  hold: vi.fn<OwnMenuActions["hold"]>(),
  stop: vi.fn<OwnMenuActions["stop"]>(),
});

const labels = (items: Array<MenuItem | null>) => items.map((item) => (item ? item.label : "—"));
const row = (items: Array<MenuItem | null>, label: string): MenuItem => {
  const found = items.find((item) => item?.label === label);
  if (!found) throw new Error(`no row ${label}: ${labels(items).join(", ")}`);
  return found;
};
const sub = (items: Array<MenuItem | null>, label: string) => row(items, label).items ?? [];

describe("the own share's menu", () => {
  it("lists its rows in the contract's order", () => {
    const items = ownShareItems(input(), t, actions());
    expect(labels(items)).toEqual([
      "Ekranı Değiştir…",
      "Kalite",
      "Sesi de Paylaş",
      "Duraklat",
      "Gelişmiş",
      "—",
      "Paylaşımı Durdur",
    ]);
    const quality = sub(items, "Kalite");
    expect(labels(quality)).toEqual(["Düşük · 480p", "Normal · 720p", "Yüksek · 1080p"]);
    expect(quality.map((item) => item?.checked)).toEqual([false, true, false]);
    expect(quality.every((item) => item?.radio)).toBe(true);
    const advanced = sub(items, "Gelişmiş");
    expect(labels(advanced)).toEqual([
      "Kare Hızı",
      "Görüntü",
      "İmleci Göster",
      "Önizleme Resmini Paylaş",
    ]);
    expect(labels(sub(advanced, "Kare Hızı"))).toEqual(["15 fps", "30 fps", "60 fps"]);
    expect(labels(sub(advanced, "Görüntü"))).toEqual(["Otomatik", "Netlik", "Akıcılık"]);
    expect(row(items, "Sesi de Paylaş").checked).toBe(true);
  });

  it("keeps only the stop row where a live share cannot change", () => {
    expect(labels(ownShareItems(input({ edit: false }), t, actions()))).toEqual([
      "Paylaşımı Durdur",
    ]);
    expect(labels(ownShareItems(input({ status: "starting" }), t, actions()))).toEqual([
      "Paylaşımı Durdur",
    ]);
  });

  it("dims what cannot be chosen now, with one sentence why", () => {
    const capped = sub(ownShareItems(input({ maxPreset: "normal" }), t, actions()), "Kalite");
    expect(row(capped, "Yüksek · 1080p")).toMatchObject({
      disabled: true,
      hint: "Bu kalite şu an kapalı.",
    });
    expect(row(capped, "Normal · 720p").disabled).toBe(false);
    const computer = sub(ownShareItems(input({ hwMax: "normal" }), t, actions()), "Kalite");
    expect(row(computer, "Yüksek · 1080p")).toMatchObject({
      disabled: true,
      hint: "Bu bilgisayarda en çok Normal.",
    });
    // Both: this computer's reason, which no limit lifts.
    const both = sub(
      ownShareItems(input({ hwMax: "normal", maxPreset: "normal" }), t, actions()),
      "Kalite",
    );
    expect(row(both, "Yüksek · 1080p").hint).toBe("Bu bilgisayarda en çok Normal.");
    const quiet = ownShareItems(input({ canAudio: false }), t, actions());
    expect(row(quiet, "Sesi de Paylaş")).toMatchObject({
      disabled: true,
      checked: false,
      hint: "Bu bilgisayarda ses paylaşılamıyor.",
    });
    const normal = sub(sub(ownShareItems(input(), t, actions()), "Gelişmiş"), "Kare Hızı");
    expect(row(normal, "60 fps")).toMatchObject({
      disabled: true,
      hint: "60 fps yalnız Yüksek kalitede.",
    });
    const high = sub(
      sub(ownShareItems(input({}, { preset: "high" }), t, actions()), "Gelişmiş"),
      "Kare Hızı",
    );
    expect(row(high, "60 fps").disabled).toBe(false);
  });

  it("asks for each change, and nothing for what is chosen already", () => {
    const act = actions();
    const items = ownShareItems(input(), t, act);
    row(sub(items, "Kalite"), "Düşük · 480p").onSelect();
    expect(act.change).toHaveBeenLastCalledWith(1, { preset: "low" });
    act.change.mockClear();
    row(sub(items, "Kalite"), "Normal · 720p").onSelect();
    expect(act.change).not.toHaveBeenCalled();
    row(items, "Sesi de Paylaş").onSelect();
    expect(act.change).toHaveBeenLastCalledWith(1, { audio: false });
    const advanced = sub(items, "Gelişmiş");
    row(sub(advanced, "Görüntü"), "Netlik").onSelect();
    expect(act.change).toHaveBeenLastCalledWith(1, { motion: "clarity" });
    row(sub(advanced, "Kare Hızı"), "15 fps").onSelect();
    expect(act.change).toHaveBeenLastCalledWith(1, { fps: 15 });
    row(advanced, "İmleci Göster").onSelect();
    expect(act.change).toHaveBeenLastCalledWith(1, { cursor: false });
    row(advanced, "Önizleme Resmini Paylaş").onSelect();
    expect(act.change).toHaveBeenLastCalledWith(1, { thumbs: false });
    row(items, "Duraklat").onSelect();
    expect(act.hold).toHaveBeenLastCalledWith(1, true);
    row(items, "Ekranı Değiştir…").onSelect();
    expect(act.switchSource).toHaveBeenLastCalledWith(1);
    row(items, "Paylaşımı Durdur").onSelect();
    expect(act.stop).toHaveBeenLastCalledWith(1);
    // Leaving "Yüksek" at 60 frames comes down to 30 with it.
    const fast = ownShareItems(input({}, { preset: "high", fps: 60 }), t, act);
    row(sub(fast, "Kalite"), "Normal · 720p").onSelect();
    expect(act.change).toHaveBeenLastCalledWith(1, { preset: "normal", fps: 30 });
  });

  it("resumes a paused share, and offers no pause where the source closed", () => {
    const act = actions();
    const paused = ownShareItems(
      input({ status: "paused" }, { st: "hold", hold: "paused" }),
      t,
      act,
    );
    expect(labels(paused)).toContain("Sürdür");
    expect(labels(paused)).not.toContain("Duraklat");
    row(paused, "Sürdür").onSelect();
    expect(act.hold).toHaveBeenLastCalledWith(1, false);
    for (const status of ["closed", "gameClosed"] as const) {
      const lost = labels(ownShareItems(input({ status }), t, act));
      expect(lost).not.toContain("Duraklat");
      expect(lost).not.toContain("Sürdür");
      expect(lost[0]).toBe("Ekranı Değiştir…");
    }
  });

  it("follows the kept choices where the share does not say them", () => {
    const kept = { ...DEFAULT_SHARE_SETTINGS, cursor: false, thumbs: false };
    const advanced = sub(ownShareItems(input({ settings: kept }), t, actions()), "Gelişmiş");
    expect(row(advanced, "İmleci Göster").checked).toBe(false);
    expect(row(advanced, "Önizleme Resmini Paylaş").checked).toBe(false);
    // What the share says wins over the kept choice.
    const own = sub(
      ownShareItems(input({ settings: kept }, { cursor: true, thumbs: true }), t, actions()),
      "Gelişmiş",
    );
    expect(row(own, "İmleci Göster").checked).toBe(true);
    expect(row(own, "Önizleme Resmini Paylaş").checked).toBe(true);
  });
});
