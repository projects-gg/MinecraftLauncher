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
// dev commands "joined", "tag-change", "hide-crew", "tags-off" and "tags-on" play the live changes.
// --link plays codeless Discord linking (docs/tasks/ovl2-protocol.md, SKIN1-O): "link" in the welcome; a mod's
// "link-want" gets a made-up permit ("link-offer") for that name, or "link-clear" for the name "Nobody".

import { randomBytes, timingSafeEqual } from "node:crypto";
import { appendFileSync, writeFileSync } from "node:fs";
import net from "node:net";

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

const now = () => Date.now();
const minutes = (m) => now() - m * 60000;
const state = {
  me: { name: args.signedOut ? "" : "OVL2Test" },
  friends: [
    { id: "f-ayse", name: "Ayşe", where: "projects", joinable: true },
    { id: "f-elif", name: "Elif", where: "server", joinable: true, host: "mc.ornek.net" },
    { id: "f-zeynep", name: "Zeynep", where: "single", joinable: false },
    { id: "f-deniz", name: "Deniz", where: "game", joinable: false },
    { id: "f-mert", name: "Mert", where: "launcher", joinable: false },
    { id: "f-can", name: "Can", where: "offline", joinable: false },
    { id: "f-ece", name: "Ece", where: "offline", joinable: false },
  ],
  convs: [
    { id: "dm:ayse", kind: "dm", title: "Ayşe", unread: 2, muted: false, writable: true },
    {
      id: "crew:gece",
      kind: "crew",
      title: "Gece Kuşları",
      unread: 0,
      muted: false,
      writable: true,
    },
    { id: "dm:mert", kind: "dm", title: "Mert", unread: 0, muted: false, writable: true },
    { id: "dm:zeynep", kind: "dm", title: "Zeynep", unread: 1, muted: false, writable: true },
    { id: "crew:akinci", kind: "crew", title: "Akıncılar", unread: 0, muted: true, writable: true },
    { id: "dm:can", kind: "dm", title: "Can", unread: 0, muted: false, writable: false },
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
      { id: "m2", from: "Ayşe", text: "Evet, 21:00'de başlıyor.", at: minutes(40), mine: false },
      {
        id: "m3",
        from: "Ayşe",
        text: "Ekip toplanıyor, sen de gel!",
        at: minutes(39),
        mine: false,
        edited: true,
      },
      {
        id: "m4",
        from: "Ayşe",
        text: "Selam! Bu akşam oynuyor musun?",
        at: minutes(3),
        mine: false,
      },
      {
        id: "m5",
        from: "Ayşe",
        text: "Haritayı bu sefer ben seçeceğim.",
        at: minutes(2),
        mine: false,
      },
    ],
    "crew:gece": [
      {
        id: "m6",
        from: "Mert",
        text: "Yarın 21:00'de antrenman var, unutmayın.",
        at: minutes(90),
        mine: false,
      },
      {
        id: "m7",
        from: "Deniz",
        text: "Ben gelemeyebilirim, haber veririm.",
        at: minutes(80),
        mine: false,
      },
      { id: "m8", from: "OVL2Test", text: "Tamam, ben oradayım.", at: minutes(75), mine: true },
    ],
    "dm:mert": [
      { id: "m9", from: "Mert", text: "Kılıcı sende mi bıraktım?", at: minutes(300), mine: false },
    ],
    "dm:zeynep": [
      { id: "m10", from: "Zeynep", text: "Adamı gördün mü az önce?", at: minutes(1), mine: false },
    ],
    "crew:akinci": [],
    "dm:can": [{ id: "m11", from: "Can", text: "Görüşürüz!", at: minutes(4000), mine: false }],
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
      from: "Ayşe",
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
      from: "Ayşe",
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
        from: "Ayşe",
        text: "Evet, 21:00'de başlıyor. Kurallar burada:…",
      },
      reactions: [
        { e: "❤️", n: 1, who: ["Ayşe"] },
        { e: "🔥", n: 1, who: ["Ayşe"] },
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
      from: "Mert",
      text: "Yarın 21:00'de antrenman var, unutmayın.",
      at: minutes(90),
      mine: false,
      reactions: [
        { e: "👍", n: 4, mine: true, who: ["Deniz", "Ayşe", "Can"] },
        { e: "🔥", n: 2, who: ["Deniz", "Elif"] },
        { e: "😂", n: 1, who: ["Can"] },
      ],
    },
    {
      id: "m7",
      sid: 107,
      from: "Deniz",
      text: "Ben gelemeyebilirim, haber veririm.",
      at: minutes(80),
      mine: false,
      reply: { id: "m6", sid: 106, from: "Mert", text: "Yarın 21:00'de antrenman var, unutmayın." },
    },
    {
      id: "m7b",
      sid: 108,
      from: "Deniz",
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
const NAMES = { Ayşe: "f-ayse", Can: "f-can", Mert: "f-mert", Deniz: "f-deniz", Elif: "f-elif" };
const NOTES = [];
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
  NOTES.push({ conv: "crew:gece", id: "j:ece", kind: "joined", name: "Ece", at: minutes(85) });
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
  send(socket, {
    t: "welcome",
    proto: PROTO,
    launcher: "mock",
    me: state.me,
    dnd: Boolean(args.dnd),
    reducedMotion: Boolean(args.reducedMotion),
    features: args.signedOut
      ? []
      : [
          "friends",
          "chat",
          "calls",
          ...(args.chat2 ? EXTRAS : []),
          ...(tagsOn ? ["tags"] : []),
          ...(args.link ? ["link"] : []),
        ],
  });
  if (args.team6) {
    send(socket, cardsFrame());
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
}

function startSession(socket) {
  if (session && session !== socket) {
    send(session, { t: "bye", reason: "replaced" });
    session.destroy();
  }
  session = socket;
  welcome(socket);
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
      msg.text = `[Çıkartma: ${stickerName(sticker)}]`;
    }
  }
  state.history[convId].push(msg);
  send(socket, msg);
  if (c) {
    c.unread += 1;
    send(socket, { t: "conv", ...c });
  }
  const body = sticker ? `Çıkartma · ${stickerName(sticker)}` : text;
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
function stickerName(ref) {
  return STICKER_NAMES[ref] ?? "Çıkartma";
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
  later(500, () => incoming(socket, "dm:ayse", "Ayşe", "Akşam bedwars var mı? Ekip toplanıyor."));
  later(2500, () => startCall(socket, "c1", "Gece Kuşları", "Mert"));
}

