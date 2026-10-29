import { describe, expect, it } from "vitest";
import type { Friend, FriendRequest, SocialInvite, SocialNotice } from "@/lib/api/social-types";
import {
  BACKUP_PREFS,
  backupFingerprint,
  buildBackup,
  formatCodeInput,
  groupFriends,
  homeStrip,
  isProjectsHost,
  NoticeThrottle,
  normalizeCode,
  notifiesOnline,
  onlineFriends,
  presenceLine,
  readBackup,
  wantsNotice,
} from "./model";

const t = ((key: string, options?: Record<string, unknown>) =>
  options?.host ? `${key}:${String(options.host)}` : key) as never;

function friend(id: string, name: string, extra: Partial<Friend> = {}): Friend {
  return {
    id,
    name,
    username: name.toLowerCase(),
    avatar: "discord",
    state: "offline",
    dnd: false,
    discordUrl: `https://discord.com/users/${id}`,
    ...extra,
  };
}

describe("groupFriends", () => {
  const list = [
    friend("1", "Zeynep", { state: "playing", game: { where: "other" } }),
    friend("2", "Ayşe", { state: "playing", game: { where: "projects" } }),
    friend("3", "Çağrı", { state: "launcher" }),
    friend("4", "Can", { state: "launcher" }),
    friend("5", "Bora", { state: "offline", lastSeen: "week" }),
    friend("6", "Ece", { state: "offline", lastSeen: "today" }),
    friend("7", "Deniz", { state: "offline" }),
    friend("8", "İrem", { state: "playing", game: { where: "projects" } }),
  ];

  it("puts projects.gg first, then Turkish name order", () => {
    const groups = groupFriends(list, []);
    expect(groups.playing.map((f) => f.name)).toEqual(["Ayşe", "İrem", "Zeynep"]);
    expect(groups.online.map((f) => f.name)).toEqual(["Can", "Çağrı"]);
    // Seen recently first; unknown last.
    expect(groups.offline.map((f) => f.name)).toEqual(["Ece", "Bora", "Deniz"]);
  });

  it("lets favourites lead every group", () => {
    const groups = groupFriends(list, ["1", "3", "7"]);
    expect(groups.playing[0]?.name).toBe("Zeynep");
    expect(groups.online[0]?.name).toBe("Çağrı");
    expect(groups.offline[0]?.name).toBe("Deniz");
    expect(onlineFriends(list, []).map((f) => f.id)).toEqual(["2", "8", "1", "4", "3"]);
  });
});

describe("homeStrip", () => {
  const empty = {
    enabled: true,
    auth: "signedIn" as const,
    loaded: true,
    friends: [] as Friend[],
    incoming: [] as FriendRequest[],
    outgoing: [] as FriendRequest[],
    invites: [] as SocialInvite[],
  };
  const request: FriendRequest = {
    id: "9",
    name: "Yiğit",
    username: "yigit",
    avatar: "discord",
    sentAt: 0,
  };

  it("shows nothing while the feature is off", () => {
    expect(homeStrip({ ...empty, enabled: false, auth: "signedOut" }, [])).toBeNull();
    expect(homeStrip({ ...empty, enabled: false }, [])).toBeNull();
  });

  it("invites to sign in until the player closes it", () => {
    expect(homeStrip({ ...empty, auth: "signedOut", loaded: false }, [])).toBe("connect");
    // The browser is open: the line stays, its button leads back to the waiting card.
    expect(homeStrip({ ...empty, auth: "signingIn", loaded: false }, [])).toBe("connect");
    expect(homeStrip({ ...empty, auth: "signedOut" }, ["connect"])).toBeNull();
    expect(homeStrip({ ...empty, auth: "signedOut" }, ["add"])).toBe("connect");
  });

  it("waits while a saved session is checked or the list is on its way", () => {
    expect(homeStrip({ ...empty, auth: "restoring", loaded: false }, [])).toBeNull();
    expect(homeStrip({ ...empty, loaded: false }, [])).toBeNull();
  });

  it("invites to add a first friend only with nobody on the list and no request", () => {
    expect(homeStrip(empty, [])).toBe("add");
    expect(homeStrip(empty, ["add"])).toBeNull();
    expect(homeStrip({ ...empty, outgoing: [request] }, [])).toBeNull();
    expect(homeStrip({ ...empty, incoming: [request] }, [])).toBeNull();
    expect(homeStrip({ ...empty, friends: [friend("1", "Ayşe")] }, [])).toBeNull();
    expect(
      homeStrip({ ...empty, friends: [friend("1", "Ayşe", { state: "launcher" })] }, []),
    ).toBeNull();
  });

  it("shows friends in game, closed invitations or not", () => {
    const playing = [friend("1", "Ayşe", { state: "playing", game: { where: "projects" } })];
    expect(homeStrip({ ...empty, friends: playing }, [])).toBe("playing");
    expect(homeStrip({ ...empty, friends: playing }, ["connect", "add"])).toBe("playing");
  });
});

