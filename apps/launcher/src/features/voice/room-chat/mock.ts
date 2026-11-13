// Room chat in the browser preview (SQOL §1.3): nothing leaves the page. The people who write are
// the voice mock's room's own (its first three, and the player), so any room scenario works:
// `voice=crew&social=team` (the crew's "Genel"), `voice=room` (a friend room of four), `voice=full`
// (ten), `voice=shareWatch` (watching a stream: the panel beside the stage), `voice=call1` (a call
// with KorAtesi: with `social=chat` the panel shows their conversation instead).
//
// `rchat=<scenario>`, read once from the page URL's query or the route's:
// - `busy`: the room has talked a while: words, a link, a sticker, a few emoji, one message a
//   moderator took away;
// - `empty`: nothing written yet;
// - `refusals`: the room refused three of the player's messages (a phone number, a short link, too
//   fast);
// - `peek`: nothing yet; three messages come 0.9, 1.6 and 2.3 s after the start (the previews);
// - `live`: as `busy`, and someone writes every 6 s;
// - `off` (or none): the room has no chat.
// `roomPanel=1` opens the room panel at the start. A message with "hızlı" or "fast" in it is
// refused as too fast; anything else goes after 0.3 s.
import { useVoiceUi } from "@/features/voice/store";
import { pick } from "@/lib/api/mock-lang";
import { param } from "@/share/param";
import type {
  RoomChatApi,
  RoomChatEvent,
  RoomChatMsg,
  RoomChatRefusal,
  RoomChatSend,
} from "./types";

const ME = "100";

/** The preview's room chat scenario (none: the room has no chat). */
export function mockScenario(): string | null {
  const scenario = param("rchat");
  return scenario && scenario !== "off" ? scenario : null;
}

/** Who writes in the preview's room: its first three, else the voice mock's friends. */
function writers(): [string, string, string] {
  const ids = useVoiceUi.getState().state.room?.people.map((p) => p.id) ?? [];
  const [a = "301", b = "303", c = "909"] = ids;
  return [a, b, c];
}

let counter = 0;
const nextId = () => `m${Date.now().toString(36)}${(counter++).toString(36)}`;

function history(scenario: string): RoomChatMsg[] {
  if (scenario !== "busy" && scenario !== "live" && scenario !== "refusals") return [];
  const [a, b, c] = writers();
  const now = Date.now();
  const at = (minutesAgo: number) => now - minutesAgo * 60_000;
  const say = (from: string, minutesAgo: number, tr: string, en: string): RoomChatMsg => ({
    id: nextId(),
    from,
    at: at(minutesAgo),
    text: pick(tr, en),
  });
  if (scenario === "refusals") {
    return [
      say(a, 9, "Akşam Nether'e iniyor muyuz?", "Are we going down to the Nether tonight?"),
      say(ME, 8, "Evet, 9'da kapıda olun", "Yes, be at the portal at 9"),
    ];
  }
  return [
    say(a, 41, "Kale duvarını kim yapıyor?", "Who's building the castle wall?"),
    say(b, 40, "Ben taş getiriyorum, 5 dk", "I'm bringing stone, 5 min"),
    say(ME, 39, "Ben de kapıyı yapayım o zaman", "I'll do the gate then"),
    say(ME, 39, "Kırmızı taş mı koyalım?", "Shall we use red stone?"),
    { id: nextId(), from: c, at: at(30), text: "", sticker: "pl1/gg" },
    say(
      a,
      22,
      "Bu yapıya bak: https://modrinth.com/mod/sodium",
      "Look at this build: https://modrinth.com/mod/sodium",
    ),
    say(b, 21, "😂😂", "😂😂"),
    say(c, 12, "Bunu kimse görmesin", "Nobody should see this"),
    say(a, 4, "Akşam 9'da Nether'e iniyoruz, geç kalmayın", "Nether at 9 tonight, don't be late"),
    say(b, 2, "Tamam, ben iksirleri hazırlarım", "OK, I'll make the potions"),
  ];
}

const listeners = new Set<(event: RoomChatEvent) => void>();
const emit = (event: RoomChatEvent) => {
  for (const listener of [...listeners]) listener(event);
};

/** What the room says now and then in the `peek` and `live` scenarios. */
function liveTalk(room: string, scenario: string) {
  const [a, b, c] = writers();
  const lines: Array<[string, string, string]> = [
    [a, "Kapıdayım, gelin", "I'm at the portal, come"],
    [b, "Bir dakika, zırhımı alıyorum", "One minute, grabbing my armour"],
    [c, "Ben de geliyorum!", "Coming too!"],
  ];
  const say = (index: number) => {
    const [from, tr, en] = lines[index % lines.length] ?? lines[0] ?? [a, "", ""];
    emit({
      room,
      kind: "msg",
      msg: { id: nextId(), from, at: Date.now(), text: pick(tr, en) },
    });
  };
  if (scenario === "peek") {
    [900, 1600, 2300].forEach((wait, index) => {
      setTimeout(() => say(index), wait);
    });
  } else if (scenario === "live") {
    let index = 0;
    setInterval(() => say(index++), 6_000);
  }
}

const started = new Set<string>();

export const mockRoomChat: RoomChatApi = {
  log: async (room) => {
    const scenario = mockScenario();
    if (!scenario) return [];
    const msgs = history(scenario);
    if (!started.has(room)) {
      started.add(room);
      // A moderator took one away a while ago (its tombstone shows).
      const taken = msgs.find((m) => m.text.includes("görmesin") || m.text.includes("see this"));
      if (taken) setTimeout(() => emit({ room, kind: "del", id: taken.id, by: "mod" }), 0);
      liveTalk(room, scenario);
    }
    return msgs;
  },
  send: async (_room, body): Promise<RoomChatSend> => {
    await new Promise((done) => setTimeout(done, 300));
    const words = (body.text ?? "").toLocaleLowerCase("tr");
    if (mockScenario() === "refusals" || words.includes("hızlı") || words.includes("fast")) {
      return { refused: { code: "rateLimited" } };
    }
    return { sent: { id: nextId(), at: Date.now() } };
  },
  remove: async (room, id) => {
    await new Promise((done) => setTimeout(done, 200));
    emit({ room, kind: "del", id, by: "self" });
  },
  report: async () => {
    await new Promise((done) => setTimeout(done, 300));
  },
  typing: async () => {},
  onEvent: (listener) => {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  },
};

/** The `refusals` scenario's messages the room did not take, as the player wrote them. */
export function mockRefused(): Array<{ text: string; refusal: RoomChatRefusal }> {
  if (mockScenario() !== "refusals") return [];
  return [
    {
      text: pick("Numaram 0532 123 45 67, yaz bana", "My number is 0532 123 45 67, text me"),
      refusal: { code: "rejected", reason: "phone" },
    },
    {
      text: "https://bit.ly/3kale",
      refusal: { code: "rejected", reason: "link", rule: "shortener" },
    },
    { text: pick("Hadi hadi hadi", "Come on come on"), refusal: { code: "rateLimited" } },
  ];
}
