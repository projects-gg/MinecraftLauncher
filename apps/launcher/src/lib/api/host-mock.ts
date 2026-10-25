// Browser preview of world sharing (HOST1, "Dünyanı arkadaşlarına aç"): the player's own world
// opening, open and failing, a friend's world invite and every way its "Katıl" can go, kept in
// memory. Nothing leaves the page; the friends are the friends mock's (Ayşe, Mert, Elif Su…).
//
// Preview switches, read once from the page URL's query or the route's (`#/…?host=open`):
// - `host=<scenario>`:
//   (none) / `ready`: world sharing is on, no world is open, no invite waits;
//   `off`: it is off (nothing shows anywhere);
//   `open`: the player's "Yeni Dünya" (Projects) is open: Deniz in it, Mert on his way, Elif Su
//   invited; `full`: open with every place taken (7 guests in it);
//   `opening`: it is opening (`&step=backup|starting|publishing`, `&percent=<0-100>`);
//   `failed`: it did not open (`&reason=unavailable|limit`);
//   `invited`: Mert's world invite waits (the list, his row, Home); `quiet`: it came during
//   do-not-disturb (listed only); `invite`: it arrives live (`&inviteAt=<ms>`, 1500);
//   `joining`: "Katıl" is under way on it (`&stage=preparing|ready|connecting|in|lost`);
//   `ask`: it waits on a question (`&ask=copy|copyUnknown|install|name|nameTaken`);
//   `failedJoin`: the last "Katıl" ended (`&failure=full|unreachable|hostPackOld|mods`).
// - `join=<how>`: what "Katıl" does: `ok` (the default), `copy`, `copyUnknown`,
//   `install`, `name`, `nameTaken`, `full`, `closed`, `removed`, `unreachable`, `packOld`, `mods`
//   (after joining), `lost` (after joining), `slow` (it stays preparing), `missing` (two mods were
//   not found; it joins anyway).
// - `place=hidden`: no instance offers it; `place=playing`: the game runs Projects in "Yeni Dünya".
// - `open=fail` (`&reason=`): an opening fails at its last step; `open=slow`: each step waits longer.
// - `invites=daily|session|limited`: invites are refused.
// - `backup=0`: an opening needs no backup.
// - `max=<1-7>`: the guest limit (7).

import type {
  GuestState,
  HostApi,
  HostNotice,
  HostSession,
  HostView,
  InviteRefusal,
  JoinAnswer,
  JoinAsk,
  JoinFailure,
  JoinStage,
  OpenFailure,
  OpeningStep,
  WorldGuest,
  WorldInvite,
} from "./host-types";
import { mockConnectInGame } from "./mock-connect";

const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));
const nowSec = () => Math.floor(Date.now() / 1000);

function param(key: string): string | null {
  if (typeof window === "undefined") return null;
  const fromSearch = new URLSearchParams(window.location.search).get(key);
  const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
  return fromSearch ?? fromHash;
}

const scenario = () => param("host") ?? "ready";

/** The friends mock's people (same ids, names and pictures). */
const PEOPLE: Record<string, Omit<WorldGuest, "state" | "gameName"> & { gameName: string }> = {
  "301": { friend: "301", name: "Ayşe", avatar: "preset:fox", gameName: "AyseKaya" },
  "302": {
    friend: "302",
    name: "KorAteşi",
    avatar: "discord",
    avatarUrl: "https://cdn.discordapp.com/embed/avatars/2.png",
    gameName: "KorAtesi",
  },
  "303": { friend: "303", name: "Mert", avatar: "preset:knight", gameName: "MertYldz" },
  "304": { friend: "304", name: "Deniz", avatar: "preset:wizard", gameName: "Denizz" },
  "305": { friend: "305", name: "Elif Su", avatar: "preset:axolotl", gameName: "ElifSu" },
  "306": {
    friend: "306",
    name: "Bora",
    avatar: "discord",
    avatarUrl: "https://cdn.discordapp.com/embed/avatars/0.png",
    gameName: "Bora_mc",
  },
  "307": { friend: "307", name: "Zeynep", avatar: "preset:wolf", gameName: "Zeyno" },
  "308": { friend: "308", name: "Can Ejder", avatar: "preset:ember-dragon", gameName: "CanEjder" },
  "309": { friend: "309", name: "Selin", avatar: "discord", gameName: "SelinAk" },
};