describe("presenceLine", () => {
  it("names where a friend is", () => {
    expect(
      presenceLine(friend("1", "A", { state: "playing", game: { where: "projects" } }), t),
    ).toBe("social.presence.projects");
    expect(presenceLine(friend("1", "A", { state: "playing", game: { where: "other" } }), t)).toBe(
      "social.presence.other",
    );
    expect(
      presenceLine(
        friend("1", "A", { state: "playing", game: { where: "other", host: "mc.example.net" } }),
        t,
      ),
    ).toBe("social.presence.otherHost:mc.example.net");
    expect(presenceLine(friend("1", "A", { state: "playing" }), t)).toBe("social.presence.playing");
    expect(presenceLine(friend("1", "A", { state: "launcher" }), t)).toBe(
      "social.presence.launcher",
    );
    expect(presenceLine(friend("1", "A", { lastSeen: "yesterday" }), t)).toBe(
      "social.presence.seen.yesterday",
    );
    expect(presenceLine(friend("1", "A"), t)).toBe("social.presence.offline");
  });

  it("adds the version a friend plays to every place in game, as is (SOC9)", () => {
    const playing = (game: Friend["game"]) => friend("1", "A", { state: "playing", game });
    expect(presenceLine(playing({ where: "projects", ver: "1.21.4" }), t)).toBe(
      "social.presence.projects · 1.21.4",
    );
    expect(
      presenceLine(playing({ where: "other", host: "mc.example.net", ver: "26.2" }), t),
    ).toBe("social.presence.otherHost:mc.example.net · 26.2");
    expect(presenceLine(playing({ where: "singleplayer", ver: "25w14a" }), t)).toBe(
      "social.presence.singleplayer · 25w14a",
    );
    expect(presenceLine(playing({ where: "other", ver: null }), t)).toBe("social.presence.other");
  });

  it("knows projects.gg hosts", () => {
    expect(isProjectsHost("play.projects.gg")).toBe(true);
    expect(isProjectsHost("projects.gg:25570")).toBe(true);
    expect(isProjectsHost("notprojects.gg")).toBe(false);
    expect(isProjectsHost(undefined)).toBe(false);
  });
});

describe("friend codes", () => {
  it("formats while typing", () => {
    expect(formatCodeInput("")).toBe("");
    expect(formatCodeInput("7k")).toBe("PRJ-7K");
    expect(formatCodeInput("prj 7k3m")).toBe("PRJ-7K3M");
    expect(formatCodeInput("PRJ-7K3M-Q9")).toBe("PRJ-7K3M-Q9");
    expect(formatCodeInput("7k3mq9tdEXTRA")).toBe("PRJ-7K3M-Q9TD");
    // Crockford: I and L read as 1, O as 0; U is not a symbol.
    expect(formatCodeInput("ilou")).toBe("PRJ-110");
  });

  it("accepts only complete codes", () => {
    expect(normalizeCode("prj-7k3m-q9td")).toBe("PRJ-7K3M-Q9TD");
    expect(normalizeCode("7K3MQ9TD")).toBe("PRJ-7K3M-Q9TD");
    expect(normalizeCode("PRJ-7K3M-Q9T")).toBeNull();
    expect(normalizeCode("hello")).toBeNull();
  });

  it("finds the code in a pasted link or message", () => {
    expect(formatCodeInput("https://social.projects.gg/f/PRJ-7K3M-Q9TD")).toBe("PRJ-7K3M-Q9TD");
    expect(formatCodeInput("projects://friend/prj-7k3m-q9td/")).toBe("PRJ-7K3M-Q9TD");
    expect(formatCodeInput("Kodum: PRJ-7K3M-Q9TD, ekle beni")).toBe("PRJ-7K3M-Q9TD");
    expect(normalizeCode("https://social.projects.gg/f/PRJ-7K3M-Q9TD")).toBe("PRJ-7K3M-Q9TD");
  });
});

