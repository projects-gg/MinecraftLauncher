import { describe, expect, it } from "vitest";
import { en } from "@/locales/en";
import { tr } from "@/locales/tr";
import { type Line, questionWords, type SwitchSides, stepWords } from "./words";

/** The line in a language, as i18next fills it. */
function say(strings: object, [key, values]: Line): string {
  let text: unknown = strings;
  for (const part of key.split(".")) text = (text as Record<string, unknown>)?.[part];
  if (typeof text !== "string") throw new Error(`no text for ${key}`);
  return text.replace(/\{\{(\w+)\}\}/g, (_, name: string) => values?.[name] ?? `{{${name}}}`);
}

const turkish = (line: Line) => say(tr, line);
const english = (line: Line) => say(en, line);

const ejder = { id: "a1", name: "EjderAvcisi" };
const kor = { id: "a2", name: "KorAtesi_" };

function sides(overrides: Partial<SwitchSides> = {}): SwitchSides {
  return {
    stage: "open",
    from: { account: ejder, instance: "projects", instanceName: "Projects" },
    to: { account: kor, instance: "projects", instanceName: "Projects" },
    ...overrides,
  };
}

describe("the question (PLAY-ACCT)", () => {
  it("names the account to play with and the one whose game is saved and closed", () => {
    const words = questionWords(sides());
    expect(turkish(words.title)).toBe("Oyun KorAtesi_ ile açılsın mı?");
    expect(words.body.map(turkish)).toEqual([
      "EjderAvcisi ile açık olan oyun kaydedilip kapanacak.",
    ]);
    expect(turkish(words.confirm)).toBe("Kapat ve oyna");
    expect(english(words.title)).toBe("Play as KorAtesi_?");
    expect(words.body.map(english)).toEqual([
      "The game running as EjderAvcisi will be saved and closed.",
    ]);
    expect(english(words.confirm)).toBe("Close and play");
  });

  it("names the other installation as one when the other account plays another one", () => {
    const words = questionWords(
      sides({ to: { account: kor, instance: "fabric", instanceName: "Fabric 1.21" } }),
    );
    expect(turkish(words.title)).toBe("Oyun KorAtesi_ ile açılsın mı?");
    expect(words.body.map(turkish)).toEqual([
      "EjderAvcisi ile Projects kurulumunda açık olan oyun kaydedilip kapanacak.",
    ]);
  });

  it("names the installations when the same account plays another one", () => {
    const words = questionWords(
      sides({ to: { account: ejder, instance: "fabric", instanceName: "Fabric 1.21" } }),
    );
    expect(turkish(words.title)).toBe("Fabric 1.21 açılsın mı?");
    // "Projects" is also the server: the installation is called one.
    expect(words.body.map(turkish)).toEqual([
      "Projects kurulumunda açık olan oyun kaydedilip kapanacak.",
    ]);
    expect(english(words.title)).toBe("Play Fabric 1.21?");
  });

  it("stops a game that is still opening, with nothing to save", () => {
    const words = questionWords(sides({ stage: "opening" }));
    expect(words.body.map(turkish)).toEqual(["EjderAvcisi ile açılmakta olan oyun durdurulacak."]);
    expect(turkish(words.confirm)).toBe("Durdur ve oyna");
    expect(english(words.confirm)).toBe("Stop and play");
  });

  it("stops the files being prepared for another installation", () => {
    const words = questionWords(
      sides({
        stage: "preparing",
        from: { account: null, instance: "projects", instanceName: "Projects" },
        to: { account: ejder, instance: "fabric", instanceName: "Fabric 1.21" },
      }),
    );
    expect(turkish(words.title)).toBe("Fabric 1.21 açılsın mı?");
    expect(words.body.map(turkish)).toEqual(["Projects kurulumunun hazırlığı durdurulacak."]);
    expect(turkish(words.confirm)).toBe("Durdur ve oyna");
  });

  it("speaks of the game without names it does not know yet", () => {
    const words = questionWords(
      sides({
        from: { account: null, instance: null, instanceName: null },
        to: { account: ejder, instance: "fabric", instanceName: null },
      }),
    );
    expect(turkish(words.title)).toBe("Oyun yeniden açılsın mı?");
    expect(words.body.map(turkish)).toEqual(["Açık olan oyun kaydedilip kapanacak."]);
  });
});

describe("the way after the answer", () => {
  it("says what comes once the game has closed", () => {
    const closing = stepWords(sides(), "closing");
    expect(turkish(closing.title)).toBe("Oyun kapanıyor");
    expect(closing.body.map(turkish)).toEqual(["Kapanınca KorAtesi_ ile açılacak."]);
    const other = sides({
      to: { account: ejder, instance: "fabric", instanceName: "Fabric 1.21" },
    });
    expect(stepWords(other, "closing").body.map(turkish)).toEqual([
      "Kapanınca Fabric 1.21 kurulumu açılacak.",
    ]);
    expect(stepWords(other, "closing").body.map(english)).toEqual([
      "Once it closes, Fabric 1.21 will start.",
    ]);
  });

  it("offers to end a game that has not closed yet, with its warning", () => {
    const words = stepWords(sides(), "overdue");
    expect(turkish(words.title)).toBe("Oyun henüz kapanmadı");
    expect(words.body.map(turkish)).toEqual([
      "Zorla kapatırsan tek oyunculu dünyandaki kaydedilmemiş ilerleme kaybolabilir.",
    ]);
    expect(turkish(stepWords(sides(), "forcing").title)).toBe("Oyun kapatılıyor");
    expect(tr.playAcct.switch.force).toBe("Zorla kapat ve oyna");
    expect(english(["playAcct.switch.force"])).toBe("Force close and play");
  });
});
