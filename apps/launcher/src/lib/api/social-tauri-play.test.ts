import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FriendView, SocialView } from "@/bindings";

// CREW-PLUS: what the app's adapter does with a friend's play time and "Oynama Süremi Göster".
const calls = vi.hoisted(() => ({ privacy: [] as unknown[][], playTime: [] as unknown[][] }));
vi.mock("@/bindings", async (importOriginal) => {
  const real = await importOriginal<typeof import("@/bindings")>();
  const done = { status: "ok" as const, data: null };
  return {
    ...real,
    commands: {
      ...real.commands,
      socialSetPrivacy: async (...args: unknown[]) => {
        calls.privacy.push(args);
        return done;
      },
      socialSetChatPrivacy: async () => done,
      socialSetPlayTimePrivacy: async (...args: unknown[]) => {
        calls.playTime.push(args);
        return done;
      },
    },
  };
});

const { snapshot, tauriSocial } = await import("./social-tauri");

const friend = (playingSince: number | null): FriendView => ({
  id: "300000000000000903",
  name: "GeceKurdu_",
  username: "GeceKurdu_",
  avatar: "discord",
  avatarUrl: null,
  since: "2026-11-03",
  lastSeen: null,
  state: "playing",
  dnd: false,
  game: { where: "projects", ver: "1.21.4" },
  discordUrl: "https://discord.com/users/300000000000000903",
  discordId: "300000000000000903",
  badge: null,
  marks: [],
  voice: null,
  playingSince,
});

// Only what `snapshot` reads of the friends; the rest of a real view does not matter here.
const view = (friends: FriendView[]): SocialView =>
  ({
    seq: 1,
    enabled: true,
    auth: "signedIn",
    connection: "online",
    me: null,
    loaded: true,
    friends,
    incoming: [],
    outgoing: [],
    blocked: [],
    invites: [],
    activity: { state: "launcher" },
    groups: null,
    teams: true,
    teamFeatures: [],
    crews: [],
    crewLimit: 3,
    teamInvites: [],
    teamRequests: [],
    plans: [],
    together: [],
    calls: [],
    callPlace: null,
    crewNotifications: null,
    chatAllowed: true,
    chat: null,
    notices: [],
    restriction: null,
    backupWaitsUntil: null,
    crewJoins: [],
    markDefs: [],
    providers: [],
    signUpWays: [],
    providersKnown: true,
    signingInWith: null,
    identityChange: null,
  }) as unknown as SocialView;

describe("a friend's play time (CREW-PLUS)", () => {
  beforeEach(() => {
    calls.privacy.length = 0;
    calls.playTime.length = 0;
  });

  it("keeps when their session began, and nothing when the service sends none", () => {
    const [shown] = snapshot(view([friend(1_792_400_100)])).friends;
    expect(shown?.playingSince).toBe(1_792_400_100);
    const [hidden] = snapshot(view([friend(null)])).friends;
    expect(hidden?.playingSince).toBeUndefined();
  });

  it('sends "Oynama Süremi Göster" on its own command, and only when it changes', async () => {
    await tauriSocial.setPrivacy({ sharePlayTime: false });
    expect(calls.playTime).toEqual([[false]]);
    expect(calls.privacy).toEqual([]);
    await tauriSocial.setPrivacy({ shareLastSeen: true });
    expect(calls.playTime).toEqual([[false]]);
    expect(calls.privacy).toEqual([[null, true]]);
  });
});
