// The browser preview's `discoverApi` (CREW-DISCOVER): invented crews (no real names) in a fixed
// order, joins that answer as the service would, and the leader's block in any state.
//
// - `&discover=1` on a crew scenario of mock-social.ts turns the feature on (`teamFeatures`);
// - `&discoverList=empty|off|slow` the listing: none listed, the switch went off, a slow answer
//   (the skeleton);
// - `&discoverState=off|waiting|listed|change|refused|closed|held|notready|edits` the leader's block.
import type {
  DiscoverApi,
  DiscoverCrew,
  DiscoverListing,
  DiscoverStatus,
  DiscoverTag,
} from "./discover-types";
import { pick } from "./mock-lang";

function param(key: string): string | null {
  if (typeof window === "undefined") return null;
  const fromSearch = new URLSearchParams(window.location.search).get(key);
  const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
  return fromSearch ?? fromHash;
}

const nowSec = () => Math.floor(Date.now() / 1000);
const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** The next 18:00 in Turkey (UTC+3), as the service counts it. */
function nextWave(): number {
  const now = nowSec() + 60;
  const dayStart = Math.floor((now + 3 * 3600) / 86_400) * 86_400 - 3 * 3600;
  const today = dayStart + 18 * 3600;
  return today > now ? today : today + 86_400;
}

type Seed = [
  id: string,
  name: string,
  tag: string,
  emblem: string,
  color: string,
  frame: string,
  tr: string,
  en: string,
  tags: DiscoverTag[],
  join: "open" | "approval",
  members: number,
  active: boolean,
];

const SEEDS: Seed[] = [
  [
    "a1b2c3d4e5f60718",
    "Demir Kurtlar",
    "DKR",
    "paw",
    "silver",
    "shield",
    "Akşamları SkyBlock'ta adamızı büyütüyoruz. Yardımlaşmayı seven, sakin oyuncular arıyoruz.",
    "We grow our island on SkyBlock in the evenings. Looking for calm players who like to help out.",
    ["skyblock", "newcomers"],
    "open",
    12,
    true,
  ],
  [
    "b2c3d4e5f6071829",
    "Kızıl Tilkiler",
    "KZL",
    "flame",
    "crimson",
    "cut",
    "Survival'da büyük bir köy kuruyoruz. Her cuma birlikte madene iniyoruz.",
    "We're building a big village on Survival. Every Friday we go mining together.",
    ["survival", "build", "events"],
    "approval",
    21,
    true,
  ],
  [
    "c3d4e5f60718293a",
    "Ada Kurucuları",
    "ADA",
    "waves",
    "teal",
    "hex",
    "Yeni başlayanlara SkyBlock'ta ilk adayı birlikte kuruyoruz. Sesli sohbette soru sorabilirsin.",
    "We help new players set up their first SkyBlock island. Ask anything in voice chat.",
    ["skyblock", "voice", "newcomers"],
    "open",
    7,
    true,
  ],
  [
    "d4e5f60718293a4b",
    "Gece Vardiyası",
    "GVR",
    "moon",
    "navy",
    "banner",
    "Gece geç saatte oynayanlar için. HG'de takım kurup turnuvalara giriyoruz.",
    "For players who stay up late. We team up on HG and enter tournaments.",
    ["hg", "pvp"],
    "approval",
    9,
    false,
  ],
  [
    "e5f60718293a4b5c",
    "Yıldız Madencileri",
    "YLD",
    "star",
    "gold",
    "cut",
    "SMP'de ekonomi ve ticaret yapıyoruz. Pazar akşamları ekipçe yeni bölge keşfediyoruz.",
    "We trade and run a little economy on SMP. On Sunday evenings we explore a new area together.",
    ["smp", "events"],
    "open",
    15,
    true,
  ],
  [
    "f60718293a4b5c6d",
    "Çam Ormanı",
    "CAM",
    "tree",
    "green",
    "shield",
    "Sohbet etmeyi ve birlikte yapı yapmayı seven küçük bir ekibiz.",
    "A small crew that likes to chat and build together.",
    ["chat", "build"],
    "open",
    4,
    true,
  ],
  [
    "0718293a4b5c6d7e",
    "Buz Kalesi",
    "BUZ",
    "snowflake",
    "blue",
    "hex",
    "PvP antrenmanı yapıyoruz. Haftada iki kez ekip içi turnuva var.",
    "We practise PvP. There's a crew tournament twice a week.",
    ["pvp", "events"],
    "approval",
    18,
    true,
  ],
  [
    "18293a4b5c6d7e8f",
    "Mor Bulut",
    "MOR",
    "planet",
    "purple",
    "banner",
    "Survival ve SkyBlock arasında gidip geliyoruz, kimseyi yalnız bırakmıyoruz.",
    "We switch between Survival and SkyBlock and never leave anyone behind.",
    ["survival", "skyblock", "chat"],
    "open",
    26,
    false,
  ],
  [
    "293a4b5c6d7e8f90",
    "Kum Saati",
    "KUM",
    "cactus",
    "copper",
    "cut",
    "Yavaş ama düzenli oynuyoruz. Hafta sonları SMP'de birlikte çiftlik kuruyoruz.",
    "We play slowly but steadily. On weekends we build farms together on SMP.",
    ["smp", "build"],
    "approval",
    6,
    true,
  ],
];

