// Browser preview of the account calls: several Microsoft and offline accounts, an expired
// session, a very long name, painted pixel heads, a bundled picture, two pinned accounts,
// undoable removal and every Microsoft sign-in outcome.
//
// Preview switches (read when the call runs):
// - `?accounts=empty` in the page URL starts with no accounts; `?accounts=plain` with no pins
//   and no pictures.
// - `window.__plMockLogin = "<scenario>"` picks how the next Microsoft sign-in ends: "added"
//   (default), "already", "other", "timeout", "noLicense", "noProfile", "network", "child".
import { pick } from "./mock-lang";
import type { AccountSummary, LoginStep, MicrosoftLoginResult, SessionCheck } from "./types";

/** Same shape as the app's command errors, so the UI shows the friendly message. */
class MockCommandError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.details = `Kod: ${code}\nÖnizleme verisi`;
  }
}

// The errors the calls below share, worded like the backend (`pl_auth::AuthError::user_message`).
const accountNotFound = () =>
  new MockCommandError(
    "HESAP-YOK",
    pick(
      "Hesap bulunamadı. Hesap listesi değişmiş olabilir, tekrar dene.",
      "Account not found. The account list may have changed; try again.",
    ),
  );
const nameTaken = () =>
  new MockCommandError(
    "AD-VAR",
    pick("Bu adla bir hesabın zaten var.", "You already have an account with this name."),
  );
const signInCancelled = () =>
  new MockCommandError("IPTAL", pick("Giriş iptal edildi.", "Sign-in cancelled."));

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const ago = (hours: number) => new Date(Date.now() - hours * 3_600_000).toISOString();

interface HeadPalette {
  hair: string;
  skin: string;
  eyes: string;
  mouth: string;
  /** Optional hat or hood colour over the top rows. */
  hat?: string;
}

/** Deterministic noise so painted heads look like pixel art, not flat fills. */
function shade(color: string, x: number, y: number): string {
  const n = Math.sin(x * 12.9898 + y * 78.233) * 43758.5453;
  const delta = Math.round(((n - Math.floor(n)) * 2 - 1) * 12);
  const value = Number.parseInt(color.slice(1), 16);
  const channel = (shift: number) =>
    Math.max(0, Math.min(255, ((value >> shift) & 255) + delta))
      .toString(16)
      .padStart(2, "0");
  return `#${channel(16)}${channel(8)}${channel(0)}`;
}

/** An 8×8 face as a PNG data URL (what the app's backend cuts from a skin). */
function paintHead(p: HeadPalette): string | undefined {
  if (typeof document === "undefined") return undefined;
  const canvas = document.createElement("canvas");
  canvas.width = 8;
  canvas.height = 8;
  const ctx = canvas.getContext("2d");
  if (!ctx) return undefined;
  const px = (x: number, y: number, color: string) => {
    ctx.fillStyle = shade(color, x, y);
    ctx.fillRect(x, y, 1, 1);
  };
  for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) px(x, y, p.skin);
  for (let x = 0; x < 8; x++) {
    px(x, 0, p.hat ?? p.hair);
    px(x, 1, p.hat ?? p.hair);
  }
  for (const [x, y] of [
    [0, 2],
    [7, 2],
    [0, 3],
    [7, 3],
  ] as const)
    px(x, y, p.hair);
  if (p.hat) for (const x of [0, 7]) for (let y = 2; y < 6; y++) px(x, y, p.hat);
  for (const x of [1, 5]) {
    px(x, 4, "#f3ebdd");
    px(x + 1, 4, p.eyes);
  }
  px(3, 6, p.mouth);
  px(4, 6, p.mouth);
  return canvas.toDataURL("image/png");
}

const palettes = {
  steve: { hair: "#2f1f12", skin: "#b5835f", eyes: "#4a3d8f", mouth: "#6b3a2a" },
  ember: { hair: "#5a1d0e", skin: "#c99673", eyes: "#2a6e4f", mouth: "#7a2416", hat: "#7a2416" },
  frost: { hair: "#d9d4cc", skin: "#e0b391", eyes: "#2f6f9e", mouth: "#9c5a4a" },
  night: { hair: "#15121a", skin: "#8d5a3d", eyes: "#3fa6a6", mouth: "#4a2418", hat: "#26303d" },
  gold: { hair: "#7a4b12", skin: "#d8a77e", eyes: "#3b2c1f", mouth: "#8e4a2a" },
} satisfies Record<string, HeadPalette>;

interface MockAccount extends AccountSummary {
  palette?: keyof typeof palettes;
}

