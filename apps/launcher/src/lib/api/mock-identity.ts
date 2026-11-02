// Browser preview of SOC-ID (the ways into the friends account), on mock-social.ts's data.
// Scenarios (`?social=<scenario>`):
// - `signedOutMicrosoft`: the sign-in screen while Microsoft opens accounts too (both buttons as
//   equals); "Microsoft ile giriş yap" opens a new account without Discord and its name step;
// - `signedOutLinked`: the sign-in screen while Microsoft signs in only the accounts it is linked
//   to (under "Bağlı hesabınla gir", one size smaller);
// - `signingInMicrosoft`: the waiting card of a Microsoft sign-in;
// - `notLinked`: as `signedOutLinked`, and this Microsoft account is no Projects account's yet;
// - `nameStep`: a new account opened with Microsoft, its name step on screen;
// - `microsoftOnly`: signed in with an account without Discord ("Bağlı hesaplar" with Microsoft
//   alone, "Discord hesabını bağla"; no Discord photo, no Projects skin, no 7/24 Bulut);
// - `linkedBoth`: an account opened with Discord with Microsoft linked three days ago;
// - `linkedOne`: an account opened with Discord, Microsoft offered ("Microsoft hesabını bağla");
// - `identityWaiting`: a link of Microsoft waits for the browser;
// - `identityNotice`: Microsoft was linked on another device (the one-time note);
// - `limitedAccount`: a new account without Discord is limited ("Hesabın yeni olduğu için…").
// Switches: `&identityError=<reason>` makes the next link or removal fail so
// (`taken`, `proofFailed`, `tooMany`…); `&nameError=<result>` the next name (`rejected`,
// `tooMany`…); `&unlink=browser` makes a removal ask for the browser proof.
// In these scenarios ElmasKazma (303) opened their account with Microsoft and has no Discord.
import type {
  IdentityFailure,
  IdentityStep,
  NameResult,
  SocialApi,
  SocialMe,
  SocialNotice,
  SocialProvider,
  SocialSnapshot,
} from "./social-types";

export const IDENTITY_SCENARIOS = [
  "signedOutMicrosoft",
  "signedOutLinked",
  "signingInMicrosoft",
  "notLinked",
  "nameStep",
  "microsoftOnly",
  "linkedBoth",
  "linkedOne",
  "identityWaiting",
  "identityNotice",
  "limitedAccount",
];

interface Context {
  scenario: string;
  param: (key: string) => string | null;
  view: () => SocialSnapshot;
  commit: (change: (draft: SocialSnapshot) => SocialSnapshot) => void;
  tell: (notice: SocialNotice) => void;
}

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const now = () => Math.floor(Date.now() / 1000);
const BOTH: SocialProvider[] = ["discord", "microsoft"];
/** The scenarios where Microsoft signs in but opens no account yet. */
const LINKED_ONLY = ["signedOutLinked", "notLinked"];
const LINK_PAGE = "https://social.projects.gg/auth/link?t=preview";

