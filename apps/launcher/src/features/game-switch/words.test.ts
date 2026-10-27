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
  it("asks the owner's question with the name, nothing more when the memory holds", () => {
    const words = oneMoreWords("again", false, "EjderAvcisi");
    expect(turkish(words.title)).toBe("Bir oyun daha açılsın mı?");
    expect(words.body.map(turkish)).toEqual([
      "EjderAvcisi zaten oyunda.",
      "İki oyun aynı sunucuya girerse biri atılır.",
    ]);
    expect(words.note).toBeNull();
    expect(turkish(words.confirm)).toBe("Yine de aç");
    expect(english(words.title)).toBe("Open another game?");
    expect(words.body.map(english)).toEqual([
      "EjderAvcisi is already in a game.",
      "If both games join the same server, one gets kicked.",
    ]);
    expect(english(words.confirm)).toBe("Open anyway");
    // The account not known: the owner's own words.
    expect(oneMoreWords("again", false).body.map(turkish)).toEqual([
      "Bu adla oyun zaten açık.",
      "İki oyun aynı sunucuya girerse biri atılır.",
    ]);
  });

  it("keeps one shape when the free memory would not hold one more game", () => {
    const words = oneMoreWords("again", true, "EjderAvcisi");
    expect(turkish(words.title)).toBe("Bir oyun daha açılsın mı?");
    expect(words.body.map(turkish)).toEqual([
      "EjderAvcisi zaten oyunda.",
      "İki oyun aynı sunucuya girerse biri atılır.",
    ]);
    // The second worry is its own line under the first.
    expect(words.note && turkish(words.note)).toBe(
      "Bilgisayarında boş bellek az, oyunlar takılabilir.",
    );
    expect(turkish(words.confirm)).toBe("Yine de aç");
    // Another account's game only: the memory is the whole reason, the same title and answer.
    const beside = oneMoreWords("memory", true, "KorAtesi_");
    expect(turkish(beside.title)).toBe("Bir oyun daha açılsın mı?");
    expect(beside.body.map(turkish)).toEqual([
      "Bilgisayarında boş bellek az, oyunlar takılabilir.",
    ]);
    expect(beside.note).toBeNull();
    expect(turkish(beside.confirm)).toBe("Yine de aç");
    expect(beside.body.map(english)).toEqual([
      "Your computer is low on free memory, so the games may stutter.",
    ]);
    expect(english(beside.confirm)).toBe("Open anyway");
  });

  it("never names a technical figure or an id", () => {
    for (const kind of ["again", "memory"] as const) {
      const words = oneMoreWords(kind, true);
      const note = words.note ? [words.note] : [];
      for (const line of [words.title, ...words.body, ...note, words.confirm]) {
        for (const text of [turkish(line), english(line)]) {
          expect(text).not.toMatch(/\d|MB|GB|RAM|JVM/);
        }
      }
    }
  });
});
