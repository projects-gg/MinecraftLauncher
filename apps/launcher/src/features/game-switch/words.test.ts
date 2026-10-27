import { describe, expect, it } from "vitest";
import { en } from "@/locales/en";
import { tr } from "@/locales/tr";
import { type Line, oneMoreWords, questionWords, type SwitchSides, stepWords } from "./words";

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
    stage: "opening",
    from: { account: ejder, instance: "projects", instanceName: "Projects" },
    to: { account: kor, instance: "projects", instanceName: "Projects" },
    ...overrides,
  };
}

describe("Durdur ve oyna (PLAY-ACCT): another launch on its way", () => {
  it("names the account to play with and the one whose start stops", () => {
    const words = questionWords(sides());
    expect(turkish(words.title)).toBe("Oyun KorAtesi_ hesabıyla açılsın mı?");
    expect(words.body.map(turkish)).toEqual(["EjderAvcisi ile açılmakta olan oyun durdurulacak."]);
    expect(turkish(words.confirm)).toBe("Durdur ve oyna");
    expect(english(words.title)).toBe("Play as KorAtesi_?");
    expect(words.body.map(english)).toEqual(["The game starting as EjderAvcisi will stop."]);
    expect(english(words.confirm)).toBe("Stop and play");
  });

  it("names the other installation as one when the other account starts another one", () => {
    const words = questionWords(
      sides({ to: { account: kor, instance: "fabric", instanceName: "Fabric 1.21" } }),
    );
    expect(words.body.map(turkish)).toEqual([
      "EjderAvcisi ile Projects kurulumunda açılmakta olan oyun durdurulacak.",
    ]);
  });

  it("names the installations when the same account starts another one", () => {
    const words = questionWords(
      sides({ to: { account: ejder, instance: "fabric", instanceName: "Fabric 1.21" } }),
    );
    expect(turkish(words.title)).toBe("Fabric 1.21 kurulumu açılsın mı?");
    // "Projects" is also the server: the installation is called one.
    expect(words.body.map(turkish)).toEqual([
      "Projects kurulumunda açılmakta olan oyun durdurulacak.",
    ]);
    expect(english(words.title)).toBe("Play Fabric 1.21?");
  });

  it("stops the files being prepared for another installation", () => {
    const words = questionWords(
      sides({
        stage: "preparing",
        from: { account: null, instance: "projects", instanceName: "Projects" },
        to: { account: ejder, instance: "fabric", instanceName: "Fabric 1.21" },
      }),
    );
    expect(turkish(words.title)).toBe("Fabric 1.21 kurulumu açılsın mı?");
    expect(words.body.map(turkish)).toEqual(["Projects kurulumunun hazırlığı durdurulacak."]);
  });

  it("speaks of the game without names it does not know yet", () => {
    const words = questionWords(
      sides({
        from: { account: null, instance: null, instanceName: null },
        to: { account: ejder, instance: "fabric", instanceName: null },
      }),
    );
    expect(turkish(words.title)).toBe("Oyun yeniden açılsın mı?");
    expect(words.body.map(turkish)).toEqual(["Açılmakta olan oyun durdurulacak."]);
  });

  it("says what comes once the start has stopped", () => {
    const step = stepWords(sides());
    expect(turkish(step.title)).toBe("Oyun kapanıyor");
    expect(step.body.map(turkish)).toEqual(["Kapanınca KorAtesi_ hesabıyla açılacak."]);
    const other = sides({
      to: { account: ejder, instance: "fabric", instanceName: "Fabric 1.21" },
    });
    expect(stepWords(other).body.map(english)).toEqual(["Once it closes, Fabric 1.21 will start."]);
  });
});

describe("one more game (PLAY-AGAIN)", () => {
  it("asks the owner's question for the same name, nothing more when the memory holds", () => {
    const words = oneMoreWords("again", false);
    expect(turkish(words.title)).toBe("Bu adla oyun zaten açık");
    expect(words.body.map(turkish)).toEqual(["Yine de bir tane daha açılsın mı?"]);
    expect(turkish(words.confirm)).toBe("Bir tane daha aç");
    expect(english(words.title)).toBe("A game with this name is already open");
    expect(words.body.map(english)).toEqual(["Open another one anyway?"]);
    expect(english(words.confirm)).toBe("Open another");
  });

  it("says first when the free memory would not hold one more game", () => {
    const words = oneMoreWords("again", true);
    expect(words.body.map(turkish)).toEqual([
      "Bilgisayarında bir oyun daha açmaya yetecek boş bellek yok.",
      "Yine de bir tane daha açılsın mı?",
    ]);
    // Another account's game only: the memory is the whole question.
    const beside = oneMoreWords("memory", true);
    expect(turkish(beside.title)).toBe("Bir oyun daha açılsın mı?");
    expect(beside.body.map(turkish)).toEqual([
      "Bilgisayarında bir oyun daha açmaya yetecek boş bellek yok.",
    ]);
    expect(turkish(beside.confirm)).toBe("Yine de aç");
    expect(english(beside.confirm)).toBe("Open anyway");
  });

  it("never names a technical figure or an id", () => {
    for (const kind of ["again", "memory"] as const) {
      const words = oneMoreWords(kind, true);
      for (const line of [words.title, ...words.body, words.confirm]) {
        for (const text of [turkish(line), english(line)]) {
          expect(text).not.toMatch(/\d|MB|GB|RAM|JVM/);
        }
      }
    }
  });
});