function initialAccounts(): MockAccount[] {
  const mode =
    typeof window === "undefined"
      ? null
      : new URLSearchParams(window.location.search).get("accounts");
  if (mode === "empty") return [];
  const plain = mode === "plain";
  return [
    {
      id: "a1",
      kind: "offline",
      username: "EjderAvcisi",
      active: true,
      addedAt: ago(24 * 40),
      lastUsedAt: ago(2),
      palette: "steve",
      avatar: plain ? undefined : "preset:golden-dragon",
    },
    {
      id: "a2",
      kind: "microsoft",
      username: "KorAtesi_",
      active: false,
      addedAt: ago(24 * 90),
      lastUsedAt: ago(26),
      palette: "ember",
      pinned: !plain,
    },
    {
      id: "a3",
      kind: "microsoft",
      username: "AlevKanat",
      active: false,
      addedAt: ago(24 * 120),
      lastUsedAt: ago(24 * 12),
      needsRelogin: true,
      palette: "night",
    },
    {
      // Same name as the connected AFK session "KasaBekcisi" (mock-afk.ts).
      id: "a4",
      kind: "offline",
      username: "KasaBekcisi",
      active: false,
      addedAt: ago(24 * 20),
      palette: "frost",
      pinned: !plain,
    },
    {
      // A 16-character name, and no head: no client jar on this computer yet.
      id: "a5",
      kind: "offline",
      username: "UzunIsimliOyuncu",
      active: false,
      addedAt: ago(24 * 3),
    },
  ];
}

const accounts: MockAccount[] = initialAccounts();
const removed = new Map<string, { account: MockAccount; index: number; wasActive: boolean }>();
let cancelled = false;
let nextId = 1;

function summary(account: MockAccount): AccountSummary {
  const { palette, ...rest } = account;
  return { ...rest, headUrl: palette ? heads(palette) : undefined };
}

const headMemo = new Map<string, string | undefined>();
function heads(palette: keyof typeof palettes): string | undefined {
  if (!headMemo.has(palette)) headMemo.set(palette, paintHead(palettes[palette]));
  return headMemo.get(palette);
}

/** The most recently played remaining account, as the backend picks it. */
function mostRecentlyUsed(): MockAccount | undefined {
  return [...accounts].sort((a, b) => {
    const played = (b.lastUsedAt ?? "").localeCompare(a.lastUsedAt ?? "");
    return played || (b.addedAt ?? "").localeCompare(a.addedAt ?? "");
  })[0];
}

function select(id: string | null) {
  for (const a of accounts) a.active = a.id === id;
}

function scenario(): string {
  return (globalThis as { __plMockLogin?: string }).__plMockLogin ?? "added";
}

/** Names of offline accounts, for the AFK preview and the skin panel. */
export function mockAccountName(id: string): string | undefined {
  return accounts.find((a) => a.id === id)?.username;
}

