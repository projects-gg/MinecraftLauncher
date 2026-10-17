// Settings → Hesaplar shows the account center itself: one component, one source of truth.
import { AccountCenter } from "@/features/accounts/account-center";

export { MicrosoftLoginDialog } from "@/features/accounts/microsoft-login-dialog";
/** Minecraft's rule for player names (the first-run flow checks offline names with it). */
export { OFFLINE_NAME } from "@/features/accounts/name-rules";

export function AccountsSection() {
  return <AccountCenter variant="embedded" />;
}
