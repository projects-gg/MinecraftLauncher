// Plays the in-game overlay mod against a real launcher's bridge, for local end-to-end runs.
// It reads `-Dprojects.overlay.port` and `-Dprojects.overlay.token` from the running game's command
// line (or its Java argument file) the way the mod reads its system properties, says hello, logs
// every frame and can act like the panel. It never connects anywhere but 127.0.0.1.
//
//   node crates/pl-overlay/tools/mock-mod.mjs [--pid <game pid>] [--port <p> --token <t>]
//        [--log <file.jsonl>] [--expect <frame type>[,<type>...]] [--timeout <s>]
//        [--send <conv>=<text>] [--open <conv>] [--answer <call>=coming|soon|cant]
//        [--join <friend id>] [--connect ok|busy|silent] [--stay]
//
// `--connect` says `features: ["connect"]` in the hello and answers the launcher's `connect`
// frames: `ok` (switched), `busy` (refused with that reason) or `silent` (no answer).
//
// Exits 0 once every `--expect`ed frame type arrived (or at `bye` / the end of the stream with
// `--stay`), 1 on a timeout, 2 when no game with a bridge was found. Windows only for the lookup.

import { execFileSync } from "node:child_process";
import { appendFileSync, readFileSync } from "node:fs";
import net from "node:net";

const PROTO = 1;
const PORT_PROP = "-Dprojects.overlay.port=";

function parseArgs(list) {
  const out = {};
  for (let i = 0; i < list.length; i++) {
    const key = list[i].replace(/^--/, "");
    const value = list[i + 1] && !list[i + 1].startsWith("--") ? list[++i] : true;
    out[key] = value;
  }
  return out;
}

const args = parseArgs(process.argv.slice(2));
const started = Date.now();

function log(direction, frame) {
  const shown = frame?.t === "hello" ? { ...frame, token: "<redacted>" } : frame;
  const line = JSON.stringify({ ms: Date.now() - started, dir: direction, frame: shown });
  console.log(line);
  if (args.log) appendFileSync(args.log, `${line}\n`);
}

/** The two properties from an argument list, following a Java argument file (`@path`). */
function fromArguments(text) {
  const port = text.match(/-Dprojects\.overlay\.port=(\d+)/);
  const token = text.match(/-Dprojects\.overlay\.token=([A-Za-z0-9_-]{20,})/);
  if (port && token) return { port: Number(port[1]), token: token[1] };
  for (const file of text.matchAll(/(?:^|\s)"?@([^"\s]+)"?/g)) {
    try {
      const found = fromArguments(readFileSync(file[1], "utf8"));
      if (found) return found;
    } catch {
      // Not readable: not ours.
    }
  }
  return null;
}

/** The running games' command lines (Java processes), newest first. */
function gameCommandLines() {
  const filter = args.pid ? `ProcessId=${Number(args.pid)}` : "Name='java.exe' OR Name='javaw.exe'";
  const script = `Get-CimInstance Win32_Process -Filter "${filter}" | Sort-Object CreationDate -Descending | Select-Object ProcessId,CommandLine | ConvertTo-Json -Compress`;
  const json = execFileSync("powershell", ["-NoProfile", "-Command", script], {
    encoding: "utf8",
  }).trim();
  if (!json) return [];
  const parsed = JSON.parse(json);
  return (Array.isArray(parsed) ? parsed : [parsed]).filter((p) => p.CommandLine);
}

function endpoint() {
  if (args.port && args.token) return { port: Number(args.port), token: String(args.token) };
  for (const game of gameCommandLines()) {
    if (!game.CommandLine.includes(PORT_PROP) && !game.CommandLine.includes("@")) continue;
    const found = fromArguments(game.CommandLine);
    if (found) {
      log("info", { t: "game", pid: game.ProcessId, port: found.port });
      return found;
    }
  }
  return null;
}

const target = endpoint();
if (!target) {
  log("info", { t: "no-game" });
  process.exit(2);
}

const expected = new Set(
  typeof args.expect === "string" ? args.expect.split(",").filter(Boolean) : [],
);
const seen = new Set();
const timeoutMs = Number(args.timeout ?? 30) * 1000;
const socket = net.connect({ host: "127.0.0.1", port: target.port });
let buffered = Buffer.alloc(0);
let done = false;

function finish(code) {
  if (done) return;
  done = true;
  log("info", { t: "done", code, seen: [...seen] });
  socket.destroy();
  process.exit(code);
}

function send(frame) {
  log("out", frame);
  socket.write(`${JSON.stringify(frame)}\n`);
}

/** The panel's actions, once the launcher greeted the mod. */
function act() {
  if (typeof args.open === "string") send({ t: "panel", open: true });
  if (typeof args.open === "string") send({ t: "open-conv", conv: args.open });
  if (typeof args.send === "string") {
    const at = args.send.indexOf("=");
    send({
      t: "send",
      cid: `mock-${Date.now()}`,
      conv: args.send.slice(0, at),
      text: args.send.slice(at + 1),
    });
  }
  if (typeof args.answer === "string") {
    const [call, answer] = args.answer.split("=");
    send({ t: "answer", call, answer });
  }
  if (typeof args.join === "string") send({ t: "join", friend: args.join });
}

setTimeout(() => finish(expected.size && !args.stay ? 1 : 0), timeoutMs);

socket.on("connect", () => {
  const features = typeof args.connect === "string" ? ["connect"] : [];
  send({
    t: "hello",
    proto: PROTO,
    token: target.token,
    mod: "mock",
    mc: "26.1.2",
    lang: "tr_tr",
    features,
  });
});

/** The launcher's "Bağlan": answered as `--connect` says. */
function onConnect(frame) {
  if (args.connect === "ok") send({ t: "connect-result", id: frame.id, ok: true });
  if (args.connect === "busy") {
    send({ t: "connect-result", id: frame.id, ok: false, reason: "busy" });
  }
}
socket.on("data", (chunk) => {
  buffered = Buffer.concat([buffered, chunk]);
  for (let nl = buffered.indexOf(10); nl >= 0; nl = buffered.indexOf(10)) {
    const line = buffered.subarray(0, nl).toString("utf8").replace(/\r$/, "");
    buffered = buffered.subarray(nl + 1);
    let frame;
    try {
      frame = JSON.parse(line);
    } catch {
      continue;
    }
    log("in", frame);
    seen.add(frame.t);
    if (frame.t === "connect") onConnect(frame);
    if (frame.t === "snapshot" && !seen.has("acted")) {
      seen.add("acted");
      act();
    }
    const all = [...expected].every((t) => seen.has(t));
    if (frame.t === "bye") finish(all ? 0 : 1);
    if (expected.size && all && !args.stay) finish(0);
  }
});
socket.on("error", (error) => {
  log("info", { t: "error", code: error.code });
  finish(expected.size ? 1 : 0);
});
socket.on("close", () => finish(expected.size && !args.stay ? 1 : 0));