describe("wantsNotice", () => {
  const prefs = { favorites: ["9"], notifyFavoriteOnline: true, notifyFavoriteProjects: false };
  const invite: SocialNotice = {
    kind: "invite",
    invite: { id: "i", from: "9", at: 1, quiet: false },
  };

  it("lets do-not-disturb silence everything but a sign-out", () => {
    expect(wantsNotice(invite, "dnd", prefs)).toBe(false);
    expect(wantsNotice({ kind: "friendOnline", id: "9" }, "dnd", prefs)).toBe(false);
    expect(wantsNotice({ kind: "signedOutRemotely" }, "dnd", prefs)).toBe(true);
  });

  it("only tells about favourites, when asked", () => {
    expect(wantsNotice({ kind: "friendOnline", id: "9" }, "online", prefs)).toBe(true);
    expect(wantsNotice({ kind: "friendOnline", id: "8" }, "online", prefs)).toBe(false);
    expect(wantsNotice({ kind: "friendOnProjects", id: "9" }, "online", prefs)).toBe(false);
    expect(wantsNotice(invite, "invisible", prefs)).toBe(true);
    expect(
      wantsNotice({ kind: "invite", invite: { ...invite.invite, quiet: true } }, "online", prefs),
    ).toBe(false);
  });

  it("lets the player choose for one friend, over the favourites' rule", () => {
    const chosen = { ...prefs, onlineAlerts: { "8": true, "9": false } };
    expect(notifiesOnline(chosen, "8")).toBe(true);
    expect(notifiesOnline(chosen, "9")).toBe(false);
    expect(notifiesOnline(chosen, "7")).toBe(false);
    expect(wantsNotice({ kind: "friendOnline", id: "8" }, "online", chosen)).toBe(true);
    expect(wantsNotice({ kind: "friendOnline", id: "9" }, "online", chosen)).toBe(false);
    // Do-not-disturb still wins, and joining projects.gg stays a favourites' notice.
    expect(wantsNotice({ kind: "friendOnline", id: "8" }, "dnd", chosen)).toBe(false);
    expect(wantsNotice({ kind: "friendOnProjects", id: "8" }, "online", chosen)).toBe(false);
    // Favourites follow the setting while nothing was chosen for them.
    expect(notifiesOnline({ ...prefs, notifyFavoriteOnline: false }, "9")).toBe(false);
  });

  it("throttles a flapping friend", () => {
    const throttle = new NoticeThrottle(60_000);
    expect(throttle.allow("9", 0)).toBe(true);
    expect(throttle.allow("9", 30_000)).toBe(false);
    expect(throttle.allow("8", 30_000)).toBe(true);
    expect(throttle.allow("9", 61_000)).toBe(true);
  });
});

/**
 * The service's own rule (services/social/src/settings.ts in the Studio repository): it refuses
 * these key words.
 */
const DENY = new Set([
  "password",
  "passwords",
  "passwd",
  "passphrase",
  "pwd",
  "sifre",
  "parola",
  "secret",
  "secrets",
  "token",
  "tokens",
  "cookie",
  "cookies",
  "session",
  "sessions",
  "credential",
  "credentials",
  "bearer",
  "authorization",
  "apikey",
  "privatekey",
  "otp",
  "totp",
  "afk",
]);

function keyWords(key: string): string[] {
  return key
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
}

function allKeys(value: unknown): string[] {
  if (!value || typeof value !== "object") return [];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) => [
    key,
    ...allKeys(child),
  ]);
}

