// SHEET-UX: an AFK account's settings panel (desktop and 24/7 Cloud) in a few sections, its change
// bar, and the card's settings button while edits wait unsaved (docs/tasks/sheet-ux.md).
export const enAfkPanel = {
  sections: {
    connection: "Connection",
    commands: "Commands",
    game: "Gameplay",
    replies: "Auto-reply",
    log: "Logs and alerts",
  },
  unsavedSection: "unsaved changes",
  settingsUnsaved: "Settings, unsaved changes",
  barLabel: "Unsaved changes",
  portRange: "The number after the colon (:) must be between 1 and 65535.",
  interval: "Every",
  saveOnly: "Save only",
  kept: {
    title: "{{name}} settings not saved",
    body: "Your changes will be there when you open the settings again.",
  },
  wrongPassword: "The server didn't accept this password. Type the right one.",
  wrongPasswordSealed:
    'The server didn\'t accept this password. Press "Change" and type the right one.',
  rule: {
    match: "Phrase in the message",
    pattern: "Custom pattern",
    patternPlaceholder: "(?i)^!invite \\w+$",
    patternHelp: "A regular expression. Start it with (?i) so letter case doesn't matter.",
    writeMyself: "I'll write the pattern myself",
    toPlain: "Back to a plain phrase",
    cooldown: "Time between replies",
    ownerOnly: "Only reply to me",
    ownerOnlyHelp: "Works only when {{name}} writes.",
    ownerFirst: "First write the name you play with above.",
    ownerDesktop: "Works only when the account you play with in the launcher writes.",
  },
};
