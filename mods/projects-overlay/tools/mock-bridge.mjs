// Plays the launcher's side of the overlay bridge for local tests, with made-up sample data.
// It listens on 127.0.0.1 only and never connects anywhere.
//
//   node tools/mock-bridge.mjs [--port 0] [--token <t>] [--info <file>] [--log <file>] [--auto-demo]
//                              [--reduced-motion] [--signed-out] [--quiet]
//
// --info writes {"port","token"} once listening, for the script that starts the game.
// The game's scenario driver sends {"t":"dev","cmd":...} frames to script the scenario; the real launcher
// ignores unknown frame types, so these exist only here. Frames follow ovl1.md §3 and the additions in
// docs/tasks/ovl2-protocol.md (L1-L16, and "connect" with its "connect-result").
// {"t":"dev","cmd":"connect","host":"localhost","port":25599} makes the mock send the launcher's "Bağlan".
// Each mod frame is written to --log, so a run's connect-result answers can be checked afterwards.

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
    features: args.signedOut ? [] : ["friends", "chat", "calls"],
  });
  send(socket, {
    t: "snapshot",
    friends: args.signedOut ? [] : state.friends,
    convs: args.signedOut ? [] : state.convs,
    calls: state.calls,
    dnd: Boolean(args.dnd),
  });
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
function incoming(socket, convId, from, text) {
  const c = conv(convId);
  const msg = { t: "msg", conv: convId, id: `m${nextId++}`, from, text, at: now(), mine: false };
  state.history[convId].push(msg);
  send(socket, msg);
  if (c) {
    c.unread += 1;
    send(socket, { t: "conv", ...c });
  }
  const toast = { t: "toast", kind: "message", key: convId, title: from, body: text, conv: convId };
  if (c?.kind === "crew") {
    toast.crew = c.title;
  }
  send(socket, toast);
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
    case "open-conv": {
      const msgs = state.history[frame.conv] ?? [];
      send(socket, { t: "history", conv: frame.conv, msgs });
      const c = conv(frame.conv);
      if (c && c.unread > 0) {
        c.unread = 0;
        send(socket, { t: "conv", ...c });
      }
      break;
    }
    case "send": {
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
      state.history[frame.conv].push({ id, from: state.me.name, text, at: now(), mine: true });
      later(250, () => send(socket, { t: "sent", cid: frame.cid, id }));
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
    case "dev":
      onDev(socket, frame);
      break;
    default:
      // panel, join, connect-result and anything newer: the mock only logs them.
      break;
  }
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