/** `YYYY-MM-DD` of `days` ago, on this computer's calendar. */
function daysAgo(days: number): string {
  const at = new Date(Date.now() - days * 86_400_000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
}

/** A new account opened with Microsoft: no Discord, the service's placeholder name. */
export function microsoftMe(base: SocialMe, named?: string): SocialMe {
  return {
    ...base,
    id: "9123456789012345678",
    name: named ?? "Oyuncu4821",
    username: "oyuncu4821",
    avatar: "preset:fox",
    avatarUrl: undefined,
    discordAvatarUrl: "",
    discordId: null,
    identities: [{ provider: "microsoft", linkedOn: daysAgo(0), isNew: true }],
    signInVia: "microsoft",
    nameSource: named ? "own" : "unset",
    ownNames: true,
  };
}

/** The preview's account, opened with Discord (its own id is its Discord id). */
function discordMe(base: SocialMe, microsoft: boolean): SocialMe {
  return {
    ...base,
    identities: [
      { provider: "discord", linkedOn: "2026-10-16", isNew: false, hint: base.username },
      ...(microsoft ? [{ provider: "microsoft" as const, linkedOn: daysAgo(3), isNew: true }] : []),
    ],
    signInVia: "discord",
    nameSource: "discord",
    ownNames: true,
  };
}

/** ElmasKazma opened their account with Microsoft: no Discord to message. */
function withoutDiscord(view: SocialSnapshot): SocialSnapshot {
  return {
    ...view,
    friends: view.friends.map((friend) =>
      friend.id === "303" ? { ...friend, discordId: null, discordUrl: "" } : friend,
    ),
  };
}

/** The scenario's view, on mock-social.ts's own. */
export function identitySnapshot(view: SocialSnapshot, ctx: Pick<Context, "scenario">) {
  if (!IDENTITY_SCENARIOS.includes(ctx.scenario)) return view;
  const base = withoutDiscord({
    ...view,
    providers: BOTH,
    signUpWays: LINKED_ONLY.includes(ctx.scenario) ? ["discord"] : BOTH,
    providersKnown: true,
  });
  const me = base.me;
  switch (ctx.scenario) {
    case "signedOutMicrosoft":
    case "signedOutLinked":
    case "notLinked":
      return { ...base, auth: "signedOut" as const, me: undefined, loaded: false, friends: [] };
    case "signingInMicrosoft":
      return {
        ...base,
        auth: "signingIn" as const,
        signingInWith: "microsoft" as const,
        me: undefined,
        loaded: false,
        friends: [],
      };
    case "nameStep":
    case "microsoftOnly":
      return me ? { ...base, me: microsoftMe(me) } : base;
    case "limitedAccount":
      return me
        ? {
            ...base,
            me: {
              ...microsoftMe(me, "GeceBekcisi"),
              limited: {
                until: now() + 6 * 86_400,
                reason: "account" as const,
                items: [
                  { key: "crewFound" as const },
                  { key: "codeLookups" as const, perDay: 10 },
                  { key: "settingsBackup" as const },
                  { key: "chatLinks" as const },
                ],
              },
            },
          }
        : base;
    case "linkedBoth":
      return me ? { ...base, me: discordMe(me, true) } : base;
    case "identityWaiting":
      return me
        ? {
            ...base,
            me: discordMe(me, false),
            identityChange: { provider: "microsoft" as const, unlink: false, url: LINK_PAGE },
          }
        : base;
    case "identityNotice":
      return me
        ? {
            ...base,
            me: discordMe(me, true),
            notices: [
              {
                id: "n-identity",
                kind: "identityLinked" as const,
                at: now() - 600,
                provider: "microsoft" as const,
              },
            ],
          }
        : base;
    default:
      return me ? { ...base, me: discordMe(me, false) } : base;
  }
}

/** Live steps: the name step comes right after the preview's sign-in. */
export function startIdentity(ctx: Context): void {
  if (ctx.scenario === "nameStep") {
    setTimeout(() => ctx.tell({ kind: "signedIn", askName: true }), 400);
  }
}

/** Whether the preview's sign-in stops here (`notLinked`), or the account it opens. */
export function identitySignIn(
  ctx: Pick<Context, "scenario">,
  provider: SocialProvider,
): { refused: "notLinked" | null; microsoft: boolean } {
  if (provider !== "microsoft") return { refused: null, microsoft: false };
  return { refused: ctx.scenario === "notLinked" ? "notLinked" : null, microsoft: true };
}

const FAILURES: IdentityFailure[] = [
  "providerOff",
  "exists",
  "notFound",
  "last",
  "taken",
  "proofFailed",
  "unavailable",
  "expired",
  "tooMany",
  "offline",
  "failed",
];
const NAME_RESULTS: NameResult[] = [
  "invalid",
  "rejected",
  "tooMany",
  "providerOff",
  "offline",
  "failed",
];

type IdentityApi = Pick<
  SocialApi,
  "refreshProviders" | "linkIdentity" | "unlinkIdentity" | "cancelIdentityChange" | "setName"
>;

export function identityApi(ctx: Context): IdentityApi {
  // The asked failure comes once; the next try goes through.
  let failOnce = ctx.param("identityError") as IdentityFailure | null;
  let nameOnce = ctx.param("nameError") as NameResult | null;
  const takeFailure = (): IdentityFailure | null => {
    const reason = failOnce && FAILURES.includes(failOnce) ? failOnce : null;
    failOnce = null;
    return reason;
  };
  // A link finishes when "the browser" comes back.
  let pending: ReturnType<typeof setTimeout> | null = null;
  const settle = (provider: SocialProvider, linked: boolean) => {
    ctx.commit((v) => {
      const me = v.me;
      if (!me) return { ...v, identityChange: undefined };
      const ways = me.identities ?? [];
      const identities = linked
        ? [...ways, { provider, linkedOn: daysAgo(0), isNew: true }]
        : ways.filter((way) => way.provider !== provider);
      const discord = provider === "discord";
      return {
        ...v,
        identityChange: undefined,
        me: {
          ...me,
          identities,
          ...(discord && !linked
            ? { discordId: null, discordAvatarUrl: "", avatar: "preset:golden-dragon" }
            : {}),
          ...(discord && linked
            ? {
                discordId: "80351110224678912",
                discordAvatarUrl: "https://cdn.discordapp.com/embed/avatars/3.png",
              }
            : {}),
        },
      };
    });
    ctx.tell({ kind: "identityChanged", provider, linked });
  };
  const toBrowser = (provider: SocialProvider, unlink: boolean): IdentityStep => {
    ctx.commit((v) => ({ ...v, identityChange: { provider, unlink, url: LINK_PAGE } }));
    if (pending) clearTimeout(pending);
    pending = setTimeout(() => {
      pending = null;
      if (!ctx.view().identityChange) return;
      settle(provider, !unlink);
    }, 4200);
    return { kind: "browser", url: LINK_PAGE };
  };
  return {
    // The preview's service offers Microsoft only in these scenarios (it ships switched off).
    async refreshProviders() {
      await delay(80);
    },
    async linkIdentity(provider) {
      await delay(400);
      const reason = takeFailure();
      if (reason) return { kind: "refused", reason };
      return toBrowser(provider, false);
    },
    async unlinkIdentity(provider) {
      await delay(400);
      const reason = takeFailure();
      if (reason) return { kind: "refused", reason };
      if ((ctx.view().me?.identities ?? []).length < 2) return { kind: "refused", reason: "last" };
      if (ctx.param("unlink") === "browser") return toBrowser(provider, true);
      settle(provider, false);
      return { kind: "done" };
    },
    async cancelIdentityChange() {
      if (pending) clearTimeout(pending);
      pending = null;
      ctx.commit((v) => ({ ...v, identityChange: undefined }));
    },
    async setName(name) {
      await delay(450);
      const asked = nameOnce && NAME_RESULTS.includes(nameOnce) ? nameOnce : null;
      nameOnce = null;
      if (asked) return asked;
      ctx.commit((v) => (v.me ? { ...v, me: { ...v.me, name, nameSource: "own" } } : v));
      return "saved";
    },
  };
}