function listing(): DiscoverListing {
  const crews: DiscoverCrew[] = SEEDS.map(
    ([id, name, tag, emblem, color, frame, tr, en, tags, join, members, active]) => ({
      id,
      name,
      tag,
      emblem,
      color,
      frame,
      motto: "",
      text: pick(tr, en),
      tags,
      join,
      members,
      // CREW-CAP: the default cap; Mor Bulut has a small one of its own, so its card shows the
      // cap beside the count (nearly full).
      maxMembers: id === "18293a4b5c6d7e8f" ? 28 : 100,
      active,
    }),
  );
  return { ver: 3, at: nowSec() - 3 * 3600, next: nextWave(), crews };
}

const off = () =>
  Object.assign(new Error(pick("Ekip bul şu an kapalı.", "Find a crew is off right now.")), {
    code: "SOSYAL-KESIF-KAPALI",
    details: "Önizleme verisi",
  });

/** The leader's block in the state `&discoverState=` asks for. */
function status(): DiscoverStatus {
  const state = param("discoverState") ?? "off";
  const draft = {
    text: pick(
      "Akşamları SkyBlock'ta adamızı büyütüyoruz. Yardımlaşmayı seven, sakin oyuncular arıyoruz.",
      "We grow our island on SkyBlock in the evenings. Looking for calm players who like to help out.",
    ),
    tags: ["skyblock", "newcomers"] as DiscoverTag[],
  };
  const checks = {
    members: { need: 3, have: state === "notready" ? 2 : 12 },
    crewDays: { need: 3, have: state === "notready" ? 1 : 30 },
    leaderDays: { need: 14, have: 200 },
    visible: true,
  };
  const base: DiscoverStatus = {
    state: "off",
    listed: null,
    draft: null,
    changeWaits: false,
    waveAt: nextWave(),
    retryAt: null,
    checks,
    editsLeft: state === "edits" ? 0 : 4,
    editsReset: nextWave() + 3 * 86_400,
  };
  switch (state) {
    case "waiting":
      return { ...base, state: "waiting", draft };
    case "listed":
    case "edits":
      return { ...base, state: "listed", listed: draft, draft };
    case "change":
      return {
        ...base,
        state: "listed",
        listed: draft,
        draft: { ...draft, tags: ["skyblock", "voice"] },
        changeWaits: true,
      };
    case "refused":
      return { ...base, state: "refused", draft, retryAt: nowSec() + 6 * 86_400 + 5 * 3600 };
    case "closed":
      return { ...base, state: "closed", draft };
    case "held":
      return { ...base, state: "held", listed: draft, draft };
    default:
      return base;
  }
}

let current: DiscoverStatus | null = null;

export const mockDiscover: DiscoverApi = {
  async list(etag) {
    const mode = param("discoverList");
    await wait(mode === "slow" ? 60_000 : 280);
    if (mode === "off") throw off();
    const page = listing();
    if (mode === "empty") page.crews = [];
    const tag = `"d${page.ver}.${page.next}"`;
    return etag === tag ? null : { listing: page, etag: tag };
  },
  async join(teamId) {
    await wait(420);
    const crew = SEEDS.find((seed) => seed[0] === teamId);
    if (!crew) return "gone";
    return crew[9] === "open" ? "joined" : "requested";
  },
  async status() {
    await wait(220);
    current ??= status();
    return current;
  },
  async set(_teamId, text, tags) {
    await wait(380);
    const now = current ?? status();
    current =
      now.state === "listed"
        ? { ...now, draft: { text, tags }, changeWaits: true, editsLeft: now.editsLeft - 1 }
        : {
            ...now,
            state: "waiting",
            draft: { text, tags },
            editsLeft: Math.max(0, now.editsLeft - 1),
          };
    return current;
  },
  async off() {
    await wait(260);
    const now = current ?? status();
    current = { ...now, state: "off", listed: null, changeWaits: false };
    return current;
  },
};