function guest(id: string, state: GuestState): WorldGuest {
  const person = PEOPLE[id] ?? { friend: id, name: id, avatar: "discord", gameName: id };
  const { gameName, ...rest } = person;
  return { ...rest, state, ...(state === "in" ? { gameName } : {}) };
}

const WORLD = { instanceId: "projects", folder: "Yeni Dünya", worldName: "Yeni Dünya" };
const MAX = Math.min(7, Math.max(1, Number(param("max") ?? 7) || 7));

/** Mert's world session (the invite the guest previews answer). */
const MERT_SESSION = "mert-session";

function mertInvite(extra: Partial<WorldInvite> = {}): WorldInvite {
  return {
    session: MERT_SESSION,
    from: "303",
    name: "Mert",
    avatar: "preset:knight",
    mc: param("join") === "install" || param("ask") === "install" ? "26.3" : "26.1.2",
    guests: 1,
    max: MAX,
    at: nowSec() - 40,
    quiet: false,
    ...extra,
  };
}

function askOf(kind: string | null): JoinAsk {
  switch (kind) {
    case "copyUnknown":
      return { kind: "copy", mods: 12, bytes: 85_400_000, unknown: 2 };
    case "install":
      return { kind: "install", mc: "26.3" };
    case "name":
      return { kind: "name", verdict: "own", name: "EjderAvcisi" };
    case "nameTaken":
      return { kind: "name", verdict: "taken", name: "EjderAvcisi" };
    default:
      return { kind: "copy", mods: 12, bytes: 85_400_000, unknown: 0 };
  }
}

function stageOf(kind: string | null): JoinStage {
  switch (kind) {
    case "ready":
      return { kind: "ready", instanceId: "projects", port: 49_152, missing: 0 };
    case "connecting":
      return { kind: "connecting" };
    case "in":
      return { kind: "in" };
    case "lost":
      return { kind: "lost" };
    default:
      return { kind: "preparing", percent: kind === "download" ? 42 : undefined };
  }
}

function openSession(guests: WorldGuest[]): HostSession {
  return { ...WORLD, phase: "open", mode: "survival", commands: false, guests, max: MAX };
}

function initialSession(): HostSession | null {
  switch (scenario()) {
    case "open":
      return openSession([guest("304", "in"), guest("303", "joining"), guest("305", "invited")]);
    case "full":
      return openSession(
        ["301", "302", "303", "304", "305", "306", "307"].map((id) => guest(id, "in")),
      );
    case "opening": {
      const step = (param("step") ?? "starting") as OpeningStep;
      const percent = Number(param("percent") ?? 38);
      return {
        ...WORLD,
        phase: "opening",
        step,
        ...(step === "backup" ? { percent } : {}),
        guests: [guest("304", "pending"), guest("303", "pending"), guest("305", "pending")],
        max: MAX,
      };
    }
    case "failed":
      return {
        ...WORLD,
        phase: "failed",
        reason: (param("reason") ?? "unavailable") as OpenFailure,
        guests: [guest("301", "pending"), guest("303", "pending")],
        max: MAX,
      };
    default:
      return null;
  }
}

function initialInvites(): WorldInvite[] {
  switch (scenario()) {
    case "invited":
      return [mertInvite()];
    case "quiet":
      return [mertInvite({ quiet: true })];
    case "joining":
      return [mertInvite({ join: stageOf(param("stage")) })];
    case "ask":
      return [mertInvite({ join: { kind: "ask", ask: askOf(param("ask")) } })];
    case "failedJoin": {
      const failure = (param("failure") ?? "full") as JoinFailure;
      return [
        mertInvite({
          join: { kind: "failed", failure },
          // A full world counts everyone it holds (8/8), as the live count would.
          ...(failure === "full" ? { guests: MAX - 1 } : {}),
        }),
      ];
    }
    default:
      return [];
  }
}

let view: HostView = {
  seq: 1,
  enabled: scenario() !== "off",
  maxGuests: MAX,
  session: initialSession(),
  invites: initialInvites(),
};

const changeListeners = new Set<(view: HostView) => void>();
const noticeListeners = new Set<(notice: HostNotice) => void>();

function emit(next: Partial<Omit<HostView, "seq">>) {
  view = { ...view, ...next, seq: view.seq + 1 };
  for (const listener of changeListeners) listener(view);
}

