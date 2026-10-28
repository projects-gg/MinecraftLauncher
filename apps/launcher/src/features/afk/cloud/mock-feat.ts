// Browser previews of the cloud features (CAFK-FEAT phase 1), for mock.ts. Preview switches (read
// once, from the page URL), on top of `?cloud=accounts`:
// - `&feat=on`: Studio switched everything on (inventory `full`); `&feat=view`: the inventory is
//   view-only; without `feat` the service is one from before the features (nothing of them shows);
// - `&inv=setting|restart|version|trade|chest|menu|slow`: Madenci07's inventory is off for the
//   account, waits for a reconnect, has no inventory on its version, is held by an AFK+ trade,
//   has a chest or a server menu open, or answers slowly (the empty window shows meanwhile);
// - `&legacy=1`: Madenci07's replies still match the old way;
// - `&bars=0`: no status strip.
import {
  createMockMachine,
  type InvFrame,
  type InvOp,
  type InvResult,
  type MockMachine,
  SAMPLE_CHEST,
  SAMPLE_INVENTORY,
  SAMPLE_KITS,
  type WinFrame,
} from "@pl/ui/inventory";
import type {
  CloudAccountFeatures,
  CloudConsoleLine,
  CloudFeatures,
  CloudInvItem,
  CloudLiveEvent,
  CloudReplyTry,
} from "@/bindings";
import type { TryRule } from "./api";

function param(key: string): string | null {
  try {
    const fromSearch = new URLSearchParams(window.location.search).get(key);
    const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
    return fromSearch ?? fromHash;
  } catch {
    return null;
  }
}

export const featScenario: "off" | "on" | "view" =
  param("feat") === "on" ? "on" : param("feat") === "view" ? "view" : "off";
const invScenario = param("inv");
export const MADENCI = "a_7KQ2MZ3P9TW4XB1C";

export function offerFeatures(): CloudFeatures | null {
  if (featScenario === "off") return null;
  return {
    inventory: featScenario === "view" ? "view" : "full",
    consoleKinds: true,
    placeholders: true,
    placeholderCommands: ["msg", "m", "tell", "w", "whisper", "r", "reply", "pm"],
  };
}

export function accountFeatures(id: string): CloudAccountFeatures | null {
  const offer = offerFeatures();
  if (!offer) return null;
  return {
    inventory: {
      mode: offer.inventory,
      on: !(id === MADENCI && invScenario === "setting"),
    },
    consoleKinds: true,
    placeholders: true,
    legacyRules: id === MADENCI && param("legacy") === "1",
  };
}

type Kind = [k: string, c: string, text: string, who?: string];

/** The console as the machine writes it with kinds (a projects.gg evening). */
const KIND_LINES: Kind[] = [
  ["n", "cli", "Sunucuya bağlanılıyor: play.projects.gg"],
  ["n", "sys", "[Projects] Sunucuya hoş geldin!"],
  ["s", "sys", "/login ******"],
  ["n", "sys", "[AuthMe] Başarıyla giriş yaptın."],
  ["s", "sys", "/warp maden"],
  ["n", "sys", "Maden bölgesine ışınlandın."],
  ["n", "chat", "GeceKurdu_ » selam millet", "GeceKurdu_"],
  ["n", "pm", "✉ »» ElmasKazma: Madenci07 akşam pvp var mı", "ElmasKazma"],
  ["n", "chat", "SessizOkcu » Madenci07 maden açık mı", "SessizOkcu"],
  ["n", "title", "Hoş geldin"],
  ["n", "cli", "[Envanter] Menü açıldı: Kitler"],
  ["n", "sys", "[Duyuru] Etkinlik 21:00'de başlıyor!"],
  ["e", "err", "Sunucuyla bağlantı bir an yavaşladı."],
  ["n", "chat", "Lavci_42 » biri pazara gelsin", "Lavci_42"],
];

