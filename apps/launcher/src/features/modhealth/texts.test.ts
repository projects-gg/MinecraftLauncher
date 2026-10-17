import { describe, expect, it } from "vitest";
import type { HealthIssue, RepairStep } from "@/lib/api";
import i18n from "@/lib/i18n";
import {
  countSteps,
  crashHint,
  crashTitle,
  currentStep,
  fixLabel,
  formatMemory,
  issueText,
  joinNames,
  stepNote,
  stepText,
  suspectText,
} from "./texts";

const t = i18n.t.bind(i18n);

function issue(extra: Partial<HealthIssue>): HealthIssue {
  return {
    key: "k",
    severity: "blocking",
    kind: "missingDependency",
    mods: ["Sodium Extra"],
    bundled: false,
    files: ["sodium-extra.jar"],
    ...extra,
  };
}

function step(extra: Partial<RepairStep>): RepairStep {
  return { key: "s", kind: "update", name: "Sodium", status: "pending", ...extra };
}

describe("mod health texts", () => {
  it("joins names the Turkish way", () => {
    expect(joinNames(["Sodium"])).toBe("Sodium");
    expect(joinNames(["Sodium", "Iris"])).toBe("Sodium ve Iris");
    expect(joinNames(["Sodium", "Iris", "Lithium", "Iris"])).toBe("Sodium, Iris ve Lithium");
  });

  it("says what each issue is, by name only", () => {
    expect(issueText(t, issue({ target: "Sodium" }), "1.21.1")).toBe(
      "Sodium Extra için Sodium gerekiyor.",
    );
    expect(
      issueText(t, issue({ target: "Sodium", offer: { label: "enable", count: 1 } }), "1.21.1"),
    ).toBe("Sodium Extra için gereken Sodium kapalı.");
    expect(
      issueText(t, issue({ kind: "incompatibleMinecraft", mods: ["Xaero's Minimap"] }), "1.21.1"),
    ).toBe("Xaero's Minimap, Minecraft 1.21.1 ile uyumlu değil.");
    expect(
      issueText(t, issue({ kind: "wrongLoader", mods: ["Create"], madeFor: "Forge" }), "1.21.1"),
    ).toBe("Create, Forge için yapılmış; bu kurulumda çalışmaz.");
    expect(issueText(t, issue({ kind: "javaVersion", mods: ["Modern"], java: 25 }), "1.21.1")).toBe(
      "Modern, Java 25 istiyor.",
    );
    expect(issueText(t, issue({ kind: "javaVersion", mods: ["Modern"] }), "1.21.1")).toBe(
      "Modern daha yeni bir Java istiyor.",
    );
    expect(
      issueText(t, issue({ kind: "breaks", mods: ["Iris"], target: "OptiFabric" }), "1.21.1"),
    ).toBe("Iris ile OptiFabric birlikte çalışmıyor.");
    expect(issueText(t, issue({ kind: "duplicate", mods: ["Lithium"] }), "1.21.1")).toBe(
      "Lithium birden fazla kez kurulu.",
    );
    // The launcher names a broken file by its catalog title (Rust side), never its file name.
    const text = issueText(t, issue({ kind: "corrupt", mods: ["Sodium"] }), "1.21.1");
    expect(text).toBe("Sodium dosyası bozuk.");
  });

  it("keeps one dependency many mods need to one short line", () => {
    const mods = ["EntityCulling", "Gamma Utils", "Zoomify", "AppleSkin", "Mod Menu", "Iris"];
    expect(issueText(t, issue({ mods, target: "Fabric API" }), "1.21.1")).toBe(
      "EntityCulling, Gamma Utils ve 4 mod daha için Fabric API gerekiyor.",
    );
    expect(issueText(t, issue({ mods: mods.slice(0, 3), target: "Fabric API" }), "1.21.1")).toBe(
      "EntityCulling, Gamma Utils ve Zoomify için Fabric API gerekiyor.",
    );
  });

  it("labels the fix buttons", () => {
    expect(fixLabel(t, { label: "fix", count: 1 })).toBe("Düzelt");
    expect(fixLabel(t, { label: "disable", count: 1 })).toBe("Bu modu kapat");
    expect(fixLabel(t, { label: "disableNamed", name: "OptiFabric", count: 1 })).toBe(
      "OptiFabric modunu kapat",
    );
    expect(fixLabel(t, { label: "disableOld", count: 1 })).toBe("Eski kopyayı kapat");
    expect(fixLabel(t, { label: "disableOld", count: 2 })).toBe("Eski kopyaları kapat");
    expect(fixLabel(t, { label: "enable", count: 1 })).toBe("Aç");
  });

  it("tells a step's story in tenses", () => {
    expect(stepText(t, step({}))).toBe("Sodium güncellenecek");
    expect(stepText(t, step({ status: "running" }))).toBe("Sodium güncelleniyor");
    expect(stepText(t, step({ status: "done" }))).toBe("Sodium güncellendi");
    expect(stepText(t, step({ status: "failed", note: "noVersion" }))).toBe(
      "Sodium güncellenemedi",
    );
    expect(stepNote(t, step({ status: "failed", note: "noVersion" }))).toBe(
      "Bu sürüm için uygun sürümü yok.",
    );
    // Turned off instead of updated: the line says what happened, the note says why.
    const off = step({ status: "done", note: "disabledInstead" });
    expect(stepText(t, off)).toBe("Sodium kapatıldı");
    expect(stepNote(t, off)).toBe("Uyumlu sürümü yok.");
    expect(
      stepText(t, step({ kind: "fetch", name: "Fabric API", status: "done", note: "fromPack" })),
    ).toBe("Fabric API önerilen modlardan açıldı");
    expect(stepText(t, step({ kind: "verifyPack", name: "", status: "running" }))).toBe(
      "Paket dosyaları denetleniyor",
    );
    expect(stepText(t, step({ kind: "disableOld", name: "Lithium", status: "done" }))).toBe(
      "Lithium modunun eski kopyası kapatıldı",
    );
    const skipped = step({ status: "skipped", note: "notNeeded" });
    expect(stepText(t, skipped)).toBe("Sodium güncellenecek");
    expect(stepNote(t, skipped)).toBe("Gerek kalmadı.");
  });

  it("counts steps and finds the current one", () => {
    const steps = [
      step({ key: "a", status: "done" }),
      step({ key: "b", status: "failed" }),
      step({ key: "c", status: "pending" }),
      step({ key: "d", status: "skipped" }),
    ];
    expect(countSteps(steps)).toEqual({ done: 1, failed: 1, skipped: 1, total: 4 });
    expect(currentStep(steps)?.key).toBe("c");
    expect(currentStep([...steps, step({ key: "e", status: "running" })])?.key).toBe("e");
  });

  it("explains crash suspects and kinds", () => {
    expect(suspectText(t, { name: "Sodium", reason: "mixinFailed", strong: true })).toBe(
      "Oyunu değiştirirken hata verdi",
    );
    expect(
      suspectText(t, {
        name: "Iris",
        reason: "declaresIncompatible",
        related: "OptiFabric",
        strong: true,
      }),
    ).toBe("OptiFabric ile birlikte çalışmıyor");
    expect(suspectText(t, { name: "Iris", reason: "declaresIncompatible", strong: true })).toBe(
      "Başka bir modla birlikte çalışmıyor",
    );
    expect(crashTitle(t, "outOfMemory", "x")).toBe("Oyunun belleği yetmedi");
    expect(crashTitle(t, undefined, "Kendi başlığı")).toBe("Kendi başlığı");
    expect(crashHint(t, "graphicsDriver", "")).toContain("sürücünü güncelleyip");
  });

  it("formats memory sizes", () => {
    expect(formatMemory(6144)).toBe("6 GB");
    expect(formatMemory(4608)).toBe("4,5 GB");
    expect(formatMemory(768)).toBe("768 MB");
  });
});