function notice(event: HostNotice) {
  for (const listener of noticeListeners) listener(event);
}

function setSession(session: HostSession | null) {
  emit({ session });
}

function setGuests(update: (guests: WorldGuest[]) => WorldGuest[]) {
  if (!view.session) return;
  setSession({ ...view.session, guests: update(view.session.guests) } as HostSession);
}

function setJoin(session: string, join: JoinStage | undefined) {
  emit({
    invites: view.invites.map((invite) =>
      invite.session === session ? { ...invite, join } : invite,
    ),
  });
}

function joinOf(session: string): JoinStage | undefined {
  return view.invites.find((invite) => invite.session === session)?.join;
}

/** One run of a join; a newer one (or "Vazgeç") makes the older one stop where it is. */
const joinRuns = new Map<string, number>();

function alive(session: string, run: number): boolean {
  return joinRuns.get(session) === run;
}

const slow = () => param("open") === "slow";

/** The opening of a session, one at a time. */
let openRun = 0;

/** After the opening's invites went out: friends arrive one by one, as they would. */
async function arrive(run: number, ids: string[]) {
  for (const id of ids.slice(0, 2)) {
    await delay(2600);
    if (run !== openRun) return;
    setGuests((guests) => guests.map((g) => (g.friend === id ? guest(id, "joining") : g)));
    await delay(1500);
    if (run !== openRun) return;
    setGuests((guests) => guests.map((g) => (g.friend === id ? guest(id, "in") : g)));
  }
}

async function finishOpening(run: number, ids: string[]) {
  await delay(slow() ? 4000 : 6500);
  if (run !== openRun || view.session?.phase !== "opening") return;
  setSession({ ...view.session, step: "publishing", percent: undefined });
  await delay(slow() ? 3000 : 1100);
  if (run !== openRun || view.session?.phase !== "opening") return;
  if (param("open") === "fail") {
    const reason = (param("reason") ?? "unavailable") as OpenFailure;
    setSession({ ...view.session, phase: "failed", reason });
    notice({ kind: "openFailed", reason });
    return;
  }
  const refused = param("invites") as InviteRefusal | null;
  setSession(
    openSession(ids.map((id) => guest(id, refused ? "pending" : "invited"))) satisfies HostSession,
  );
  notice({ kind: "opened", worldName: WORLD.worldName });
  if (refused) {
    notice({ kind: "inviteRefused", why: refused });
    return;
  }
  void arrive(run, ids);
}

/** The world's way and install, prepared step by step; `download` counts a percent up. */
async function prepare(session: string, run: number, download: boolean): Promise<boolean> {
  if (!download) {
    setJoin(session, { kind: "preparing" });
    await delay(1100);
    return alive(session, run);
  }
  for (let percent = 0; percent <= 100; percent += 4) {
    if (!alive(session, run)) return false;
    setJoin(session, { kind: "preparing", percent });
    await delay(90);
  }
  return alive(session, run);
}

function fail(session: string, failure: JoinFailure) {
  const invite = view.invites.find((one) => one.session === session);
  const gone = failure === "closed" || failure === "removed";
  if (gone) emit({ invites: view.invites.filter((one) => one.session !== session) });
  else setJoin(session, { kind: "failed", failure });
  notice({ kind: "joinFailed", session, name: invite?.name ?? "", failure });
}

/** The way after the decisions: ready, then (once the game came) connecting and in. */
async function ready(session: string, run: number, how: string) {
  setJoin(session, {
    kind: "ready",
    instanceId: WORLD.instanceId,
    port: 49_152,
    missing: how === "missing" ? 2 : 0,
  });
  // The preview's game opens in about six seconds (mock.ts); then it connects through the way.
  await delay(6800);
  if (!alive(session, run) || joinOf(session)?.kind !== "ready") return;
  setJoin(session, { kind: "connecting" });
  await delay(1200);
  if (!alive(session, run)) return;
  setJoin(session, { kind: "in" });
  emit({
    invites: view.invites.map((one) =>
      one.session === session ? { ...one, guests: one.guests + 1 } : one,
    ),
  });
  if (how === "mods") {
    await delay(2600);
    if (alive(session, run)) fail(session, "mods");
  } else if (how === "lost") {
    await delay(3200);
    if (alive(session, run)) setJoin(session, { kind: "lost" });
  }
}

