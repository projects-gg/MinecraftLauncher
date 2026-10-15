// Simulated AFK sessions for previews: states move like the real MCC bridge would report them.
import type { AfkAccount, AfkLogLine } from "./types";

const listeners = new Set<() => void>();
const notify = () => {
  for (const l of listeners) l();
};

let lineId = 0;
const now = () => new Date().toISOString();

const accounts: AfkAccount[] = [
  {
    id: "afk1",
    nickname: "KasaBekcisi",
    server: "play.projects.gg",
    autoStart: true,
    mcVersion: "26.3",
    state: { kind: "connected", since: new Date(Date.now() - 65 * 60_000).toISOString() },
    health: 20,
    food: 18,
    players: 214,
  },
  {
    id: "afk2",
    nickname: "TarlaBot_1",
    server: "play.projects.gg",
    autoStart: true,
    mcVersion: "26.3",
    state: {
      kind: "reconnecting",
      attempt: 2,
      nextAttemptAt: new Date(Date.now() + 22_000).toISOString(),
    },
  },
  {
    id: "afk3",
    nickname: "Madenci07",
    server: "play.projects.gg",
    autoStart: false,
    mcVersion: "auto",
    state: { kind: "stopped" },
  },
];

const logs = new Map<string, AfkLogLine[]>([
  [
    "afk1",
    [
      {
        id: ++lineId,
        time: now(),
        text: "Sunucuya bağlanılıyor: play.projects.gg",
        kind: "system",
      },
      { id: ++lineId, time: now(), text: "/login ********", kind: "script" },
      { id: ++lineId, time: now(), text: "Başarıyla giriş yaptın.", kind: "chat" },
      { id: ++lineId, time: now(), text: "<AlevKanat> akşam boss'a gelen var mı?", kind: "chat" },
      { id: ++lineId, time: now(), text: "<Demirci> ben varım", kind: "chat" },
    ],
  ],
]);

function push(id: string, text: string, kind: AfkLogLine["kind"]) {
  const list = logs.get(id) ?? [];
  list.push({ id: ++lineId, time: now(), text, kind });
  if (list.length > 500) list.splice(0, list.length - 500);
  logs.set(id, list);
}

function setState(account: AfkAccount, state: AfkAccount["state"]) {
  account.state = state;
  notify();
}

async function connect(account: AfkAccount) {
  setState(account, { kind: "starting" });
  await new Promise((r) => setTimeout(r, 700));
  setState(account, { kind: "connecting" });
  push(account.id, `Sunucuya bağlanılıyor: ${account.server}`, "system");
  await new Promise((r) => setTimeout(r, 1400));
  account.health = 20;
  account.food = 20;
  account.players = 211;
  setState(account, { kind: "connected", since: now() });
  push(account.id, "Başarıyla giriş yaptın.", "chat");
}

export const mockAfk = {
  async afkAccounts() {
    return accounts.map((a) => ({ ...a }));
  },
  async afkAdd(nickname: string) {
    const account: AfkAccount = {
      id: `afk-${nickname}`,
      nickname,
      server: "play.projects.gg",
      autoStart: false,
      mcVersion: "26.3",
      state: { kind: "stopped" },
    };
    accounts.push(account);
    notify();
    return account;
  },
  async afkRemove(id: string) {
    const index = accounts.findIndex((a) => a.id === id);
    if (index >= 0) accounts.splice(index, 1);
    notify();
  },
  async afkConnect(id: string) {
    const account = accounts.find((a) => a.id === id);
    if (account) void connect(account);
  },
  async afkStop(id: string) {
    const account = accounts.find((a) => a.id === id);
    if (!account) return;
    setState(account, { kind: "stopping" });
    push(id, "/quit", "self");
    setTimeout(() => setState(account, { kind: "stopped" }), 900);
  },
  async afkConnectAll() {
    const waiting = accounts.filter((a) => a.state.kind === "stopped" || a.state.kind === "error");
    waiting.forEach((account, index) => {
      if (index === 0) {
        void connect(account);
        return;
      }
      setState(account, { kind: "queued", inSecs: index * 15 });
      setTimeout(() => void connect(account), index * 15_000);
    });
  },
  async afkStopAll() {
    for (const a of accounts) if (a.state.kind !== "stopped") void mockAfk.afkStop(a.id);
  },
  async afkSend(id: string, line: string) {
    push(id, `> ${line}`, "self");
    notify();
  },
  async afkLog(id: string) {
    return [...(logs.get(id) ?? [])];
  },
  onAfkChange(listener: () => void) {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  },
};
