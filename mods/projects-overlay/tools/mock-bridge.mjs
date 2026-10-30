// Plays the launcher's side of the overlay bridge for local tests, with made-up sample data.
// It listens on 127.0.0.1 only and never connects anywhere.
//
//   node tools/mock-bridge.mjs [--port 0] [--token <t>] [--info <file>] [--log <file>] [--auto-demo]
//                              [--reduced-motion] [--signed-out] [--quiet] [--chat2] [--team6]
//
// --info writes {"port","token"} once listening, for the script that starts the game.
// --chat2 plays a launcher with CHAT2's extras (docs/tasks/ovl2-protocol.md, CHAT2-O): message ids (sid),
// stickers, replies, reactions, links and cards, "Görüldü", "yazıyor…"; the panel's stickers, replies and
// reactions are answered as the launcher would (a reaction with 😮 is refused as "full", for the note).
// The game's scenario driver sends {"t":"dev","cmd":...} frames to script the scenario; the real launcher
// ignores unknown frame types, so these exist only here. Frames follow ovl1.md §3 and the additions in
// docs/tasks/ovl2-protocol.md (L1-L16, and "connect" with its "connect-result").
// {"t":"dev","cmd":"connect","host":"localhost","port":25599} makes the mock send the launcher's "Bağlan".
// Each mod frame is written to --log, so a run's connect-result answers can be checked afterwards.
// --team6 plays crew tags (docs/tasks/ovl2-protocol.md, TEAM6-O): "tags" in the welcome, the cards ("badges")
// before the snapshot, friends', crew messages' and toasts' "badge", the crews' "ekibe katıldı" lines ("note");
// dev commands "joined", "tag-change", "hide-crew", "tags-off" and "tags-on" play the live changes; "tag-text" (with
// "crew" and "tag") renames a crew's tag.
// --people plays the friend menu (docs/tasks/ovl2-protocol.md, OVL-UX): "people" and "invite" in the welcome (the
// player is on Projects), friends' "dm" and "discord", the player's own tag in "me" (with --team6); the menu's frames
// (invite, discord, unfriend, block, report) are answered with "friend-result" as the launcher would (an invite to an
// offline friend waits for them, a second one to Mert is too soon; unfriend and block take the friend off the list). A
// message to a conversation that had none yet makes it real ("conv"). Dev commands "badge-off" / "badge-on" take the
// player's own tag off and on (a new welcome), "invite-off" leaves Projects.
// --people2 (with --people) plays the rest of the launcher's person menu (docs/tasks/ovl2-protocol.md, OVL-UX2):
// "people2" in the welcome, the "lists" frame (two categories, two crews the player may invite to, one of them full,
// crewmates' friend requests: Cem asked the player; Cem plays on Projects and his Discord profile is known),
// friends' "fav", "alert", "group", "crews" and offline ones'
// "seen", crew messages' "by" (and a message from Cem, a crewmate who is not a friend). "favorite" and "alert" come
// back as the friend's frame, "move", "crew-invite" and "add-friend" also with "friend-result"; "block" and "report"
// take a crewmate too.
// --people3 (with --people2) plays OVL-UX3: "people3" in the welcome, the player's rights in Gece Kuşları in the
// lists' "roles" (the player leads it, roles on, Deniz moderates; its chat carries the rows), "crew-mod", "crew-lead"
// and "crew-remove" answered with "friend-result" and the new rights (a hand-over leaves the player a moderator who
// may not remove anyone; a removal takes them out), and a "move" with a "name" makes the category and moves them.
// --link plays codeless Discord linking (docs/tasks/ovl2-protocol.md, SKIN1-O): "link" in the welcome; a mod's
// "link-want" gets a made-up permit ("link-offer") for that name, or "link-clear" for the name "Nobody".
// --host plays world sharing's host launcher (docs/tasks/ovl2-protocol.md, HOST1, H1-H17): "host" in the welcome;
// "host-request" is answered with "host-publish", "host-published" with the session's mirror ("host-state"), and
// then the gate is probed as the host launcher's local connections would reach it (PLG1 headers, crates/pl-host
// PROTOCOL.md section 7): a connection without a header, a header made with another key, the host's own name, a
// guest logging in for real (to Login Success) and a reused connection number. Each probe's outcome goes to the log
// as {"dir":"info","frame":{"t":"probe",...}}. --relay-as <name> also opens a relay port (in the --info file as
// "relayPort") that puts a header for <name> in front of every connection and pipes it to the world, so a real
// client can join as a guest. --relay <guest>:<friend>:<name>[,...] opens one such port per guest identity
// ("relayPorts" in the --info file, in order; friend is ayse, mert or deniz; --relay-as <name> is g1:ayse:<name>).
// Dev commands: "world-invite", "world-connect", "host-mirror", "host-fail", "host-refuse" (the next opening),
// "host-stop", "host-remove".
// --lang tr|en plays the overlay's language (docs/tasks/ovl2-protocol.md, OVL-LANG, G1-G4) with this as the launcher's
// own language: a mod whose hello lists "lang" hears "lang" in every welcome (the player's pick, else the launcher's),
// its "lang" pick is kept (in --lang-file <file> when given, "tr", "en" or "auto", as the launcher's overlay-language
// file; read at the start, so a second run starts where the first left) and answered with a new welcome in it; the
// mock's own words for the mod (a sticker in words) follow it. Dev commands "launcher-lang" (with "lang": the
// launcher's language changes; the overlay follows while nothing is picked) and "clear-pick". --welcome-delay <ms>
// greets that much later (under the mod's 5 s handshake wait), to see the panel before the launcher said anything.
// --emojis [file] plays the chat's emoji picker (docs/tasks/ovl2-protocol.md, CHAT-EMOJI, E1-E4): a mod whose hello lists
// "emoji" hears "emojis" after the greeting (the switch and the list: the given file, else the launcher's built-in one,
// crates/pl-content/data/chat-emojis.json); --emoji-off starts with the switch off. The mod's "emoji" switch is kept and
// answered with "emojis". Dev commands "emojis-off" / "emojis-on" switch it as the launcher's Settings would, "emojis-list"
// (with "count") sends a shorter list as a Studio publish would. A mod whose hello lists "panel-off" (the picker alone)
// hears only "welcome" and "emojis", as the launcher's quiet session.

import { createHash, createHmac, randomBytes, timingSafeEqual } from "node:crypto";
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import net from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const PROTO = 1;
// Launcher frames may carry a snapshot or a history; the mod's frames are small player actions.
const MAX_OUT_FRAME = 256 * 1024;
const MAX_IN_FRAME = 16 * 1024;
const MAX_TEXT = 300;
const HELLO_TIMEOUT_MS = 3000;

const args = parseArgs(process.argv.slice(2));
const token = args.token ?? randomBytes(32).toString("base64url");
const started = Date.now();
let port = Number(args.port ?? 0);
let server = null;
let session = null;
let nextId = 100;

// OVL-LANG: the launcher's own language, the player's pick in the overlay (kept in --lang-file), and whether the
// connected mod's words follow the launcher (its hello lists "lang").
const langs = {
  launcher: args.lang === "en" || args.lang === "tr" ? args.lang : null,
  pick: readPick(),
  follows: false,
};
function readPick() {
  if (typeof args.langFile !== "string") {
    return null;
  }
  try {
    const text = readFileSync(args.langFile, "utf8").trim();
    return text === "tr" || text === "en" ? text : null;
  } catch {
    return null;
  }
}
function keepPick() {
  if (typeof args.langFile === "string") {
    writeFileSync(args.langFile, langs.pick ?? "auto");
  }
}
// CHAT-EMOJI: the picker's switch and list, and what the connected mod is (it has the picker; it runs it alone).
const emojis = {
  on: !args.emojiOff,
  doc: readEmojis(),
  hears: false,
  quiet: false,
};
function readEmojis() {
  const file =
    typeof args.emojis === "string"
      ? args.emojis
      : join(
          dirname(fileURLToPath(import.meta.url)),
          "../../../crates/pl-content/data/chat-emojis.json",
        );
  try {
    return JSON.parse(readFileSync(file, "utf8"));
  } catch {
    return { hosts: [], emojis: [] };
  }
}
function sendEmojis(socket, count) {
  if (!args.emojis || !emojis.hears) {
    return;
  }
  const list = count === undefined ? emojis.doc.emojis : emojis.doc.emojis.slice(0, count);
  send(socket, { t: "emojis", on: emojis.on, hosts: emojis.doc.hosts, emojis: list });
}

