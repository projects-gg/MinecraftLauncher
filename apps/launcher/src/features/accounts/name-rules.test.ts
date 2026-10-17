import { describe, expect, it } from "vitest";
import { checkOfflineName, findTakenName, type NamedAccount, suggestName } from "./name-rules";

const accounts: NamedAccount[] = [
  { id: "a1", kind: "offline", username: "EjderAvcisi" },
  { id: "a2", kind: "microsoft", username: "KorAtesi_" },
  { id: "a3", kind: "offline", username: "Cagri" },
];

describe("checkOfflineName", () => {
  it("accepts Minecraft names", () => {
    for (const name of ["abc", "Notch", "A_1234567890_bcd", "  Steve  ", "___"]) {
      const check = checkOfflineName(name, accounts, { final: true });
      expect(check.problem, name).toBeNull();
      expect(check.name).toBe(name.trim());
    }
  });

  it("waits for the player before calling a name too short or empty", () => {
    expect(checkOfflineName("", accounts).problem).toBeNull();
    expect(checkOfflineName("ab", accounts).problem).toBeNull();
    expect(checkOfflineName("", accounts, { final: true }).problem).toBe("empty");
    expect(checkOfflineName("   ", accounts, { final: true }).problem).toBe("empty");
    expect(checkOfflineName("ab", accounts, { final: true }).problem).toBe("tooShort");
  });

  it("flags long names at once", () => {
    expect(checkOfflineName("abcdefghijklmnopq", accounts).problem).toBe("tooLong");
    expect(checkOfflineName("abcdefghijklmnop", accounts).problem).toBeNull();
  });

  it("explains Turkish letters and spaces, with a fix", () => {
    const turkish = checkOfflineName("Gölge Avcı", accounts);
    expect(turkish.problem).toBe("turkish");
    expect(turkish.suggestion).toBe("Golge_Avci");

    const spaces = checkOfflineName("Ali Veli", accounts);
    expect(spaces.problem).toBe("spaces");
    expect(spaces.suggestion).toBe("Ali_Veli");

    const symbols = checkOfflineName("nick?!", accounts);
    expect(symbols.problem).toBe("characters");
    expect(symbols.suggestion).toBe("nick");
    expect(checkOfflineName("a-b", accounts).problem).toBe("characters");
  });

  it("does not suggest a name that is taken or unusable", () => {
    // "Çağrı" spells out to "Cagri", which another account has.
    const taken = checkOfflineName("Çağrı", accounts);
    expect(taken.problem).toBe("turkish");
    expect(taken.suggestion).toBeNull();
    expect(checkOfflineName("ş!", accounts).suggestion).toBeNull();
    expect(suggestName("İstanbul Şehri 34 Çok Uzun")).toBe("Istanbul_Sehri_3");
  });

  it("finds duplicates ignoring case, only among offline accounts", () => {
    const check = checkOfflineName("ejderavcisi", accounts);
    expect(check.problem).toBe("taken");
    expect(check.takenBy?.id).toBe("a1");
    // A Microsoft account with that name is a different player.
    expect(checkOfflineName("koratesi_", accounts).problem).toBeNull();
    // Renaming an account to its own name in other case is fine.
    expect(checkOfflineName("EJDERAVCISI", accounts, { selfId: "a1" }).problem).toBeNull();
    expect(findTakenName("cagri", accounts, "a1")?.id).toBe("a3");
    expect(findTakenName("cagri", accounts, "a3")).toBeNull();
  });
});
