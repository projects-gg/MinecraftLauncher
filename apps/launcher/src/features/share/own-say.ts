// Status messages for the player's own share (SQOL; WCAG 4.1.3): "Paylaşım duraklatıldı.",
// "Kalite: Yüksek, 1080p." go through the app's one polite live region (`announce`), loaded when
// first needed, so the "Yayın" window's page, which has no region of its own, never carries it.
export function sayShare(text: string) {
  void import("@/features/accounts/use-accounts")
    .then(({ announce }) => announce(text))
    .catch(() => {
      // No region on this page: nothing to say it through.
    });
}
