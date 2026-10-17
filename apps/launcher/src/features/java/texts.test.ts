import { describe, expect, it } from "vitest";
import type { JavaPrompt, JavaRuntime } from "@/lib/api";
import i18n from "@/lib/i18n";
import {
  groupRuntimes,
  locationLabel,
  promptBody,
  promptTitle,
  readableVersion,
  runtimeAction,
  runtimeTitle,
  sameJava,
  shortPath,
  switchableUsers,
} from "./texts";

describe("matching a picked Java", () => {
  const javaw = "C:\\Program Files\\Eclipse Adoptium\\jdk-21.0.4\\bin\\javaw.exe";
  it("takes java.exe, javaw.exe, the bin folder and the folder as one Java", () => {
    expect(sameJava(javaw, "C:\\Program Files\\Eclipse Adoptium\\jdk-21.0.4\\bin\\java.exe")).toBe(
      true,
    );
    expect(sameJava(javaw, "c:/program files/eclipse adoptium/jdk-21.0.4/bin")).toBe(true);
    expect(sameJava(javaw, "C:\\Program Files\\Eclipse Adoptium\\jdk-21.0.4\\")).toBe(true);
    expect(sameJava("/usr/lib/jvm/java-21/bin/java", "/usr/lib/jvm/java-21")).toBe(true);
  });

  it("tells different Javas apart and never matches an empty path", () => {
    expect(
      sameJava(javaw, "C:\\Program Files\\Eclipse Adoptium\\jdk-17.0.12\\bin\\javaw.exe"),
    ).toBe(false);
    expect(sameJava("", "")).toBe(false);
    expect(sameJava("  ", javaw)).toBe(false);
  });
});

const t = i18n.t.bind(i18n);

function prompt(extra: Partial<JavaPrompt>): JavaPrompt {
  return {
    instanceId: "projects",
    instanceName: "Projects",
    problem: "tooOld",
    have: 17,
    need: 25,
    forMods: false,
    fixable: true,
    automatic: true,
    ...extra,
  };
}

function runtime(extra: Partial<JavaRuntime>): JavaRuntime {
  return {
    id: "managed:java-runtime-delta/windows-x64",
    kind: "managed",
    major: 21,
    version: "21.0.7",
    location: "C:\\Data\\runtimes\\java-runtime-delta\\windows-x64",
    executable: "C:\\Data\\runtimes\\java-runtime-delta\\windows-x64\\bin\\javaw.exe",
    bits32: false,
    users: [],
    component: "java-runtime-delta",
    platform: "windows-x64",
    staged: false,
    queued: false,
    updateFailed: false,
    ...extra,
  };
}

describe("the Java question", () => {
  it("names what the game needs", () => {
    expect(promptTitle(t, prompt({}))).toBe("Java 25 gerekiyor");
    expect(promptTitle(t, prompt({ problem: "tooNew", have: 21, need: 8 }))).toBe(
      "Bu sürüm Java 8 ile çalışır",
    );
    expect(promptTitle(t, prompt({ problem: "missing", have: undefined }))).toBe("Java bulunamadı");
    expect(promptTitle(t, prompt({ problem: "broken", have: undefined }))).toBe("Java açılmıyor");
  });

  it("says what Kur ve oyna changes", () => {
    expect(promptBody(t, prompt({}))).toBe(
      "Uygun Java hazırlanır ve kurulum Otomatik Java'ya geçer.",
    );
    expect(promptBody(t, prompt({ automatic: false, need: 21, forMods: true }))).toBe(
      "Launcher'ın Java 21 sürümü hazırlanır ve bu kuruluma bağlanır.",
    );
    expect(promptBody(t, prompt({ fixable: false }))).toBe(
      "Uygun Java'yı kurulum ayarlarından seçebilirsin.",
    );
  });
});