const LIVE_LINES: Kind[] = [
  ["n", "chat", "GeceKurdu_ » biri pazara gelsin", "GeceKurdu_"],
  ["n", "sys", "[Duyuru] Haftalık sıralama yenilendi."],
  ["n", "pm", "✉ »» SessizOkcu: hoş geldin Madenci07", "SessizOkcu"],
  ["n", "sys", "[Projects] 15 dakika AFK ödülü: 120 altın."],
];

/**
 * A line in the account's own console: its name, the server it joins (`host` or `host:port`), and
 * a server other than projects.gg sends no "[Projects]" lines.
 */
function kindLine(
  [k, c, text, who]: Kind,
  at: number,
  name: string,
  server: string,
): CloudConsoleLine {
  const words = text.replace("Madenci07", name).replace("play.projects.gg", server);
  const projects = /(^|\.)projects\.gg$/.test(server.split(":")[0] ?? "");
  return {
    at,
    text: projects ? words : words.replace("[Projects] ", ""),
    k,
    c,
    who: who ?? null,
    goto: text.startsWith("[Envanter] Menü açıldı") ? "inventory" : null,
  };
}

export function kindLines(
  count: number,
  endAt: number,
  name: string,
  server: string,
): CloudConsoleLine[] {
  return Array.from({ length: count }, (_, i) =>
    kindLine(
      KIND_LINES[i % KIND_LINES.length] ?? ["n", "sys", ""],
      endAt - (count - i) * 37,
      name,
      server,
    ),
  );
}

export function liveKindLine(
  n: number,
  at: number,
  name: string,
  server: string,
): CloudConsoleLine {
  return kindLine(LIVE_LINES[n % LIVE_LINES.length] ?? ["n", "sys", ""], at, name, server);
}

/** The strip right after a watch: a timer on the action bar and an event's boss bar. */
export function barsEvent(acct: string): CloudLiveEvent | null {
  if (featScenario === "off" || param("bars") === "0") return null;
  const now = Date.now();
  return {
    kind: "bars",
    acct,
    bars: {
      actionbar: { text: "Günlük ödüle 2 saat 14 dakika", untilMs: now + 60 * 60_000 },
      title: null,
      boss: [
        { id: "b1", text: "Ejderha etkinliği", progress: 0.62, color: "purple" },
        { id: "b2", text: "Sunucu yeniden başlıyor: 14 dakika", progress: 0.3, color: "red" },
      ],
    },
  };
}

// --- the inventory ---------------------------------------------------------------------------

function toCloudItem(item: InvFrame["items"][number]): CloudInvItem {
  return {
    slot: item.slot,
    id: item.id,
    count: item.count,
    label: item.label,
    name: item.name ?? null,
    lore: item.lore ?? [],
    durability: item.durability ?? null,
    cmd: item.cmd ?? null,
    model: item.model ?? null,
    ench: item.ench ?? [],
    enchanted: item.enchanted ?? false,
    inside: item.inside
      ? {
          count: item.inside.count,
          top: item.inside.top.map((s) => ({ ...s, cmd: null, model: null })),
        }
      : null,
    potion: item.potion ?? null,
    color: item.color ?? null,
    partial: item.partial ?? false,
  };
}

function inventoryEvent(acct: string, frame: InvFrame, snap: boolean): CloudLiveEvent {
  return {
    kind: "inventory",
    acct,
    held: frame.held,
    cursor: frame.cursor ? toCloudItem({ slot: 0, ...frame.cursor }) : null,
    items: frame.items.map(toCloudItem),
    snap,
  };
}

function windowEvent(acct: string, win: WinFrame | null, closedSeq: number): CloudLiveEvent {
  return win
    ? {
        kind: "window",
        acct,
        wseq: win.wseq,
        open: win.open,
        items: win.items.map(toCloudItem),
        closed: false,
      }
    : { kind: "window", acct, wseq: closedSeq, open: null, items: [], closed: true };
}

const machines = new Map<string, MockMachine>();

