import { create } from "zustand";
import { persist } from "zustand/middleware";

/**
 * "Şimdi Değil" (SRV-ACC): the digest of the list the player put off (`listDigest`), kept on this
 * computer; the row stays quiet until the list's digest differs.
 */
interface ServerAccountsPrefs {
  dismissed: string | null;
  dismiss: (digest: string | null) => void;
}

export const useServerAccountsPrefs = create<ServerAccountsPrefs>()(
  persist(
    (set) => ({
      dismissed: null,
      dismiss: (dismissed) => set({ dismissed }),
    }),
    { name: "pl.serverAccounts", version: 1 },
  ),
);
