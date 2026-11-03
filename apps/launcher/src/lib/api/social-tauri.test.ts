import { describe, expect, it } from "vitest";
import type { FriendView, SocialView } from "@/bindings";
import { snapshot } from "./social-tauri";

const friendView = (voice: FriendView["voice"]): FriendView => ({
  id: "300000000000000902",
  name: "PixelAvci",
  username: "PixelAvci",
  avatar: "discord",
  avatarUrl: null,
  since: "2026-11-03",
  lastSeen: null,
  state: "launcher",
  dnd: false,
  game: null,
  discordUrl: "https://discord.com/users/300000000000000902",
  discordId: "300000000000000902",
  badge: null,
  marks: [],
  voice,
});

// Only what `snapshot` reads; the rest of a real view does not matter here.
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

describe("snapshot", () => {
  // BATCH-206: the real app's friends lost `voice` on the way to the page, so "Sesli arama başlat"
  // stayed faded for every friend (the previews' mock API set it directly).
  it("keeps whether a friend may be called", () => {
    const [ready] = snapshot(view([friendView("ready")])).friends;
    expect(ready?.voice).toBe("ready");
    const [inRoom] = snapshot(view([friendView("in")])).friends;
    expect(inRoom?.voice).toBe("in");
  });

  it("leaves it out when they cannot be called", () => {
    const [friend] = snapshot(view([friendView(null)])).friends;
    expect(friend?.voice).toBeUndefined();
  });
});
