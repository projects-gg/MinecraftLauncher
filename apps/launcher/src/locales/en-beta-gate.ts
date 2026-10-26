// BETA-GATE: the beta update channel is for the R&D team (Settings → Updates). One line under
// "Update channel" with its action right after it; a quiet toast when an update look moved the
// launcher back to Stable. Short, sentence case.
export const enBetaGate = {
  checking: "Checking your R&D role on Discord",
  notMember: "Beta is for the R&D team.",
  apply: "Apply to join",
  // Followed by the action and a full stop: "… If you're on the team, sign in with Discord.".
  signIn: "Beta is for the R&D team. If you're on the team,",
  signInAction: "sign in with Discord",
  unknown: "Couldn't check your R&D role on Discord right now.",
  retry: "Try again",
  movedNotMember: "Beta is for the R&D team, so your update channel is now Stable.",
  movedSignIn:
    "If you're on the R&D team, sign in with Discord. Your update channel is Stable for now.",
  movedUnknown: "Couldn't check your R&D role on Discord, so your update channel is now Stable.",
};
