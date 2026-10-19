// English for tr-afk-connect.ts. Missing keys fall back to Turkish.
export const enAfkConnect = {
  preparing: "Getting the AFK client ready",
  downloading: "Downloading the AFK client",
  percent: "{{value}}%",
  cancel: "Cancel",
  problem: {
    notPublished: {
      title: "The AFK client isn't ready yet",
      hint: "Try again in a bit.",
    },
    offline: {
      title: "Couldn't download the AFK client",
      hint: "Check your internet connection.",
    },
    diskFull: {
      title: "The disk is full",
      hint: "Free up some space and try again.",
    },
    blocked: {
      title: "Couldn't install the AFK client",
      hint: "Make sure your antivirus isn't blocking it.",
    },
    broken: {
      title: "Couldn't download the AFK client",
      hint: "Try again in a bit.",
    },
  },
  notice: {
    failed: "{{name}} couldn't connect",
    failedMany: "{{count}} AFK accounts couldn't connect",
  },
  failure: {
    failed: "Couldn't connect",
    unexpectedExit: "The AFK client closed unexpectedly",
    launchFailed: "The AFK client couldn't start",
    clientMissing: "The AFK client is missing",
    secretsUnavailable: "Couldn't read the login commands",
  },
};
