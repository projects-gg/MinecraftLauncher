// BETA-GATE: the beta update channel is for the Ar-Ge team (Settings → Güncellemeler). One line
// under "Güncelleme kanalı" with its action right after it; a quiet toast when an update look
// moved the launcher back to Kararlı. Short, sentence case; "Ar-Ge üyeliği" as ProBot's role is
// named ("Ar-Ge Üyesi").
export const trBetaGate = {
  // While the answer is out (only when it takes a moment).
  checking: "Ar-Ge üyeliğine bakılıyor",
  notMember: "Beta, Ar-Ge ekibine özel.",
  // Opens the application page in the browser.
  apply: "Başvur",
  // Followed by the action and a full stop: "… Ekipteysen Discord ile giriş yap.".
  signIn: "Beta, Ar-Ge ekibine özel. Ekipteysen",
  signInAction: "Discord ile giriş yap",
  unknown: "Ar-Ge üyeliğine şu an bakılamadı.",
  retry: "Tekrar dene",
  // The toasts after an update look moved the launcher back to Kararlı (it keeps its version). They
  // only tell, what to do stays in Settings' line; "güncellemelerin" so that "kanal" on Home never
  // reads as a Discord channel.
  movedNotMember: "Beta, Ar-Ge ekibine özel. Güncellemelerin artık Kararlı kanaldan gelecek.",
  movedSignIn:
    "Beta için Discord ile giriş yap. Şimdilik güncellemelerin Kararlı kanaldan gelecek.",
  movedUnknown: "Ar-Ge üyeliğine bakılamadı. Şimdilik güncellemelerin Kararlı kanaldan gelecek.",
};