describe("Settings → Java rows", () => {
  it("titles custom paths by their problem", () => {
    expect(runtimeTitle(t, runtime({}))).toBe("Java 21");
    expect(runtimeTitle(t, runtime({ kind: "custom", problem: "missing", major: undefined }))).toBe(
      "Java bulunamadı",
    );
    expect(runtimeTitle(t, runtime({ kind: "custom", problem: "broken", major: undefined }))).toBe(
      "Java açılmıyor",
    );
  });

  it("shortens long folders to the drive and the last two folders", () => {
    expect(shortPath("C:\\Program Files\\Eclipse Adoptium\\jdk-17.0.12.7-hotspot")).toBe(
      "C:\\Program Files\\Eclipse Adoptium\\jdk-17.0.12.7-hotspot",
    );
    expect(
      shortPath(
        "C:\\Users\\Ejder\\AppData\\Roaming\\ProjectsLauncher\\runtimes\\delta\\windows-x64",
      ),
    ).toBe("C:\\…\\delta\\windows-x64");
    expect(shortPath("/usr/lib/jvm/java-21-openjdk-amd64")).toBe("/…/jvm/java-21-openjdk-amd64");
    expect(shortPath("D:\\Oyunlar\\java")).toBe("D:\\Oyunlar\\java");
  });

  it("names the folders players know", () => {
    expect(locationLabel(t, runtime({}))).toBe("Launcher klasörü");
    const at = (location: string) =>
      locationLabel(t, runtime({ kind: "system", location, component: undefined }));
    // The official launcher's own runtimes (classic install and the Store app).
    expect(
      at(
        "C:\\Users\\Ejder\\AppData\\Roaming\\.minecraft\\runtime\\java-runtime-delta\\windows-x64\\java-runtime-delta",
      ),
    ).toBe("Minecraft Launcher");
    expect(
      at(
        "C:\\Users\\Ejder\\AppData\\Local\\Packages\\Microsoft.4297127D64EC6_8wekyb3d8bbwe\\LocalCache\\Local\\runtime\\java-runtime-epsilon\\windows-x64\\java-runtime-epsilon",
      ),
    ).toBe("Minecraft Launcher");
    expect(at("C:\\Program Files\\Eclipse Adoptium\\jdk-21")).toBe(
      "C:\\Program Files\\Eclipse Adoptium\\jdk-21",
    );
  });

  it("shows one action, the most pressing first", () => {
    const update = runtime({ update: "21.0.8" });
    expect(runtimeAction(update)).toEqual({ kind: "update", version: "21.0.8" });
    // Live progress wins over everything else.
    expect(runtimeAction(update, 42)).toEqual({ kind: "updating", percent: 42 });
    expect(runtimeAction(runtime({ update: "21.0.8", updating: 10 }))).toEqual({
      kind: "updating",
      percent: 10,
    });
    expect(runtimeAction(runtime({ update: "21.0.8", staged: true }))).toEqual({
      kind: "waiting",
    });
    expect(runtimeAction(runtime({ update: "21.0.8", queued: true }))).toEqual({
      kind: "waiting",
    });
    expect(runtimeAction(runtime({ update: "21.0.8", updateFailed: true }))).toEqual({
      kind: "failed",
    });
    expect(runtimeAction(runtime({ removableBytes: 1000 }))).toEqual({
      kind: "remove",
      bytes: 1000,
    });
    // An unused runtime is offered for removal, not for an update.
    expect(runtimeAction(runtime({ update: "21.0.8", removableBytes: 1000 }))).toEqual({
      kind: "remove",
      bytes: 1000,
    });
    expect(runtimeAction(runtime({}))).toEqual({ kind: "none" });
  });

  it("moves only instances on a custom Java to Otomatik", () => {
    const system = runtime({
      id: "path:C:\\Java\\bin\\javaw.exe",
      kind: "system",
      component: undefined,
      platform: undefined,
      users: [
        { instanceId: "a", name: "A", automatic: false, fit: "tooOld" },
        { instanceId: "b", name: "B", automatic: false },
      ],
    });
    expect(switchableUsers(system)).toEqual(["a", "b"]);
    expect(runtimeAction(system)).toEqual({ kind: "useAuto", instanceIds: ["a", "b"] });
    expect(runtimeAction({ ...system, users: [] })).toEqual({ kind: "none" });
    // Launcher runtimes are never "switched away" from (Otomatik already uses them).
    expect(
      switchableUsers(runtime({ users: [{ instanceId: "c", name: "C", automatic: false }] })),
    ).toEqual([]);
  });

  it("groups launcher runtimes, the computer's and custom paths", () => {
    const groups = groupRuntimes([
      runtime({ id: "m" }),
      runtime({ id: "s", kind: "system" }),
      runtime({ id: "c", kind: "custom" }),
    ]);
    expect(groups.managed.map((r) => r.id)).toEqual(["m"]);
    expect(groups.system.map((r) => r.id)).toEqual(["s"]);
    expect(groups.custom.map((r) => r.id)).toEqual(["c"]);
  });

  it("shows versions without vendor build tags", () => {
    expect(readableVersion("8u51-cacert462b08")).toBe("8u51");
    expect(readableVersion("25.0.3+9.1")).toBe("25.0.3");
    expect(readableVersion("21.0.7")).toBe("21.0.7");
  });
});