const FAILS: Record<string, JoinFailure> = {
  full: "full",
  closed: "closed",
  removed: "removed",
  unreachable: "unreachable",
  packOld: "hostPackOld",
};

async function runJoin(session: string, how: string) {
  const run = (joinRuns.get(session) ?? 0) + 1;
  joinRuns.set(session, run);
  if (!(await prepare(session, run, false))) return;
  if (how === "slow") return;
  const failure = FAILS[how];
  if (failure) {
    fail(session, failure);
    return;
  }
  if (
    ["copy", "copyUnknown", "install", "name", "nameTaken"].includes(how) &&
    joinOf(session)?.kind === "preparing"
  ) {
    setJoin(session, { kind: "ask", ask: askOf(how) });
    return;
  }
  await ready(session, run, how);
}

/** The live invite of `host=invite` arrives once, however often the view is asked for. */
let liveInvite = false;

export const mockHost: HostApi = {
  async state() {
    await delay(60);
    if (scenario() === "invite" && !liveInvite) {
      liveInvite = true;
      const at = Number(param("inviteAt") ?? 1500);
      window.setTimeout(() => {
        const invite = mertInvite({ at: nowSec() });
        emit({
          invites: [invite, ...view.invites.filter((one) => one.session !== invite.session)],
        });
        notice({ kind: "invite", invite });
      }, at);
    }
    return view;
  },
  onChange(listener) {
    changeListeners.add(listener);
    return () => changeListeners.delete(listener);
  },
  onNotice(listener) {
    noticeListeners.add(listener);
    return () => noticeListeners.delete(listener);
  },
  async place(instanceId) {
    await delay(80);
    const where = param("place");
    if (where === "hidden" || !view.enabled || instanceId !== WORLD.instanceId) {
      return { shown: false };
    }
    return where === "playing" ? { shown: true, playing: WORLD.folder } : { shown: true };
  },
  async open(request) {
    openRun += 1;
    const run = openRun;
    const guests = request.invite.map((id) => guest(id, "pending"));
    const base = {
      instanceId: request.instanceId,
      folder: request.folder,
      worldName: request.folder,
      guests,
      max: MAX,
    };
    const running = param("place") === "playing" && request.folder === WORLD.folder;
    if (running) {
      setSession({ ...base, phase: "opening", step: "publishing" });
      void finishOpening(run, request.invite);
      return "running";
    }
    if (param("backup") !== "0") {
      for (let percent = 0; percent <= 100; percent += 5) {
        if (run !== openRun) return "cancelled";
        setSession({ ...base, phase: "opening", step: "backup", percent });
        await delay(slow() ? 220 : 70);
      }
    }
    setSession({ ...base, phase: "opening", step: "starting" });
    void finishOpening(run, request.invite);
    return "start";
  },
  async cancelOpen() {
    openRun += 1;
    setSession(null);
  },
  async invite(friends) {
    await delay(350);
    const refused = param("invites") as InviteRefusal | null;
    if (refused) return refused;
    setGuests((guests) => [
      ...guests,
      ...friends
        .filter((id) => !guests.some((g) => g.friend === id))
        .map((id) => guest(id, "invited")),
    ]);
    return "sent";
  },
  async remove(friend) {
    await delay(250);
    setGuests((guests) => guests.filter((g) => g.friend !== friend));
  },
  async stop() {
    await delay(300);
    openRun += 1;
    setSession(null);
  },
  async dismissFailure() {
    setSession(null);
  },
  async meetOnProjects() {
    await delay(400);
    if (view.session?.phase === "failed") setSession(null);
  },
  async join(session, options) {
    await delay(120);
    const how = options?.copy ? "copy" : (param("join") ?? "ok");
    void runJoin(session, how);
  },
  async answer(session, answer: JoinAnswer) {
    await delay(150);
    const run = (joinRuns.get(session) ?? 0) + 1;
    joinRuns.set(session, run);
    const download = answer.kind !== "account";
    if (!(await prepare(session, run, download))) return;
    await ready(session, run, param("join") ?? "ok");
  },
  async cancelJoin(session) {
    joinRuns.set(session, (joinRuns.get(session) ?? 0) + 1);
    setJoin(session, undefined);
  },
  async dismissInvite(session) {
    joinRuns.set(session, (joinRuns.get(session) ?? 0) + 1);
    emit({ invites: view.invites.filter((invite) => invite.session !== session) });
  },
  connectInGame: () => mockConnectInGame(),
};