function stack(socket) {
  later(0, () => incoming(socket, "dm:ayse", "Ayşe", "Akşam bedwars var mı? Ekip toplanıyor."));
  later(350, () => startCall(socket, "c1", "Gece Kuşları", "Mert"));
  later(700, () => incoming(socket, "crew:gece", "Deniz", "Ben de geliyorum, bekleyin beni!"));
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
          from: "Ayşe",
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
          title: "Ayşe",
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
    case "typing":
      // The player writes in the panel: a launcher passes it to the friends service; the mock only logs it.
      break;
    case "dev":
      onDev(socket, frame);
      break;
    default:
      // panel, join, connect-result and anything newer: the mock only logs them.
      break;
  }
}

/** CHAT2: a sticker or a reply from the panel, answered as the launcher does (sent, then the whole message). */
function onChat2Send(socket, frame) {
  const history = state.history[frame.conv] ?? (state.history[frame.conv] = []);
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
  const reactions = msg.reactions ?? (msg.reactions = []);
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
    case "demo":
      demo(socket);
      break;
    case "one":
      incoming(socket, "dm:ayse", "Ayşe", "Akşam bedwars var mı? Ekip toplanıyor.");
      break;
    case "stack":
      stack(socket);
      break;
    case "fourth":
      incoming(socket, "dm:zeynep", "Zeynep", "Hadi gel, seni bekliyoruz. Kapıda buluşalım mı?");
      break;
    case "more":
      incoming(socket, "dm:ayse", "Ayşe", "Hâlâ orada mısın?");
      break;
    case "call":
      // A crew call (its card in the panel, its toast).
      startCall(socket, "c1", "Gece Kuşları", "Mert");
      break;
    case "crew-in":
      // TEAM6: a crewmate writes in crew chat (the toast and the message carry their tag).
      incoming(
        socket,
        frame.conv ?? "crew:gece",
        frame.from ?? "Mert",
        frame.text ?? "Ben hazırım, kim geliyor?",
      );
      break;
    case "joined": {
      // TEAM6 §7: someone comes into the crew while the launcher runs.
      const note = {
        conv: frame.conv ?? "crew:gece",
        id: `j:${frame.name ?? "Kaan"}:${now()}`,
        kind: "joined",
        name: frame.name ?? "Kaan",
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
        if (msg.from === "Mert") {
          msg.badge = "t-gk";
        }
      }
      send(socket, { t: "friend", ...mert });
      send(socket, { t: "history", conv: "crew:gece", msgs: historyOf("crew:gece") });
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
        frame.from ?? "Ayşe",
        "",
        frame.sticker ?? "pl1/party",
      );
      break;
    case "typing": {
      // CHAT2: "yazıyor…" from the launcher (a real one sends it only while the panel is open).
      const convId = frame.conv ?? "dm:ayse";
      const people = convId.startsWith("crew:")
        ? [
            ["f-mert", "Mert"],
            ["f-deniz", "Deniz"],
            ["f-ece", "Ece"],
          ]
        : [["f-ayse", "Ayşe"]];
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
