// 24/7 Cloud's moderation on the player's side. English counterpart of tr-afk-mod.ts: the same
// keys, the contract's English words, the same as afk.projects.gg and the phone. Root key `afkMod`.
export const enAfkMod = {
  categories: {
    advertising: "Advertising",
    spam: "Spam",
    scam: "Scam",
    rmt: "Real-money trading",
    impersonation: "Posing as staff",
    harassment: "Harassment",
    hate: "Hate speech",
    sexual: "Inappropriate content",
    minor_safety: "Unsafe for young players",
    personal_info: "Personal information",
    off_platform: "Moving players elsewhere",
    evasion: "Rule evasion",
    automation: "Automated clicking",
    other: "Rule violation",
  },
  card: {
    warn: "Got a warning: {{kind}}",
    restrict_rows: "A row was stopped: {{kind}}",
    pause_ai: "AFK+ AI was paused: {{kind}}",
    suspend: "Stopped: {{kind}}",
    appeal: "Appeal",
    appealOpen: "Your appeal is being reviewed.",
  },
  hint: {
    until: "The account opens again on {{date}}.",
    appeal: "You can appeal until {{date}}.",
    open: "Your appeal is being reviewed.",
    upheld: "We reviewed your appeal. The decision stands.",
    days: "If the decision is lifted, the stopped days are added back.",
  },
  stateLine: "Stopped: {{kind}}",
  paused: {
    line: "Stopped: {{kind}}. Fix it and save, and it runs again.",
  },
  refused: "This row seems to contain {{kind}}. Fix or delete it.",
  notice: {
    warn: "Warning: {{kind}}. If it happens again, the account will be stopped.",
    restrict_rows: "A row was stopped ({{kind}}). Fix or delete it.",
    pause_ai: "AFK+ AI was paused ({{kind}}). Your account stays AFK.",
    suspend: "Stopped: {{kind}}. If you think this is a mistake, appeal.",
    appeal_received: "We got your appeal. We'll look at it within 3 days.",
    upheld: "We reviewed your appeal. The decision stands.",
    overturned: "Your appeal was accepted. The decision was lifted.",
  },
};
