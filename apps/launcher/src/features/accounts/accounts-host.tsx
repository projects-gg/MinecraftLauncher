// Everything the account surfaces open, mounted once with the main window's title bar: the
// account center sheet, the Microsoft sign-in, the skin panels (Microsoft's, and the Projects
// skin of username accounts with its toasts, SKIN1), the AFK launch question and a polite live
// region for screen readers (live-region.tsx).
import { prewarmImages } from "@pl/ui";
import { useEffect } from "react";
import { PRESET_AVATARS } from "@/features/avatars/presets";
import { ProjectsSkinHost } from "@/features/projects-skin/host";
import { AppearanceSheet } from "@/features/skins/appearance-sheet";
import { AccountCenterSheet } from "./account-center";
import { AfkLaunchGuardDialog } from "./afk-guard";
import { LiveRegion } from "./live-region";
import { MicrosoftLoginDialog } from "./microsoft-login-dialog";
import { useAccountsUi } from "./store";
import { useAccounts } from "./use-accounts";

export function AccountsHost() {
  const { data: accounts = [] } = useAccounts();
  const login = useAccountsUi((s) => s.login);
  const closeLogin = useAccountsUi((s) => s.closeLogin);
  const openLogin = useAccountsUi((s) => s.openLogin);
  const openCenter = useAccountsUi((s) => s.openCenter);
  const appearanceFor = useAccountsUi((s) => s.appearanceFor);
  const openAppearance = useAccountsUi((s) => s.openAppearance);
  const setHostMounted = useAccountsUi((s) => s.setHostMounted);

  useEffect(() => {
    setHostMounted(true);
    return () => setHostMounted(false);
  }, [setHostMounted]);
  // The picture picker shows all bundled pictures at once: they are fetched and decoded in idle
  // moments beforehand, so its first opening is not held up by decoding (G6: 165 ms).
  useEffect(() => prewarmImages(PRESET_AVATARS.map((avatar) => avatar.small)), []);

  const relogin = login.relogin ? (accounts.find((a) => a.id === login.relogin) ?? null) : null;
  const appearance = appearanceFor ? (accounts.find((a) => a.id === appearanceFor) ?? null) : null;

  return (
    <>
      <AccountCenterSheet />
      <MicrosoftLoginDialog
        open={login.open}
        onOpenChange={(open) => {
          if (!open) closeLogin();
        }}
        relogin={relogin}
        onUseOffline={() => openCenter({ focus: "add" })}
      />
      <AppearanceSheet
        account={appearance}
        onClose={() => openAppearance(null)}
        onSignIn={() => {
          openAppearance(null);
          openLogin(appearance?.id ?? null);
        }}
      />
      <ProjectsSkinHost />
      <AfkLaunchGuardDialog />
      <LiveRegion />
    </>
  );
}