describe("settings backup", () => {
  const prefs = {
    performanceMode: false,
    particles: true,
    sounds: false,
    soundVolume: 40,
    autoJoin: true,
    onGameStart: "tray",
    closeBehavior: "tray",
    updateChannel: "stable",
    autoDownloadUpdates: true,
    discordEnabled: true,
    discordShowServer: false,
    concurrentDownloads: 24,
    verifyOnLaunch: false,
    showSnapshots: true,
    // Never travels:
    selectedInstanceId: "projects",
    defaultJvmArgs: "-Xss2M",
    onboarded: true,
  } as Record<string, unknown>;
  const friends = {
    favorites: ["123456789012345678", "123456789012345678", "../x"],
    notifyFavoriteOnline: true,
    notifyFavoriteProjects: false,
  };

  it("sends an allow-list only, with names the service accepts", () => {
    const document = buildBackup(prefs, friends);
    expect(Object.keys(document.prefs).sort()).toEqual(Object.keys(BACKUP_PREFS).sort());
    expect(document.prefs).not.toHaveProperty("selectedInstanceId");
    expect(document.prefs).not.toHaveProperty("defaultJvmArgs");
    expect(document.friends.favorites).toEqual(["123456789012345678"]);
    for (const key of allKeys(document)) {
      const words = keyWords(key);
      expect(
        words.some((word) => DENY.has(word)),
        key,
      ).toBe(false);
      expect(key.toLowerCase().includes("logincommand"), key).toBe(false);
    }
    expect(JSON.stringify(document).length).toBeLessThan(64 * 1024);
  });

  it("reads back only known keys with valid values", () => {
    const read = readBackup({
      v: 1,
      prefs: {
        sounds: true,
        soundVolume: 500,
        onGameStart: "explode",
        concurrentDownloads: 16,
        updateChannel: "beta",
        selectedInstanceId: "evil",
      },
      friends: { favorites: ["1", 2, "x"], notifyFavoriteOnline: false, extra: true },
    });
    expect(read.prefs).toEqual({ sounds: true, concurrentDownloads: 16, updateChannel: "beta" });
    expect(read.friends).toEqual({ favorites: ["1"], notifyFavoriteOnline: false });
    expect(readBackup(null)).toEqual({ prefs: {}, friends: {} });
    expect(readBackup("nope")).toEqual({ prefs: {}, friends: {} });
  });

  it("brings the close choice back, but never the old tray switch", () => {
    // `closeToTray` was on by default, so an older backup's `true` was rarely a choice: the
    // computer keeps its own (TRAY1).
    expect(readBackup({ v: 1, prefs: { closeToTray: true } }).prefs).toEqual({});
    expect(readBackup({ v: 1, prefs: { closeBehavior: "tray" } }).prefs).toEqual({
      closeBehavior: "tray",
    });
    expect(readBackup({ v: 1, prefs: { closeBehavior: "exit" } }).prefs).toEqual({});
  });

  it("carries per-friend online notices only when there are some", () => {
    expect(buildBackup(prefs, friends).friends).not.toHaveProperty("onlineAlerts");
    const document = buildBackup(prefs, {
      ...friends,
      onlineAlerts: { "123456789012345678": true, "223456789012345678": false, "../x": true },
    });
    expect(document.friends.onlineAlerts).toEqual({
      "123456789012345678": true,
      "223456789012345678": false,
    });
    expect(
      readBackup({ friends: { onlineAlerts: { "1": false, "2": "yes", x: true } } }).friends,
    ).toEqual({ onlineAlerts: { "1": false } });
    expect(readBackup({ friends: { onlineAlerts: ["1"] } }).friends).toEqual({ onlineAlerts: {} });
  });

  it("fingerprints regardless of key order", () => {
    const a = buildBackup({ sounds: true, particles: false }, friends);
    const b = buildBackup({ particles: false, sounds: true }, friends);
    expect(backupFingerprint(a)).toBe(backupFingerprint(b));
    expect(backupFingerprint(a)).not.toBe(
      backupFingerprint(buildBackup({ sounds: false, particles: false }, friends)),
    );
  });
});