/** The account's machine, made on first watch (Madenci07 per `&inv`; others a plain inventory). */
function machineFor(acct: string): MockMachine {
  const known = machines.get(acct);
  if (known) return known;
  const window =
    acct === MADENCI && invScenario === "chest"
      ? SAMPLE_CHEST
      : acct === MADENCI && invScenario === "menu"
        ? SAMPLE_KITS
        : null;
  const machine = createMockMachine({
    inventory: SAMPLE_INVENTORY,
    window,
    windowKind: invScenario === "menu" ? "menu" : "storage",
    mode: featScenario === "view" ? "view" : "full",
    trade: acct === MADENCI && invScenario === "trade",
    latency: invScenario === "slow" ? 900 : 180,
  });
  machines.set(acct, machine);
  return machine;
}

const subscriptions = new Map<string, () => void>();

/** `watch` with `inv`: the state, then the frames (and every change after them). */
export function watchInventory(
  acct: string,
  running: boolean,
  emit: (event: CloudLiveEvent) => void,
) {
  const off = (reason: string) =>
    emit({ kind: "invState", acct, state: "off", reason, lock: null });
  const features = accountFeatures(acct);
  if (!features || features.inventory?.mode === "off") return off("service");
  if (!features.inventory?.on) return off("setting");
  if (!running) return off("not_running");
  if (acct === MADENCI && invScenario === "restart") return off("restart");
  if (acct === MADENCI && invScenario === "version") return off("version");
  const machine = machineFor(acct);
  const delay = invScenario === "slow" ? 2_400 : 180;
  window.setTimeout(() => {
    emit({
      kind: "invState",
      acct,
      state: "ok",
      reason: null,
      lock: acct === MADENCI && invScenario === "trade" ? "trade" : null,
    });
    emit(inventoryEvent(acct, machine.inventory(), true));
    const open = machine.window();
    if (open) emit(windowEvent(acct, open, open.wseq));
  }, delay);
  subscriptions.get(acct)?.();
  let shownSeq = machine.window()?.wseq ?? 0;
  subscriptions.set(
    acct,
    machine.subscribe((inventory, win) => {
      emit(inventoryEvent(acct, inventory, false));
      emit(windowEvent(acct, win, shownSeq));
      shownSeq = win?.wseq ?? shownSeq;
    }),
  );
}

export function unwatchInventory(acct: string) {
  subscriptions.get(acct)?.();
  subscriptions.delete(acct);
}

export async function inventoryMove(acct: string, op: InvOp): Promise<InvResult> {
  return machineFor(acct).op(op);
}

// --- "Dene" without the backend -------------------------------------------------------------

const PLACEHOLDER = /\{(u|[1-9])\}/g;
const VALUE = /^[\p{L}\p{N}_]{1,24}$/u;

/** The preview's reading of a rule (JavaScript's regex; the launcher's is Rust's). */
export function mockTryReply(rule: TryRule, sample: string, sender: string): CloudReplyTry {
  const pattern = rule.pattern.trim();
  if (!pattern) return { kind: "pattern" };
  let groups: string[] = [];
  if (rule.regex) {
    let compiled: RegExp;
    try {
      const insensitive = pattern.startsWith("(?i)");
      compiled = new RegExp(insensitive ? pattern.slice(4) : pattern, insensitive ? "iu" : "u");
    } catch {
      return { kind: "pattern" };
    }
    const match = compiled.exec(sample);
    if (!match) return { kind: "noMatch" };
    groups = [...match].map((g) => g ?? "");
  } else if (!sample.toLowerCase().includes(pattern.toLowerCase())) {
    return { kind: "noMatch" };
  }
  const value = (name: string) =>
    name === "u" ? (rule.from === "server" ? "" : sender) : (groups[Number(name)] ?? "");
  for (const [, name] of rule.reply.matchAll(PLACEHOLDER)) {
    if (!VALUE.test(value(name ?? ""))) return { kind: "value" };
  }
  return { kind: "matches", reply: rule.reply.replace(PLACEHOLDER, (_, name) => value(name)) };
}
