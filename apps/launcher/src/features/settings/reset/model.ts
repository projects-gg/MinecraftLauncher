// "Launcher'ı sıfırla" (Settings → Veri, INSTALL-UX): when the dialog's button may act and the
// names it lists. What a reset removes and keeps: src-tauri reset.rs and pl_data::reset.

/**
 * The typed word confirms the reset. Case, the spaces around it and Turkish dotted and dotless
 * i do not matter: "SIFIRLA", "Sıfırla" and "sifirla" (an English keyboard) all count.
 */
export function confirmMatches(typed: string, word: string): boolean {
  const fold = (text: string) => text.trim().toLocaleLowerCase("tr").replaceAll("ı", "i");
  const expected = fold(word);
  return expected.length > 0 && fold(typed) === expected;
}

/** AFK states in which an account runs or is on its way: the reset waits for them to stop. */
const AFK_RESTING = new Set(["stopped", "clientFailed", "error"]);

/** Some AFK account runs (the backend refuses a reset then; the dialog says so before typing). */
export function afkRunning(afk: readonly { state: { kind: string } }[] | undefined): boolean {
  return (afk ?? []).some((account) => !AFK_RESTING.has(account.state.kind));
}

/** The saved names a reset removes, as the dialog lists them (in the order the player sees). */
export function removedNames(
  accounts: readonly { username: string }[] | undefined,
  afk: readonly { nickname: string }[] | undefined,
): { accounts: string[]; afk: string[] } {
  return {
    accounts: (accounts ?? []).map((account) => account.username),
    afk: (afk ?? []).map((account) => account.nickname),
  };
}
