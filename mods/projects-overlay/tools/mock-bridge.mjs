// Plays the launcher's side of the overlay bridge for local tests, with made-up sample data.
// It listens on 127.0.0.1 only and never connects anywhere.
//
//   node tools/mock-bridge.mjs [--port 0] [--token <t>] [--info <file>] [--log <file>] [--auto-demo]
//
// --info writes {"port","token"} once listening, for the script that starts the game.
// The game's spike driver sends {"t":"dev","cmd":...} frames to script the scenario; the real launcher
// ignores unknown frame types, so these exist only here.

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
const state = {
  me: { name: "OVL1Test", status: "online" },
  friends: [
    { id: "f-ayse", name: "Ayşe", where: "projects", joinable: true },
    { id: "f-mert", name: "Mert", where: "launcher", joinable: false },
    { id: "f-zeynep", name: "Zeynep", where: "single", joinable: false },
    { id: "f-can", name: "Can", where: "offline", joinable: false },
  ],
  convs: [
    { id: "dm:ayse", kind: "dm", title: "Ayşe", unread: 1, muted: false },
    { id: "crew:gece", kind: "crew", title: "Gece Kuşları", unread: 0, muted: false },
  ],
  history: {
    "dm:ayse": [
      {
        id: "m1",
        from: "Ayşe",
        text: "Selam! Bu akşam oynuyor musun?",
        at: now() - 600000,
        mine: false,
      },
    ],
    "crew:gece": [
      {
        id: "m2",
        from: "Mert",
        text: "Yarın 21:00'de turnuva var, unutmayın.",
        at: now() - 900000,
        mine: false,
      },
    ],
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
          send(socket, { t: "bye", reason: "proto", supported: [PROTO, PROTO] });
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

function startSession(socket) {
  if (session && session !== socket) {
    send(session, { t: "bye", reason: "replaced" });
    session.destroy();
  }
  session = socket;
  send(socket, {
    t: "welcome",
    proto: PROTO,
    launcher: "mock",
    me: state.me,
    dnd: false,
    reducedMotion: Boolean(args.reducedMotion),
    features: ["friends", "chat", "calls"],
  });
  send(socket, { t: "snapshot", friends: state.friends, convs: state.convs, calls: state.calls });
  if (args.autoDemo) {
    demo(socket);
  }
}

function demo(socket) {
  later(500, () => {
    const msg = {
      t: "msg",
      conv: "dm:ayse",
      id: `m${nextId++}`,
      from: "Ayşe",
      text: "Akşam bedwars var mı? Ekip toplanıyor.",
      at: now(),
      mine: false,
    };
    state.history["dm:ayse"].push(msg);
    send(socket, msg);
    send(socket, { t: "conv", ...state.convs[0], unread: 2 });
    send(socket, {
      t: "toast",
      kind: "message",
      key: "dm:ayse",
      title: "Ayşe",
      body: msg.text,
      conv: "dm:ayse",
    });
  });
  later(2500, () => {
    const call = {
      id: "c1",
      crew: "Gece Kuşları",
      from: "Mert",
      since: now(),
      answers: { coming: 1, soon: 0, cant: 0 },
      mine: null,
    };
    state.calls = [call];
    send(socket, { t: "call", ...call });
    send(socket, {
      t: "toast",
      kind: "call",
      key: "call:c1",
      title: "Gece Kuşları",
      // Names only: the mod words calls in the game's language.
      body: "Mert",
    });
  });
}

function onFrame(socket, frame) {
  switch (frame.t) {
    case "open-conv": {
      const msgs = state.history[frame.conv] ?? [];
      send(socket, { t: "history", conv: frame.conv, msgs });
      const conv = state.convs.find((c) => c.id === frame.conv);
      if (conv && conv.unread > 0) {
        conv.unread = 0;
        send(socket, { t: "conv", ...conv });
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
        send(socket, { t: "send-failed", cid: frame.cid, reason: "link" });
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
        send(socket, { t: "call", ...call });
      }
      break;
    }
    case "dev":
      onDev(socket, frame);
      break;
    default:
      // panel, join and anything newer: the mock only logs them.
      break;
  }
}

function onDev(socket, frame) {
  if (frame.cmd === "demo") {
    demo(socket);
  } else if (frame.cmd === "drop") {
    // Simulates a launcher crash and restart on the same port and token (re-adoption).
    const ms = Number(frame.ms ?? 4000);
    log("info", { t: "dropping", ms });
    socket.destroy();
    server.close();
    later(ms, () => listen());
  } else if (frame.cmd === "bye") {
    send(socket, { t: "bye", reason: frame.reason ?? "launcher-exit" });
    socket.end();
    server.close();
    later(1000, () => process.exit(0));
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