/** The overlay's language: the player's pick, else the launcher's (null: this mock plays no language). */
function overlayLang() {
  return langs.pick ?? langs.launcher;
}
/** The mock's own words for the mod in English (a mod that follows the launcher, which speaks English now). */
function inEnglish() {
  return langs.follows && overlayLang() === "en";
}

const now = () => Date.now();
const minutes = (m) => now() - m * 60000;
const state = {
  me: { name: args.signedOut ? "" : "OVL2Test" },
  // Shown names are gamer tags (owner, 28 Oct: no real names on screen); ids keep the old first names: f-ayse PixelAvci,
  // f-elif ZumrutKilic, f-zeynep YildizTozu, f-deniz MaviElmas, f-mert NetherKral, f-can KorAtesi, f-ece SessizOkcu,
  // u-cem DemirGolem_; a joined note's default TasDevi.
  friends: [
    { id: "f-ayse", name: "PixelAvci", where: "projects", joinable: true },
    { id: "f-elif", name: "ZumrutKilic", where: "server", joinable: true, host: "mc.ornek.net" },
    { id: "f-zeynep", name: "YildizTozu", where: "single", joinable: false },
    { id: "f-deniz", name: "MaviElmas", where: "game", joinable: false },
    { id: "f-mert", name: "NetherKral", where: "launcher", joinable: false },
    { id: "f-can", name: "KorAtesi", where: "offline", joinable: false },
    { id: "f-ece", name: "SessizOkcu", where: "offline", joinable: false },
  ],
  convs: [
    { id: "dm:ayse", kind: "dm", title: "PixelAvci", unread: 2, muted: false, writable: true },
    {
      id: "crew:gece",
      kind: "crew",
      title: "Gece Kuşları",
      unread: 0,
      muted: false,
      writable: true,
    },
    { id: "dm:mert", kind: "dm", title: "NetherKral", unread: 0, muted: false, writable: true },
    { id: "dm:zeynep", kind: "dm", title: "YildizTozu", unread: 1, muted: false, writable: true },
    { id: "crew:akinci", kind: "crew", title: "Akıncılar", unread: 0, muted: true, writable: true },
    { id: "dm:can", kind: "dm", title: "KorAtesi", unread: 0, muted: false, writable: false },
  ],
  history: {
    "dm:ayse": [
      {
        id: "m1",
        from: "OVL2Test",
        text: "Bugün bedwars turnuvası var mıydı?",
        at: minutes(42),
        mine: true,
      },
      {
        id: "m2",
        from: "PixelAvci",
        text: "Evet, 21:00'de başlıyor.",
        at: minutes(40),
        mine: false,
      },
      {
        id: "m3",
        from: "PixelAvci",
        text: "Ekip toplanıyor, sen de gel!",
        at: minutes(39),
        mine: false,
        edited: true,
      },
      {
        id: "m4",
        from: "PixelAvci",
        text: "Selam! Bu akşam oynuyor musun?",
        at: minutes(3),
        mine: false,
      },
      {
        id: "m5",
        from: "PixelAvci",
        text: "Haritayı bu sefer ben seçeceğim.",
        at: minutes(2),
        mine: false,
      },
    ],
    "crew:gece": [
      {
        id: "m6",
        from: "NetherKral",
        text: "Yarın 21:00'de antrenman var, unutmayın.",
        at: minutes(90),
        mine: false,
      },
      {
        id: "m7",
        from: "MaviElmas",
        text: "Ben gelemeyebilirim, haber veririm.",
        at: minutes(80),
        mine: false,
      },
      { id: "m8", from: "OVL2Test", text: "Tamam, ben oradayım.", at: minutes(75), mine: true },
    ],
    "dm:mert": [
      {
        id: "m9",
        from: "NetherKral",
        text: "Kılıcı sende mi bıraktım?",
        at: minutes(300),
        mine: false,
      },
    ],
    "dm:zeynep": [
      {
        id: "m10",
        from: "YildizTozu",
        text: "Adamı gördün mü az önce?",
        at: minutes(1),
        mine: false,
      },
    ],
    "crew:akinci": [],
    "dm:can": [{ id: "m11", from: "KorAtesi", text: "Görüşürüz!", at: minutes(4000), mine: false }],
  },
  calls: [],
};

