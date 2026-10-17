import { describe, expect, it } from "vitest";
import type { ModIssue } from "@/lib/api";
import i18n from "@/lib/i18n";
import { issueLines } from "./issue-lines";

const t = i18n.t.bind(i18n);

const issue = (extra: Partial<ModIssue>): ModIssue => ({
  fileName: "sodium-fabric-0.6.13+mc1.21.4.jar",
  folder: "mods",
  kind: "unreadable",
  detail: "Access is denied. (os error 5)",
  ...extra,
});

describe("the mod center's problem lines", () => {
  it("names recommended mods by their titles, never by file name", () => {
    expect(issueLines(t, [issue({ title: "Sodium" })])).toEqual(["Sodium okunamadı."]);
    expect(issueLines(t, [issue({ title: "Iris", kind: "busy" })])).toEqual([
      "Iris başka bir programda açık.",
    ]);
  });

  it("counts other files by kind of file and problem", () => {
    const lines = issueLines(t, [
      issue({ folder: "config", fileName: "a.json" }),
      issue({ folder: "config/xaero", fileName: "b.txt" }),
      issue({ folder: "resourcepacks", fileName: "c.zip", kind: "notAFile" }),
      issue({ folder: "", fileName: "options.txt" }),
    ]);
    expect(lines).toEqual([
      "2 mod ayarı dosyası okunamadı.",
      "Bir kaynak paketi için dosya yerine klasör bulundu.",
      "Bir dosya okunamadı.",
    ]);
    expect(lines.join(" ")).not.toMatch(/\.(json|txt|zip|jar)/);
  });
});