export const mockAccounts = {
  async accounts() {
    return accounts.map(summary);
  },
  async addOfflineAccount(name: string) {
    await delay(220);
    const taken = accounts.find(
      (a) => a.kind === "offline" && a.username.toLowerCase() === name.toLowerCase(),
    );
    if (taken && taken.username !== name) throw nameTaken();
    if (taken) {
      select(taken.id);
      return summary(taken);
    }
    const account: MockAccount = {
      id: `off-${nextId++}`,
      kind: "offline",
      username: name,
      active: true,
      addedAt: new Date().toISOString(),
      palette: "gold",
    };
    accounts.push(account);
    select(account.id);
    return summary(account);
  },
  async loginMicrosoft(
    onStep: (step: LoginStep) => void,
    options?: { relogin?: string },
  ): Promise<MicrosoftLoginResult> {
    cancelled = false;
    const ending = scenario();
    // Step, code, then the Turkish and the English message (picked when the sign-in fails).
    const fail: Record<string, [LoginStep, string, string, string]> = {
      timeout: [
        "waitingForBrowser",
        "MSA-ZAMANASIMI",
        "Giriş süresi doldu. Tarayıcıdaki adımları beş dakika içinde tamamlayıp tekrar dene.",
        "Sign-in timed out. Finish the steps in your browser within five minutes and try again.",
      ],
      network: [
        "xboxLive",
        "AG-BAGLANTI",
        "Sunuculara ulaşılamadı. İnternet bağlantını kontrol edip tekrar dene.",
        "Couldn't reach the servers. Check your internet connection and try again.",
      ],
      noLicense: [
        "profile",
        "MC-LISANSYOK",
        "Bu Microsoft hesabında Minecraft: Java Edition yok. Oyunu satın aldığın hesapla giriş yap.",
        "This Microsoft account doesn't own Minecraft: Java Edition. Sign in with the account you bought the game with.",
      ],
      noProfile: [
        "profile",
        "MC-PROFILYOK",
        "Bu hesapta Minecraft: Java Edition var ama henüz oyuncu profili yok. minecraft.net'te giriş yapıp oyuncu adını seç, sonra tekrar dene.",
        "This account owns Minecraft: Java Edition but has no player profile yet. Sign in at minecraft.net, pick your player name, then try again.",
      ],
      child: [
        "xsts",
        "XSTS-2148916238",
        "Bu hesap bir çocuk hesabı. Xbox Live'a girebilmesi için bir yetişkinin bu hesabı Microsoft aile grubuna eklemesi gerekiyor.",
        "This is a child account. To use Xbox Live, an adult needs to add it to a Microsoft family group.",
      ],
    };
    const steps: LoginStep[] = [
      "openingBrowser",
      "waitingForBrowser",
      "microsoft",
      "xboxLive",
      "xsts",
      "minecraft",
      "profile",
    ];
    for (const step of steps) {
      if (cancelled) throw signInCancelled();
      onStep(step);
      await delay(step === "waitingForBrowser" ? 2400 : 520);
      const failure = fail[ending];
      if (failure && failure[0] === step) {
        if (ending === "timeout") await delay(600);
        throw new MockCommandError(failure[1], pick(failure[2], failure[3]));
      }
    }
    if (cancelled) throw signInCancelled();
    onStep("done");

    const target = options?.relogin ? accounts.find((a) => a.id === options.relogin) : undefined;
    if (target && ending !== "other") {
      target.needsRelogin = false;
      return { account: summary(target), outcome: "relogged" };
    }
    if (ending === "already" && !target) {
      const existing = accounts.find((a) => a.kind === "microsoft") ?? accounts[0];
      if (existing) {
        existing.needsRelogin = false;
        select(existing.id);
        return { account: summary(existing), outcome: "alreadyAdded" };
      }
    }
    const account: MockAccount = {
      id: `ms-${nextId++}`,
      kind: "microsoft",
      username: target ? "BaskaHesap" : "YildizTozu",
      active: !target,
      addedAt: new Date().toISOString(),
      palette: "gold",
    };
    accounts.push(account);
    if (!target) select(account.id);
    return { account: summary(account), outcome: target ? "otherAccount" : "added" };
  },
  async cancelMicrosoftLogin() {
    cancelled = true;
  },
  async setActiveAccount(id: string) {
    await delay(60);
    select(id);
  },
  async removeAccount(id: string) {
    await delay(120);
    const index = accounts.findIndex((a) => a.id === id);
    const account = accounts[index];
    if (!account) throw accountNotFound();
    accounts.splice(index, 1);
    removed.set(id, { account, index, wasActive: account.active });
    if (account.active) {
      account.active = false;
      select(mostRecentlyUsed()?.id ?? null);
    }
    return { activeId: accounts.find((a) => a.active)?.id ?? null };
  },
  async undoRemoveAccount(id: string) {
    await delay(100);
    const entry = removed.get(id);
    if (!entry) throw accountNotFound();
    removed.delete(id);
    accounts.splice(Math.min(entry.index, accounts.length), 0, entry.account);
    if (entry.wasActive) select(entry.account.id);
    return summary(entry.account);
  },
  async forgetRemovedAccount(id: string) {
    removed.delete(id);
  },
  async renameAccount(id: string, name: string) {
    await delay(200);
    const account = accounts.find((a) => a.id === id);
    if (!account) throw accountNotFound();
    const other = accounts.find(
      (a) => a.id !== id && a.kind === "offline" && a.username.toLowerCase() === name.toLowerCase(),
    );
    if (other) throw nameTaken();
    account.username = name;
    return summary(account);
  },
  async setAccountPinned(id: string, pinned: boolean) {
    await delay(60);
    const index = accounts.findIndex((a) => a.id === id);
    const account = accounts[index];
    if (!account) throw accountNotFound();
    if (Boolean(account.pinned) !== pinned) {
      // Same rule as the backend: a new pin joins the end of the pinned ones.
      accounts.splice(index, 1);
      account.pinned = pinned;
      const at = pinned ? accounts.findLastIndex((a) => a.pinned) + 1 : index;
      accounts.splice(at, 0, account);
    }
    return summary(account);
  },
  async reorderPinnedAccounts(ids: string[]) {
    await delay(60);
    const pinned = accounts.filter((a) => a.pinned);
    const ordered = ids.flatMap((id) => pinned.filter((a) => a.id === id));
    const rest = pinned.filter((a) => !ordered.includes(a));
    const unpinned = accounts.filter((a) => !a.pinned);
    accounts.splice(0, accounts.length, ...ordered, ...rest, ...unpinned);
  },
  async setAccountAvatar(id: string, avatar: string | null) {
    await delay(80);
    const account = accounts.find((a) => a.id === id);
    if (!account) throw accountNotFound();
    // The backend refuses a picture for a Microsoft account as an unexpected request.
    if (account.kind !== "offline")
      throw new MockCommandError(
        "AVATAR",
        pick(
          "Sunucudan beklenmedik bir yanıt geldi. Tekrar dene.",
          "The server sent an unexpected answer. Try again.",
        ),
      );
    account.avatar = avatar ?? undefined;
    return summary(account);
  },
  async checkAccountSession(id: string): Promise<SessionCheck> {
    await delay(300);
    const account = accounts.find((a) => a.id === id);
    return account?.needsRelogin ? "expired" : "valid";
  },
  async ensureAccountHeads() {
    return 0;
  },
};