// CHAT2: the same conversations with the extras a CHAT2 launcher sends (each message's sid is its order key).
const EXTRAS = ["replies", "reactions", "stickers", "typing", "reads", "links"];
let nextSid = 1000;
const newSid = () => nextSid++;
if (args.chat2) {
  const ayse = state.convs.find((c) => c.id === "dm:ayse");
  ayse.unread = 0;
  state.history["dm:ayse"] = [
    {
      id: "m1",
      sid: 101,
      from: "OVL2Test",
      text: "Bugün bedwars turnuvası var mıydı?",
      at: minutes(42),
      mine: true,
    },
    {
      id: "m2",
      sid: 102,
      from: "PixelAvci",
      text: "Evet, 21:00'de başlıyor. Kurallar burada: https://www.youtube.com/watch?v=dQw4w9WgXcQ",
      at: minutes(40),
      mine: false,
      links: [{ url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ", trusted: true }],
      card: { site: "YouTube", title: "Bedwars turnuva kuralları (2026)" },
      reactions: [{ e: "👍", n: 1, mine: true }],
    },
    {
      id: "m3",
      sid: 103,
      from: "PixelAvci",
      text: "",
      sticker: "pl1/gg",
      at: minutes(39),
      mine: false,
    },
    {
      id: "m4",
      sid: 104,
      from: "OVL2Test",
      text: "Tamam, ben de geliyorum!",
      at: minutes(3),
      mine: true,
      reply: {
        id: "m2",
        sid: 102,
        from: "PixelAvci",
        text: "Evet, 21:00'de başlıyor. Kurallar burada:…",
      },
      reactions: [
        { e: "❤️", n: 1, who: ["PixelAvci"] },
        { e: "🔥", n: 1, who: ["PixelAvci"] },
      ],
    },
    {
      id: "m5",
      sid: 105,
      from: "OVL2Test",
      text: "Haritayı bu sefer ben seçeceğim.",
      at: minutes(2),
      mine: true,
    },
  ];
  // Ayşe saw up to m4: "Görüldü" sits under it until she writes.
  ayse.seen = 104;
  state.history["crew:gece"] = [
    {
      id: "m6",
      sid: 106,
      from: "NetherKral",
      text: "Yarın 21:00'de antrenman var, unutmayın.",
      at: minutes(90),
      mine: false,
      reactions: [
        { e: "👍", n: 4, mine: true, who: ["MaviElmas", "PixelAvci", "KorAtesi"] },
        { e: "🔥", n: 2, who: ["MaviElmas", "ZumrutKilic"] },
        { e: "😂", n: 1, who: ["KorAtesi"] },
      ],
    },
    {
      id: "m7",
      sid: 107,
      from: "MaviElmas",
      text: "Ben gelemeyebilirim, haber veririm.",
      at: minutes(80),
      mine: false,
      reply: {
        id: "m6",
        sid: 106,
        from: "NetherKral",
        text: "Yarın 21:00'de antrenman var, unutmayın.",
      },
    },
    {
      id: "m7b",
      sid: 108,
      from: "MaviElmas",
      text: "",
      sticker: "pl1/sleepy",
      at: minutes(79),
      mine: false,
    },
    {
      id: "m8",
      sid: 109,
      from: "OVL2Test",
      text: "Tamam, ben oradayım.",
      at: minutes(75),
      mine: true,
      reply: { sid: 90, from: "", missing: true },
    },
  ];
}

// TEAM6: the cards of the tags friends and crewmates wear, who wears which, and the crews' join lines.
const CARDS = {
  "t-gk": {
    tag: "GK",
    color: "navy",
    frame: "shield",
    emblem: "moon",
    name: "Gece Kartalları",
    motto: "Gece bizimdir",
    since: "2026-09",
    members: 12,
  },
  "t-gece": {
    tag: "GKŞ",
    color: "gold",
    frame: "cut",
    emblem: "flame",
    name: "Gece Kuşları",
    motto: "Akşam 9'da Bedwars, sesli gelin",
    since: "2026-03",
    members: 8,
    mine: true,
  },
  "t-akn": {
    tag: "AKN",
    color: "crimson",
    frame: "banner",
    emblem: "sword",
    name: "Akıncılar",
    since: "2025-11",
    members: 30,
    mine: true,
  },
};
const WEARS = {
  "f-ayse": "t-gk",
  "f-can": "t-gk",
  "f-mert": "t-gece",
  "f-deniz": "t-gece",
  "f-elif": "t-akn",
};
const NAMES = {
  PixelAvci: "f-ayse",
  KorAtesi: "f-can",
  NetherKral: "f-mert",
  MaviElmas: "f-deniz",
  ZumrutKilic: "f-elif",
};
const NOTES = [];
// OVL-UX: friends' direct conversations (the mock's own ids), whose Discord is known, and whether the player is on
// Projects ("invite").
let inviteOn = Boolean(args.people);
const invitedOnce = new Set();
if (args.people) {
  for (const friend of state.friends) {
    friend.dm = `dm:${friend.id.slice(2)}`;
    if (friend.id !== "f-can") {
      friend.discord = true;
    }
  }
  if (args.team6) {
    state.me.badge = "t-gece";
  }
}
// OVL-UX2: the person menu's lists, the friends' marks and crews, and a crewmate who is not a friend.
const LISTS = {
  groups: [
    { id: "okul", name: "Okul" },
    { id: "pvp", name: "PvP" },
  ],
  crews: [
    { id: "t-gece", name: "Gece Kuşları" },
    { id: "t-akn", name: "Akıncılar", full: true },
  ],
  crewmates: true,
  asked: ["u-cem"],
  asking: [],
  // Crewmates who are not friends, on Projects now and with a known Discord profile ("Katıl", Discord).
  projects: ["u-cem"],
  discord: ["u-cem"],
};
const CREWMATES = { "u-cem": "DemirGolem_" };
if (args.people2 && args.people3) {
  // OVL-UX3: the player leads Gece Kuşları (its chat carries the crew rows); Deniz moderates.
  LISTS.roles = {
    "t-gece": {
      lead: true,
      roles: true,
      conv: "crew:gece",
      mates: {
        "f-mert": { kick: true },
        "f-deniz": { mod: true, kick: true },
        "u-cem": { kick: true },
      },
    },
  };
}
if (args.people2) {
  const mark = (id, fields) =>
    Object.assign(
      state.friends.find((f) => f.id === id),
      fields,
    );
  mark("f-ayse", { fav: true, alert: true, group: "okul" });
  mark("f-elif", { crews: { "t-akn": "member" } });
  mark("f-deniz", { crews: { "t-gece": "member" } });
  mark("f-mert", { crews: { "t-gece": "member" } });
  mark("f-can", { seen: "yesterday", group: "okul" });
  mark("f-ece", { seen: "week" });
  const byName = { NetherKral: "f-mert", MaviElmas: "f-deniz", DemirGolem_: "u-cem" };
  state.history["crew:gece"].splice(1, 0, {
    id: "m6c",
    sid: 1061,
    from: "DemirGolem_",
    text: "Ben de gelirim, haritayı kim seçiyor?",
    at: minutes(85),
    mine: false,
  });
  for (const msg of state.history["crew:gece"]) {
    if (!msg.mine && byName[msg.from]) {
      msg.by = byName[msg.from];
    }
  }
}

let tagsOn = Boolean(args.team6);
const wornBy = (name) => (tagsOn ? WEARS[NAMES[name]] : undefined);
if (args.team6) {
  for (const friend of state.friends) {
    friend.badge = WEARS[friend.id];
  }
  for (const msg of state.history["crew:gece"]) {
    if (!msg.mine && WEARS[NAMES[msg.from]]) {
      msg.badge = WEARS[NAMES[msg.from]];
    }
  }
  // Ece came into the crew between Mert's and Deniz's messages (the player's own line comes on "joined").
  NOTES.push({
    conv: "crew:gece",
    id: "j:ece",
    kind: "joined",
    name: "SessizOkcu",
    at: minutes(85),
  });
}

/** A conversation's messages as the launcher sends them now: crewmates' tags as they wear them today. */
function historyOf(convId) {
  return (state.history[convId] ?? []).map((m) => {
    if (!args.team6 || !convId.startsWith("crew:")) {
      return m;
    }
    const { badge, ...rest } = m;
    const worn = !m.mine ? wornBy(m.from) : undefined;
    return worn ? { ...rest, badge: worn } : rest;
  });
}

/** What the friends' rows say now: their tags while tags are on. */
function friendRows() {
  return state.friends.map((f) => ({ ...f, badge: tagsOn ? f.badge : undefined }));
}

function cardsFrame() {
  return { t: "badges", all: true, cards: tagsOn ? CARDS : {} };
}

function parseArgs(list) {
  const out = {};
  for (let i = 0; i < list.length; i++) {
    const key = list[i].replace(/^--/, "");
    const value = list[i + 1] && !list[i + 1].startsWith("--") ? list[++i] : true;
    out[key.replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = value;
  }
  return out;
}

function log(direction, frame) {
  const line = { t: now() - started, dir: direction, frame };
  if (frame?.t === "hello") {
    line.frame = { ...frame, token: "<redacted>" };
  }
  const text = JSON.stringify(line);
  if (args.log) {
    appendFileSync(args.log, `${text}\n`);
  }
  if (!args.quiet) {
    console.log(text);
  }
}

function send(socket, frame) {
  if (!socket || socket.destroyed) {
    return;
  }
  const text = JSON.stringify(frame);
  if (Buffer.byteLength(text) > MAX_OUT_FRAME) {
    throw new Error("frame too long");
  }
  log("out", frame);
  socket.write(`${text}\n`);
}

function later(ms, fn) {
  setTimeout(fn, ms);
}

function tokenMatches(candidate) {
  if (typeof candidate !== "string") {
    return false;
  }
  const a = Buffer.from(candidate);
  const b = Buffer.from(token);
  return a.length === b.length && timingSafeEqual(a, b);
}

function onConnection(socket) {
  let buffered = Buffer.alloc(0);
  let authed = false;
  const helloTimer = setTimeout(() => socket.destroy(), HELLO_TIMEOUT_MS);
  socket.on("data", (chunk) => {
    buffered = Buffer.concat([buffered, chunk]);
    for (let nl = buffered.indexOf(10); nl >= 0; nl = buffered.indexOf(10)) {
      const line = buffered.subarray(0, nl).toString("utf8").replace(/\r$/, "");
      buffered = buffered.subarray(nl + 1);
      let frame = null;
      try {
        frame = JSON.parse(line);
      } catch {
        frame = null;
      }
      if (!authed) {
        clearTimeout(helloTimer);
        if (frame?.t !== "hello") {
          log("in", { t: "garbage", length: line.length });
          socket.destroy();
          return;
        }
        log("in", frame);
        if (!tokenMatches(frame.token)) {
          send(socket, { t: "bye", reason: "auth" });
          socket.end();
          return;
        }
        if (frame.proto !== PROTO) {
          send(socket, { t: "bye", reason: "proto" });
          socket.end();
          return;
        }
        authed = true;
        langs.follows = Array.isArray(frame.features) && frame.features.includes("lang");
        emojis.hears = Array.isArray(frame.features) && frame.features.includes("emoji");
        emojis.quiet = Array.isArray(frame.features) && frame.features.includes("panel-off");
        startSession(socket);
        continue;
      }
      if (frame && typeof frame.t === "string") {
        log("in", frame);
        onFrame(socket, frame);
      }
    }
    if (buffered.length > MAX_IN_FRAME) {
      socket.destroy();
    }
  });
  socket.on("error", () => {});
  socket.on("close", () => {
    clearTimeout(helloTimer);
    if (session === socket) {
      session = null;
      log("info", { t: "closed" });
    }
  });
}

function welcome(socket) {
  if (emojis.quiet) {
    // The picker alone (E2): its words and its list, nothing of the friends feature.
    send(socket, {
      t: "welcome",
      proto: PROTO,
      launcher: "mock",
      me: state.me,
      dnd: false,
      reducedMotion: Boolean(args.reducedMotion),
      ...(langs.follows && overlayLang() ? { lang: overlayLang() } : {}),
      features: [],
    });
    sendEmojis(socket);
    return;
  }
  send(socket, {
    t: "welcome",
    proto: PROTO,
    launcher: "mock",
    me: state.me,
    dnd: Boolean(args.dnd),
    reducedMotion: Boolean(args.reducedMotion),
    ...(langs.follows && overlayLang() ? { lang: overlayLang() } : {}),
    features: args.signedOut
      ? []
      : [
          "friends",
          "chat",
          "calls",
          ...(args.chat2 ? EXTRAS : []),
          ...(tagsOn ? ["tags"] : []),
          ...(args.people ? ["people", ...(inviteOn ? ["invite"] : [])] : []),
          ...(args.people && args.people2 ? ["people2"] : []),
          ...(args.people && args.people2 && args.people3 ? ["people3"] : []),
          ...(args.link ? ["link"] : []),
          ...(args.host ? ["host"] : []),
        ],
  });
  if (args.team6) {
    send(socket, cardsFrame());
  }
  if (args.people && args.people2 && !args.signedOut) {
    send(socket, { t: "lists", ...LISTS });
  }
  send(socket, {
    t: "snapshot",
    friends: args.signedOut ? [] : friendRows(),
    convs: args.signedOut ? [] : state.convs,
    calls: state.calls,
    dnd: Boolean(args.dnd),
  });
  for (const note of NOTES) {
    send(socket, { t: "note", ...note });
  }
  sendEmojis(socket);
}

function startSession(socket) {
  if (session && session !== socket) {
    send(session, { t: "bye", reason: "replaced" });
    session.destroy();
  }
  session = socket;
  const delay = Number(args.welcomeDelay ?? 0);
  if (delay > 0) {
    later(delay, () => welcome(socket));
  } else {
    welcome(socket);
  }
  if (args.autoDemo) {
    demo(socket);
  }
}

function conv(id) {
  return state.convs.find((c) => c.id === id);
}

/** A new message from someone else: the message, its conversation's count and the toast. */
function incoming(socket, convId, from, text, sticker) {
  const c = conv(convId);
  const msg = { t: "msg", conv: convId, id: `m${nextId++}`, from, text, at: now(), mine: false };
  if (convId.startsWith("crew:") && wornBy(from)) {
    msg.badge = wornBy(from);
  }
  if (args.chat2) {
    msg.sid = newSid();
    if (sticker) {
      msg.sticker = sticker;
      msg.text = inEnglish()
        ? `[Sticker: ${stickerName(sticker)}]`
        : `[Çıkartma: ${stickerName(sticker)}]`;
    }
  }
  state.history[convId].push(msg);
  send(socket, msg);
  if (c) {
    c.unread += 1;
    send(socket, { t: "conv", ...c });
  }
  const body = sticker ? `${inEnglish() ? "Sticker" : "Çıkartma"} · ${stickerName(sticker)}` : text;
  const toast = { t: "toast", kind: "message", key: convId, title: from, body, conv: convId };
  if (sticker) {
    toast.sticker = sticker;
  }
  if (wornBy(from)) {
    toast.badge = wornBy(from);
  }
  if (c?.kind === "crew") {
    toast.crew = c.title;
  }
  send(socket, toast);
}

const STICKER_NAMES = {
  "pl1/gg": "GG",
  "pl1/hi": "Selam",
  "pl1/love": "Kalp",
  "pl1/party": "Kutlama",
};
const STICKER_NAMES_EN = {
  "pl1/gg": "GG",
  "pl1/hi": "Hi",
  "pl1/love": "Love",
  "pl1/party": "Party",
};
function stickerName(ref) {
  return inEnglish() ? (STICKER_NAMES_EN[ref] ?? "Sticker") : (STICKER_NAMES[ref] ?? "Çıkartma");
}

/** The message a reply answers, as the launcher resolves it from its own history. */
function quoteOf(convId, replySid) {
  const original = (state.history[convId] ?? []).find((m) => m.sid === replySid);
  if (!original) {
    return { sid: replySid, from: "", missing: true };
  }
  const quote = {
    id: original.id,
    sid: original.sid,
    from: original.from,
    mine: Boolean(original.mine),
  };
  if (original.sticker) {
    quote.sticker = original.sticker;
  } else {
    quote.text = original.text.length > 120 ? `${original.text.slice(0, 120)}…` : original.text;
  }
  return quote;
}

function startCall(socket, id, crew, from) {
  const call = {
    id,
    crew,
    from,
    since: now(),
    until: now() + 10 * 60000,
    answers: { coming: 1, soon: 0, cant: 0 },
    mine: null,
    where: "projects",
  };
  state.calls = [call];
  send(socket, { t: "call", ...call });
  // Names only: the mod words calls in the game's language.
  send(socket, { t: "toast", kind: "call", key: `call:${id}`, title: crew, body: from });
}

function demo(socket) {
  later(500, () =>
    incoming(socket, "dm:ayse", "PixelAvci", "Akşam bedwars var mı? Ekip toplanıyor."),
  );
  later(2500, () => startCall(socket, "c1", "Gece Kuşları", "NetherKral"));
}

function stack(socket) {
  later(0, () =>
    incoming(socket, "dm:ayse", "PixelAvci", "Akşam bedwars var mı? Ekip toplanıyor."),
  );
  later(350, () => startCall(socket, "c1", "Gece Kuşları", "NetherKral"));
  later(700, () => incoming(socket, "crew:gece", "MaviElmas", "Ben de geliyorum, bekleyin beni!"));
}

function onFrame(socket, frame) {
  switch (frame.t) {
    case "link-want": {
      // A made-up permit: "i" and 24 random bytes in base64url, 30 minutes, for the name the mod asked for.
      if (!args.link || frame.name === "Nobody") {
        send(socket, { t: "link-clear" });
      } else {
        send(socket, {
          t: "link-offer",
          nonce: `i${randomBytes(24).toString("base64url")}`,
          name: frame.name,
          expiresAt: Math.floor(Date.now() / 1000) + 1800,
        });
      }
      break;
    }
    case "open-conv": {
      const msgs = historyOf(frame.conv);
      send(socket, { t: "history", conv: frame.conv, msgs });
      const c = conv(frame.conv);
      if (c && c.unread > 0) {
        c.unread = 0;
        send(socket, { t: "conv", ...c });
      }
      break;
    }
    case "send": {
      if (args.chat2 && (typeof frame.sticker === "string" || typeof frame.reply === "number")) {
        onChat2Send(socket, frame);
        break;
      }
      const text = typeof frame.text === "string" ? frame.text : "";
      if (text.length === 0 || [...text].length > MAX_TEXT) {
        send(socket, { t: "send-failed", cid: frame.cid, reason: "too-long" });
        break;
      }
      if (/\b[a-z0-9-]+\.(gg|com|net|org|tr|io)\b/i.test(text)) {
        later(150, () => send(socket, { t: "send-failed", cid: frame.cid, reason: "link" }));
        break;
      }
      if (/\d{3}[\s-]?\d{3}[\s-]?\d{2}[\s-]?\d{2}/.test(text)) {
        later(150, () => send(socket, { t: "send-failed", cid: frame.cid, reason: "phone" }));
        break;
      }
      const id = `m${nextId++}`;
      if (!state.history[frame.conv]) {
        state.history[frame.conv] = [];
      }
      madeReal(socket, frame.conv);
      const mine = { id, from: state.me.name, text, at: now(), mine: true };
      if (args.chat2) {
        mine.sid = newSid();
      }
      state.history[frame.conv].push(mine);
      later(250, () => {
        send(socket, { t: "sent", cid: frame.cid, id, ...(args.chat2 ? { sid: mine.sid } : {}) });
        if (args.chat2) {
          send(socket, { t: "msg", conv: frame.conv, ...mine });
        }
      });
      if (args.chat2 && args.quietReplies) {
        break;
      }
      later(1200, () => {
        const reply = {
          t: "msg",
          conv: frame.conv,
          id: `m${nextId++}`,
          from: "PixelAvci",
          text: "Tamam, 5 dk sonra oradayım!",
          at: now(),
          mine: false,
        };
        if (args.chat2) {
          reply.sid = newSid();
        }
        state.history[frame.conv].push(reply);
        send(socket, reply);
        send(socket, {
          t: "toast",
          kind: "message",
          key: frame.conv,
          title: "PixelAvci",
          body: reply.text,
          conv: frame.conv,
        });
      });
      break;
    }
    case "answer": {
      const call = state.calls.find((c) => c.id === frame.call);
      if (call && ["coming", "soon", "cant"].includes(frame.answer)) {
        if (call.mine) {
          call.answers[call.mine]--;
        }
        call.mine = frame.answer;
        call.answers[frame.answer]++;
        later(120, () => send(socket, { t: "call", ...call }));
      }
      break;
    }
    case "react":
      if (args.chat2) {
        onReact(socket, frame);
      }
      break;
    case "invite":
    case "discord":
    case "unfriend":
    case "block":
    case "report":
      if (args.people) {
        onPeople(socket, frame);
      }
      break;
    case "favorite":
    case "alert":
    case "move":
    case "crew-invite":
    case "add-friend":
      if (args.people && args.people2) {
        onPeople2(socket, frame);
      }
      break;
    case "crew-mod":
    case "crew-lead":
    case "crew-remove":
      if (args.people && args.people2 && args.people3) {
        onPeople3(socket, frame);
      }
      break;
    case "typing":
      // The player writes in the panel: a launcher passes it to the friends service; the mock only logs it.
      break;
    case "dev":
      onDev(socket, frame);
      break;
    case "emoji": {
      // CHAT-EMOJI (E4): the player's switch from the overlay's settings, kept and answered.
      if (!emojis.hears || typeof frame.on !== "boolean") {
        break;
      }
      emojis.on = frame.on;
      log("info", { t: "emoji-switch", on: frame.on });
      sendEmojis(socket);
      break;
    }
    case "lang": {
      // OVL-LANG (G4): the player's pick, kept; a new welcome in it when the overlay's language changed.
      if (!langs.follows || (frame.lang !== "tr" && frame.lang !== "en")) {
        break;
      }
      const before = overlayLang();
      langs.pick = frame.lang;
      keepPick();
      log("info", { t: "lang-pick", lang: frame.lang });
      if (overlayLang() !== before) {
        welcome(socket);
      }
      break;
    }
    case "host-world":
    case "host-request":
    case "host-published":
    case "host-guest":
    case "host-invite":
    case "host-remove":
    case "host-stop":
    case "host-meet":
      if (args.host) {
        onHost(socket, frame);
      }
      break;
    default:
      // panel, join, connect-result and anything newer: the mock only logs them.
      break;
  }
}

/** OVL-UX: a message to a friend's conversation that had none yet makes it real, first in the list. */
function madeReal(socket, convId) {
  if (conv(convId)) {
    return;
  }
  const friend = state.friends.find((f) => f.dm === convId);
  if (!friend) {
    return;
  }
  const made = {
    id: convId,
    kind: "dm",
    title: friend.name,
    unread: 0,
    muted: false,
    writable: true,
  };
  state.convs.unshift(made);
  later(200, () => send(socket, { t: "conv", ...made }));
}

/** OVL-UX: the friend menu's actions, answered as the launcher would. */
function onPeople(socket, frame) {
  const friend = state.friends.find((f) => f.id === frame.friend);
  const result = (ok, more = {}) =>
    later(180, () =>
      send(socket, { t: "friend-result", friend: frame.friend, action: frame.t, ok, ...more }),
    );
  if (!friend && args.people2 && CREWMATES[frame.friend] && frame.t === "discord") {
    // OVL-UX2: a crewmate's Discord profile opens in the launcher; nothing comes back when it could.
    if (!LISTS.discord.includes(frame.friend)) {
      result(false, { reason: "failed" });
    }
    return;
  }
  if (!friend && args.people2 && CREWMATES[frame.friend] && ["block", "report"].includes(frame.t)) {
    // OVL-UX2: crew chat's sender menu blocks and reports a crewmate who is not a friend.
    result(true);
    return;
  }
  if (!friend) {
    result(false, { reason: "failed" });
    return;
  }
  switch (frame.t) {
    case "invite":
      if (!inviteOn) {
        result(false, { reason: "failed" });
      } else if (friend.id === "f-mert" && invitedOnce.has(friend.id)) {
        result(false, { reason: "wait" });
      } else {
        invitedOnce.add(friend.id);
        result(true, friend.where === "offline" ? { queued: true } : {});
      }
      break;
    case "discord":
      // The launcher opens the profile itself; nothing comes back when it could.
      if (!friend.discord) {
        result(false, { reason: "failed" });
      }
      break;
    case "report":
      if (!["name", "profile", "harassment", "spam", "scam", "other"].includes(frame.reason)) {
        result(false, { reason: "failed" });
        break;
      }
      result(true);
      if (frame.block) {
        gone(socket, friend.id);
      }
      break;
    default:
      // unfriend, block: the friend leaves the list first, then the answer comes.
      gone(socket, friend.id);
      result(true);
  }
}

/** OVL-UX2: the rest of the person menu, answered as the launcher would. */
function onPeople2(socket, frame) {
  const result = (ok, more = {}) =>
    later(180, () =>
      send(socket, { t: "friend-result", friend: frame.friend, action: frame.t, ok, ...more }),
    );
  if (frame.t === "add-friend") {
    const name = CREWMATES[frame.friend];
    if (!name || LISTS.asking.includes(frame.friend)) {
      result(false, { reason: "failed" });
      return;
    }
    if (LISTS.asked.includes(frame.friend)) {
      // They asked first: friends now (in the launcher, with a row).
      LISTS.asked = LISTS.asked.filter((id) => id !== frame.friend);
      // The lists name crewmates who are not friends only.
      LISTS.projects = LISTS.projects.filter((id) => id !== frame.friend);
      LISTS.discord = LISTS.discord.filter((id) => id !== frame.friend);
      const made = { id: frame.friend, name, where: "projects", joinable: true, dm: "dm:cem" };
      state.friends.push(made);
      delete CREWMATES[frame.friend];
      later(120, () => {
        send(socket, { t: "lists", ...LISTS });
        send(socket, { t: "friend", ...made });
      });
      result(true);
    } else {
      LISTS.asking.push(frame.friend);
      later(120, () => send(socket, { t: "lists", ...LISTS }));
      result(true, { queued: true });
    }
    return;
  }
  const friend = state.friends.find((f) => f.id === frame.friend);
  if (!friend) {
    result(false, { reason: "failed" });
    return;
  }
  switch (frame.t) {
    case "favorite":
      friend.fav = frame.on === true;
      // The launcher's rule: an online notice follows the favourite unless chosen for them.
      if (friend.alertChosen === undefined) {
        friend.alert = friend.fav;
      }
      later(60, () => send(socket, { t: "friend", ...friend }));
      break;
    case "alert":
      friend.alert = frame.on === true;
      friend.alertChosen = true;
      later(60, () => send(socket, { t: "friend", ...friend }));
      break;
    case "move": {
      if (args.people3 && !frame.group && typeof frame.name === "string" && frame.name.trim()) {
        // OVL-UX3: "Yeni kategori…": the category at the end of the list, the friend in it.
        if (LISTS.groups.length >= 20) {
          result(false, { reason: "failed" });
          break;
        }
        const id = `n${Date.now().toString(36).slice(-7)}`;
        LISTS.groups.push({ id, name: frame.name.trim().replace(/\s+/g, " ").slice(0, 24) });
        friend.group = id;
        later(40, () => send(socket, { t: "lists", ...LISTS }));
        later(60, () => send(socket, { t: "friend", ...friend }));
        result(true);
        break;
      }
      const known = LISTS.groups.some((g) => g.id === frame.group);
      if (frame.group && !known) {
        result(false, { reason: "failed" });
        break;
      }
      friend.group = known ? frame.group : undefined;
      later(60, () => send(socket, { t: "friend", ...friend }));
      result(true);
      break;
    }
    case "crew-invite": {
      const crew = LISTS.crews.find((c) => c.id === frame.crew);
      if (!crew || crew.full || friend.crews?.[crew.id]) {
        result(false, { reason: "failed" });
        break;
      }
      friend.crews = { ...(friend.crews ?? {}), [crew.id]: "invited" };
      later(60, () => send(socket, { t: "friend", ...friend }));
      result(true);
      break;
    }
    default:
      break;
  }
}

/**
 * OVL-UX3: a crew row from a sender's menu in Gece Kuşları's chat, answered as the launcher would after the friends
 * service took it: the new rights come as the lists again.
 */
function onPeople3(socket, frame) {
  const result = (ok) =>
    later(180, () =>
      send(socket, {
        t: "friend-result",
        friend: frame.friend,
        action: frame.t,
        ok,
        ...(ok ? {} : { reason: "failed" }),
      }),
    );
  const crew = LISTS.roles?.[frame.crew];
  const mate = crew?.mates?.[frame.friend];
  if (!crew || !mate) {
    result(false);
    return;
  }
  switch (frame.t) {
    case "crew-mod": {
      const mods = Object.values(crew.mates).filter((m) => m.mod).length;
      if (!crew.lead || !crew.roles || (frame.on === true && mods >= 4)) {
        result(false);
        return;
      }
      mate.mod = frame.on === true ? true : undefined;
      crew.modsFull = Object.values(crew.mates).filter((m) => m.mod).length >= 4 || undefined;
      break;
    }
    case "crew-lead":
      if (!crew.lead) {
        result(false);
        return;
      }
      // The player is a moderator now: no crew rows are theirs any more in the mock.
      delete LISTS.roles[frame.crew];
      break;
    case "crew-remove":
      if (!mate.kick) {
        result(false);
        return;
      }
      delete crew.mates[frame.friend];
      break;
    default:
      return;
  }
  later(60, () => send(socket, { t: "lists", ...LISTS }));
  result(true);
}

function gone(socket, id) {
  state.friends = state.friends.filter((f) => f.id !== id);
  later(120, () => send(socket, { t: "friend-gone", id }));
}

/** CHAT2: a sticker or a reply from the panel, answered as the launcher does (sent, then the whole message). */
function onChat2Send(socket, frame) {
  state.history[frame.conv] ??= [];
  madeReal(socket, frame.conv);
  const history = state.history[frame.conv];
  const msg = { id: `m${nextId++}`, sid: newSid(), from: state.me.name, at: now(), mine: true };
  if (typeof frame.sticker === "string") {
    if (!/^[a-z0-9]{1,8}[/][a-z0-9-]{1,24}$/.test(frame.sticker)) {
      later(100, () => send(socket, { t: "send-failed", cid: frame.cid, reason: "feature-off" }));
      return;
    }
    msg.sticker = frame.sticker;
    msg.text = `[Çıkartma: ${stickerName(frame.sticker)}]`;
  } else {
    msg.text = typeof frame.text === "string" ? frame.text : "";
  }
  if (typeof frame.reply === "number") {
    msg.reply = quoteOf(frame.conv, frame.reply);
  }
  history.push(msg);
  later(220, () => {
    send(socket, { t: "sent", cid: frame.cid, id: msg.id, sid: msg.sid });
    send(socket, { t: "msg", conv: frame.conv, ...msg });
  });
}

/** CHAT2: a reaction from the panel. 😮 is refused ("full") so the scenario can show the note. */
function onReact(socket, frame) {
  const msg = (state.history[frame.conv] ?? []).find((m) => m.sid === frame.sid);
  if (!msg) {
    send(socket, {
      t: "react-failed",
      conv: frame.conv,
      sid: frame.sid,
      e: frame.e,
      reason: "old",
    });
    return;
  }
  if (frame.e === "😮") {
    later(80, () =>
      send(socket, {
        t: "react-failed",
        conv: frame.conv,
        sid: frame.sid,
        e: frame.e,
        reason: "full",
      }),
    );
    return;
  }
  msg.reactions ??= [];
  const reactions = msg.reactions;
  const vs16 = String.fromCodePoint(0xfe0f);
  const same = (r) => r.e.replace(vs16, "") === frame.e.replace(vs16, "");
  let kind = reactions.find(same);
  if (frame.on) {
    if (!kind) {
      kind = { e: frame.e, n: 0, who: [] };
      reactions.push(kind);
    }
    if (!kind.mine) {
      kind.mine = true;
      kind.n += 1;
    }
  } else if (kind?.mine) {
    kind.mine = false;
    kind.n -= 1;
    if (kind.n <= 0) {
      reactions.splice(reactions.indexOf(kind), 1);
    }
  }
  later(60, () => send(socket, { t: "msg", conv: frame.conv, ...msg }));
}

function onDev(socket, frame) {
  switch (frame.cmd) {
    case "emojis-off":
    case "emojis-on":
      // CHAT-EMOJI: the launcher's Settings switched the picker.
      emojis.on = frame.cmd === "emojis-on";
      sendEmojis(socket);
      break;
    case "emojis-list":
      // CHAT-EMOJI: a Studio publish with a shorter list.
      sendEmojis(socket, Number(frame.count ?? 3));
      break;
    case "demo":
      demo(socket);
      break;
    case "one":
      incoming(socket, "dm:ayse", "PixelAvci", "Akşam bedwars var mı? Ekip toplanıyor.");
      break;
    case "stack":
      stack(socket);
      break;
    case "fourth":
      incoming(
        socket,
        "dm:zeynep",
        "YildizTozu",
        "Hadi gel, seni bekliyoruz. Kapıda buluşalım mı?",
      );
      break;
    case "more":
      incoming(socket, "dm:ayse", "PixelAvci", "Hâlâ orada mısın?");
      break;
    case "call":
      // A crew call (its card in the panel, its toast).
      startCall(socket, "c1", "Gece Kuşları", "NetherKral");
      break;
    case "crew-in":
      // TEAM6: a crewmate writes in crew chat (the toast and the message carry their tag).
      incoming(
        socket,
        frame.conv ?? "crew:gece",
        frame.from ?? "NetherKral",
        frame.text ?? "Ben hazırım, kim geliyor?",
      );
      break;
    case "joined": {
      // TEAM6 §7: someone comes into the crew while the launcher runs.
      const note = {
        conv: frame.conv ?? "crew:gece",
        id: `j:${frame.name ?? "TasDevi"}:${now()}`,
        kind: "joined",
        name: frame.name ?? "TasDevi",
        at: now(),
      };
      if (frame.mine) {
        note.mine = true;
      }
      NOTES.push(note);
      send(socket, { t: "note", ...note });
      break;
    }
    case "tag-change": {
      // TEAM6: Mert wears another crew's tag; his row changes and the open crew chat comes again.
      const mert = state.friends.find((f) => f.id === "f-mert");
      mert.badge = "t-gk";
      WEARS["f-mert"] = "t-gk";
      for (const msg of state.history["crew:gece"]) {
        if (msg.from === "NetherKral") {
          msg.badge = "t-gk";
        }
      }
      send(socket, { t: "friend", ...mert });
      send(socket, { t: "history", conv: "crew:gece", msgs: historyOf("crew:gece") });
      break;
    }
    case "tag-text": {
      // TEAM6: a crew's tag reads otherwise (Turkish letters at every GUI scale, OVL-LANG's legibility check).
      const card = CARDS[frame.crew];
      if (card && typeof frame.tag === "string") {
        card.tag = frame.tag;
        send(socket, { t: "badges", all: false, cards: { [frame.crew]: card } });
      }
      break;
    }
    case "hide-crew": {
      // TEAM6 §5: Gece Kartalları' leader hides the crew: its card goes, its wearers' rows lose it.
      delete CARDS["t-gk"];
      send(socket, { t: "badges", all: false, cards: { "t-gk": null } });
      for (const friend of state.friends) {
        if (friend.badge === "t-gk") {
          delete friend.badge;
          send(socket, { t: "friend", ...friend });
        }
      }
      for (const [id, crew] of Object.entries(WEARS)) {
        if (crew === "t-gk") {
          delete WEARS[id];
        }
      }
      break;
    }
    case "badge-off":
    case "badge-on":
      // The player takes their own tag off (or puts it on): the launcher greets again.
      state.me.badge = frame.cmd === "badge-on" ? "t-gece" : undefined;
      welcome(socket);
      break;
    case "invite-off":
      // The player left Projects: "Birlikte oyna" cannot go any more.
      inviteOn = false;
      welcome(socket);
      break;
    case "tags-off":
    case "tags-on":
      // Studio's switch: a new greeting without (or with) "tags".
      tagsOn = frame.cmd === "tags-on";
      welcome(socket);
      break;
    case "sticker-in":
      // CHAT2: a sticker from a friend (a toast with its picture; in the open conversation it arrives moving).
      incoming(
        socket,
        frame.conv ?? "dm:ayse",
        frame.from ?? "PixelAvci",
        "",
        frame.sticker ?? "pl1/party",
      );
      break;
    case "typing": {
      // CHAT2: "yazıyor…" from the launcher (a real one sends it only while the panel is open).
      const convId = frame.conv ?? "dm:ayse";
      const people = convId.startsWith("crew:")
        ? [
            ["f-mert", "NetherKral"],
            ["f-deniz", "MaviElmas"],
            ["f-ece", "SessizOkcu"],
          ]
        : [["f-ayse", "PixelAvci"]];
      for (const [by, name] of people.slice(0, Number(frame.count ?? people.length))) {
        send(socket, { t: "typing", conv: convId, by, name });
      }
      break;
    }
    case "seen": {
      // CHAT2: the friend has seen the player's newest message.
      const c = conv(frame.conv ?? "dm:ayse");
      const newest = (state.history[c.id] ?? []).filter((m) => m.mine && m.sid).at(-1);
      if (newest) {
        c.seen = newest.sid;
        send(socket, { t: "conv", ...c });
      }
      break;
    }
    case "connect": {
      // The launcher's "Bağlan" after the player confirmed there (a crew call, an invite, a plan).
      const out = { t: "connect", id: frame.id ?? `c${nextId++}`, host: frame.host };
      if (frame.port !== undefined) {
        out.port = frame.port;
      }
      send(socket, out);
      break;
    }
    case "edit": {
      const msgs = state.history["dm:ayse"];
      const last = msgs.findLast((m) => !m.mine);
      if (last) {
        last.text = `${last.text} (21:30'da)`;
        last.edited = true;
        send(socket, { t: "msg", conv: "dm:ayse", ...last });
      }
      break;
    }
    case "delete": {
      // Deleted for everyone (L16): the same id again, with "deleted" and no text.
      const msgs = state.history["dm:ayse"];
      const victim = msgs.find((m) => m.id === "m4");
      if (victim) {
        victim.text = "";
        victim.deleted = true;
        send(socket, { t: "msg", conv: "dm:ayse", ...victim });
      }
      break;
    }
    case "gone": {
      // A line dropped from the list (past the history limit).
      const msgs = state.history["dm:ayse"];
      const first = msgs.shift();
      if (first) {
        send(socket, { t: "msg-gone", conv: "dm:ayse", id: first.id });
      }
      break;
    }
    case "drop": {
      // Simulates a launcher crash and restart on the same port and token (re-adoption).
      const ms = Number(frame.ms ?? 4000);
      log("info", { t: "dropping", ms });
      socket.destroy();
      server.close();
      later(ms, () => listen());
      break;
    }
    case "world-invite": {
      // HOST1 (H13, H14): Ayşe invites the player to her open world.
      const friend = state.friends.find((f) => f.id === (frame.friend ?? "f-ayse"));
      friend.where = "single";
      friend.joinable = true;
      friend.world = { guests: Number(frame.guests ?? 1), max: Number(frame.max ?? 7) };
      send(socket, { t: "friend", ...friend });
      send(socket, { t: "toast", kind: "world", key: `world:${friend.id}`, title: friend.name });
      break;
    }
    case "world-connect":
      // HOST1 (H12): the launcher's way into a friend's world, read as its label.
      send(socket, {
        t: "connect",
        id: `c${nextId++}`,
        host: "127.0.0.1",
        port: Number(frame.port ?? 25599),
        label: frame.label ?? "Arkadaş dünyası: PixelAvci",
      });
      break;
    case "host-mirror":
      // HOST1 (H8): the guests as the scenario wants them ("in", "joining", "invited").
      hosting.mirror = frame.guests ? JSON.parse(frame.guests) : hosting.mirror;
      sendMirror(socket);
      break;
    case "host-fail":
      send(socket, {
        t: "host-state",
        open: false,
        max: 7,
        guests: [],
        error: frame.error ?? "unavailable",
      });
      break;
    case "host-refuse":
      // The next opening is refused as the launcher would (a daily limit, the service off).
      hosting.refuse = frame.error ?? "unavailable";
      break;
    case "host-stop":
      // The launcher closes the world (its own button, the account restricted, the launcher quitting).
      hosting.open = false;
      send(socket, { t: "host-stop", reason: frame.reason ?? "player" });
      break;
    case "host-remove":
      send(socket, {
        t: "host-remove",
        guest: frame.guest ?? "g1",
        reason: frame.reason ?? "removed",
      });
      break;
    case "launcher-lang":
    case "clear-pick": {
      // OVL-LANG: the launcher's own language changes, or the pick goes; the overlay follows when it changed.
      const before = overlayLang();
      if (frame.cmd === "launcher-lang" && (frame.lang === "tr" || frame.lang === "en")) {
        langs.launcher = frame.lang;
      } else if (frame.cmd === "clear-pick") {
        langs.pick = null;
        keepPick();
      }
      if (langs.follows && overlayLang() !== before) {
        welcome(socket);
      }
      break;
    }
    case "bye":
      send(socket, { t: "bye", reason: frame.reason ?? "launcher-exit" });
      socket.end();
      server.close();
      later(1000, () => process.exit(0));
      break;
    default:
      break;
  }
}

// ---- HOST1: the host launcher's side of world sharing ---------------------------------------------------------

const FRIEND_LABELS = {
  "f-ayse": "PixelAvci",
  "f-mert": "NetherKral",
  "f-deniz": "MaviElmas",
  "f-elif": "ZumrutKilic",
  "f-zeynep": "YildizTozu",
};
const hosting = {
  world: null,
  open: false,
  publish: null,
  port: 0,
  proto: 0,
  mirror: [],
  conn: 0,
  probes: [],
  relays: null,
  refuse: null,
};

/** PROTOCOL.md section 7.1: the gate key is HMAC-SHA256 of a phrase, keyed with the bridge token's ASCII bytes. */
function gateKey(tokenText = token) {
  return createHmac("sha256", Buffer.from(tokenText, "ascii"))
    .update("projects host gate v1")
    .digest();
}

/** A PLG1 header for these fields, MAC'd with {@code key}. */
function header(fields, key = gateKey()) {
  const json = Buffer.from(JSON.stringify({ v: 1, ...fields }), "utf8");
  const prefix = Buffer.alloc(6);
  prefix.write("PLG1", 0, "ascii");
  prefix.writeUInt16BE(json.length, 4);
  const signed = Buffer.concat([prefix, json]);
  return Buffer.concat([signed, createHmac("sha256", key).update(signed).digest()]);
}

function varInt(value) {
  const out = [];
  let v = value >>> 0;
  do {
    let b = v & 0x7f;
    v >>>= 7;
    if (v !== 0) {
      b |= 0x80;
    }
    out.push(b);
  } while (v !== 0);
  return Buffer.from(out);
}

function mcString(text) {
  const b = Buffer.from(text, "utf8");
  return Buffer.concat([varInt(b.length), b]);
}

function packet(id, ...parts) {
  const body = Buffer.concat([varInt(id), ...parts]);
  return Buffer.concat([varInt(body.length), body]);
}

/** The game's first two packets on a login: the handshake and the hello for {@code name}. */
function loginStart(name, portNumber, proto) {
  const p = Buffer.alloc(2);
  p.writeUInt16BE(portNumber, 0);
  const handshake = packet(0x00, varInt(proto), mcString("127.0.0.1"), p, varInt(2));
  const hello = packet(0x00, mcString(name), randomBytes(16));
  return Buffer.concat([handshake, hello]);
}

function readVarInt(buf, at) {
  let value = 0;
  let shift = 0;
  for (let i = at; i < buf.length && i < at + 5; i++) {
    value |= (buf[i] & 0x7f) << shift;
    if ((buf[i] & 0x80) === 0) {
      return [value, i + 1];
    }
    shift += 7;
  }
  return null;
}

/**
 * Connects to the world, writes {@code bytes}, reads what comes back for a moment and says what it was: "closed"
 * (nothing, then the end), "disconnect" with its text, "login-success", "encryption" (Mojang's check) or "silent".
 */
function probe(name, bytes, then) {
  const socket = net.connect(hosting.port, "127.0.0.1");
  let got = Buffer.alloc(0);
  let compressed = false;
  let done = false;
  const finish = (result) => {
    if (done) {
      return;
    }
    done = true;
    socket.destroy();
    hosting.probes.push({ name, ...result });
    log("info", { t: "probe", name, ...result });
    if (then) {
      later(150, then);
    }
  };
  socket.on("connect", () => socket.write(bytes));
  socket.on("data", (chunk) => {
    got = Buffer.concat([got, chunk]);
    for (;;) {
      const len = readVarInt(got, 0);
      if (!len || got.length < len[1] + len[0]) {
        return;
      }
      let body = got.subarray(len[1], len[1] + len[0]);
      got = got.subarray(len[1] + len[0]);
      if (compressed) {
        const dataLength = readVarInt(body, 0);
        if (dataLength?.[0] !== 0) {
          finish({ result: "compressed-packet" });
          return;
        }
        body = body.subarray(dataLength[1]);
      }
      const id = readVarInt(body, 0);
      if (!id) {
        return;
      }
      const payload = body.subarray(id[1]);
      if (id[0] === 0x03 && !compressed) {
        compressed = true;
        continue;
      }
      if (id[0] === 0x00) {
        const textLength = readVarInt(payload, 0);
        const text = textLength
          ? payload.subarray(textLength[1], textLength[1] + textLength[0]).toString("utf8")
          : "";
        finish({ result: "disconnect", text });
        return;
      }
      if (id[0] === 0x02) {
        finish({ result: "login-success" });
        return;
      }
      if (id[0] === 0x01) {
        finish({ result: "encryption" });
        return;
      }
      finish({ result: `packet-${id[0]}` });
      return;
    }
  });
  socket.on("close", () => finish({ result: got.length === 0 ? "closed" : "partial" }));
  socket.on("error", () => finish({ result: "error" }));
  later(6500, () => finish({ result: "silent" }));
}

const BINDING_KEY_ID = "sW9hzU3c";
const AYSE_DIGEST = "1UbirWHKbvkTXKMZNmDDug";
const MERT_DIGEST = "n7ppm8zILloa1X-qpWnqVA";
// Any 16 bytes in base64url stand for a friend here.
const DENIZ_DIGEST = createHash("sha256")
  .update("f-deniz")
  .digest()
  .subarray(0, 16)
  .toString("base64url");
const DIGESTS = { ayse: AYSE_DIGEST, mert: MERT_DIGEST, deniz: DENIZ_DIGEST };

/** The gate as the host launcher's connections reach it, one after another. */
function probeGate() {
  const id = hosting.publish;
  const proto = hosting.proto;
  const fields = (name, conn, guest, friend, premium = false) => ({
    id,
    conn,
    guest,
    name,
    premium,
    friend,
  });
  probe("no-header", Buffer.from("\x10\x00hello there, no header at all"), () =>
    probe(
      "other-key",
      Buffer.concat([
        header(
          fields("Notch", ++hosting.conn, "g1", AYSE_DIGEST),
          gateKey("another-launch-token-0123456789abcdefABCDEF"),
        ),
        loginStart("Notch", hosting.port, proto),
      ]),
      () =>
        probe(
          "host-name",
          Buffer.concat([
            header(fields(args.hostName ?? "OVL2Test", ++hosting.conn, "g2", MERT_DIGEST)),
            loginStart(args.hostName ?? "OVL2Test", hosting.port, proto),
          ]),
          () => {
            const conn = ++hosting.conn;
            probe(
              "guest-login",
              Buffer.concat([
                header(fields("Notch", conn, "g1", AYSE_DIGEST)),
                loginStart("Notch", hosting.port, proto),
              ]),
              () =>
                probe(
                  "conn-reused",
                  Buffer.concat([
                    header(fields("Notch", conn, "g1", AYSE_DIGEST)),
                    loginStart("Notch", hosting.port, proto),
                  ]),
                  () =>
                    probe(
                      "other-publish",
                      Buffer.concat([
                        header({
                          ...fields("Notch", ++hosting.conn, "g1", AYSE_DIGEST),
                          id: "p-old",
                        }),
                        loginStart("Notch", hosting.port, proto),
                      ]),
                      () =>
                        log("info", {
                          t: "probes-done",
                          probes: hosting.probes.map((p) => `${p.name}:${p.result}`),
                        }),
                    ),
                ),
            );
          },
        ),
    ),
  );
}

/** The guest identities the relay ports stand for: --relay-as first (as g1, Ayşe), then each of --relay. */
function relayIdentities() {
  const list = [];
  if (typeof args.relayAs === "string") {
    list.push({ guest: "g1", friend: AYSE_DIGEST, name: args.relayAs });
  }
  if (typeof args.relay === "string") {
    for (const item of args.relay.split(",")) {
      const [guest, friend, name] = item.split(":");
      if (guest && DIGESTS[friend] && name) {
        list.push({ guest, friend: DIGESTS[friend], name });
      }
    }
  }
  return list;
}

/** Every connection to a relay port gets a header for that port's guest and goes on to the world (as it is now). */
function openRelay() {
  const identities = relayIdentities();
  if (identities.length === 0 || hosting.relays) {
    return;
  }
  hosting.relays = [];
  const ports = [];
  let listening = 0;
  identities.forEach((who, index) => {
    const relay = net.createServer((client) => {
      const world = net.connect(hosting.port, "127.0.0.1", () => {
        world.write(
          header({
            id: hosting.publish,
            conn: ++hosting.conn,
            guest: who.guest,
            name: who.name,
            premium: false,
            friend: who.friend,
          }),
        );
        client.pipe(world);
        world.pipe(client);
      });
      world.on("error", () => client.destroy());
      client.on("error", () => world.destroy());
    });
    hosting.relays.push(relay);
    relay.listen(0, "127.0.0.1", () => {
      ports[index] = relay.address().port;
      log("info", { t: "relay", port: ports[index], as: who.name, guest: who.guest });
      listening++;
      if (listening === identities.length && args.info) {
        writeFileSync(
          args.info,
          JSON.stringify({ port, token, relayPort: ports[0], relayPorts: ports }),
        );
      }
    });
  });
}

function sendMirror(socket) {
  send(socket, { t: "host-state", open: hosting.open, max: 7, guests: hosting.mirror });
}

function onHost(socket, frame) {
  switch (frame.t) {
    case "host-world":
      hosting.world = frame.open ? frame : null;
      break;
    case "host-request": {
      if (!hosting.world || hosting.world.lan || hosting.refuse) {
        const error = hosting.refuse ?? "unavailable";
        hosting.refuse = null;
        later(200, () => send(socket, { t: "host-state", open: false, max: 7, guests: [], error }));
        break;
      }
      hosting.publish = `p${nextId++}`;
      hosting.mirror = frame.invite.map((friend) => ({
        guest: "",
        friend,
        label: FRIEND_LABELS[friend] ?? friend,
        state: "invited",
      }));
      later(250, () =>
        send(socket, {
          t: "host-publish",
          id: hosting.publish,
          mode: frame.mode,
          commands: frame.commands,
          max: 7,
          binding: BINDING_KEY_ID,
        }),
      );
      break;
    }
    case "host-published":
      if (frame.id !== hosting.publish) {
        break;
      }
      if (!frame.ok) {
        send(socket, { t: "host-state", open: false, max: 7, guests: [], error: "unavailable" });
        break;
      }
      hosting.port = frame.port;
      hosting.proto = frame.proto;
      hosting.open = true;
      // The paths and the invites take a moment in the real launcher.
      later(400, () => {
        sendMirror(socket);
        openRelay();
        if (!args.noProbe) {
          later(300, probeGate);
        }
      });
      break;
    case "host-guest": {
      const row = hosting.mirror.find((g) => g.guest === frame.guest) ?? hosting.mirror[0];
      if (row && frame.state === "in") {
        row.guest = frame.guest;
        row.name = frame.name;
        row.state = "in";
        sendMirror(socket);
      } else if (row && frame.state === "out") {
        row.state = "invited";
        sendMirror(socket);
      }
      break;
    }
    case "host-invite":
      for (const friend of frame.invite) {
        if (!hosting.mirror.some((g) => g.friend === friend)) {
          hosting.mirror.push({
            guest: "",
            friend,
            label: FRIEND_LABELS[friend] ?? friend,
            state: "invited",
          });
        }
      }
      sendMirror(socket);
      break;
    case "host-remove":
      // As the launcher: the slot goes, the game hears it (it sent the guest off already).
      hosting.mirror = hosting.mirror.filter((g) => g.guest !== frame.guest);
      send(socket, { t: "host-remove", guest: frame.guest, reason: "removed" });
      sendMirror(socket);
      break;
    case "host-stop":
      hosting.open = false;
      hosting.mirror = [];
      break;
    default:
      break;
  }
}

function listen() {
  server = net.createServer(onConnection);
  server.on("error", (err) => {
    log("info", { t: "listen-error", code: err.code });
    later(500, () => listen());
  });
  server.listen(port, "127.0.0.1", () => {
    port = server.address().port;
    log("info", { t: "listening", port });
    if (args.info) {
      writeFileSync(args.info, JSON.stringify({ port, token }));
    }
  });
}

listen();
