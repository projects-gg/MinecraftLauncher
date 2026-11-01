// The opt-in screen (S3): what is shared, in two sentences (a third about chat while chat may
// show), a link to the privacy page, and the sign-in. Pressing a button after reading is the
// consent. SOC-ID: while Microsoft signs in only the accounts it is linked to, "Microsoft ile giriş
// yap" stands one size smaller under a thin "Bağlı hesabınla gir" divider; once it opens accounts
// too it stands under Discord's button as its equal and the body names both. The screen asks which
// ways the service offers each time it opens, and shows the last answer meanwhile. While the
// browser is open the buttons give way to a calm waiting card of that way (Microsoft's says where
// the password goes); an expired or failed sign-in says so in place, and so do a suspended or
// banned account (with the community rules), a day without new accounts (MOD1, SPAM1) and a
// Microsoft account that is no Projects account's yet.
import { ArrowSquareOut, LockSimple, Prohibit, WarningCircle } from "@phosphor-icons/react";
import { Button, useAmbientMotion } from "@pl/ui";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { presetAvatar } from "@/features/avatars/presets";
import { api } from "@/lib/api";
import type { SignInFailure, SocialProvider } from "@/lib/api/social-types";
import { currentLanguage } from "@/lib/i18n";
import { openWebLink } from "@/lib/links";
import { DiscordButton, DiscordTile } from "./discord-button";
import { useMicrosoftOffer } from "./identity-store";
import { COMMUNITY_RULES_URL, dayMonth } from "./limits";
import { rise, useSocialMotion } from "./motion";
import { MicrosoftButton, ProviderTile } from "./provider-mark";
import { SocialAvatar } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";

/** The privacy page's friends section, in the launcher's language (one page, both languages). */
const PRIVACY_URL = {
  tr: "https://projects.gg/gizlilik/#arkadaslar",
  en: "https://projects.gg/gizlilik/#friends",
} as const;

/** Three bundled faces with presence: the feature at a glance, no words needed. */
function Cluster() {
  const faces = [
    { slug: "fox", size: 48, state: "playing" as const, lift: 10 },
    { slug: "golden-dragon", size: 64, state: "launcher" as const, lift: 0 },
    { slug: "wizard", size: 48, state: "playing" as const, lift: 10, dnd: true },
  ];
  return (
    <div aria-hidden className="relative flex items-start justify-center gap-4">
      <span className="pointer-events-none absolute inset-x-2 -inset-y-8 bg-[radial-gradient(closest-side,rgb(232_154_44/0.2),transparent)]" />
      {faces.map((face) => {
        const preset = presetAvatar(`preset:${face.slug}`);
        return (
          <span key={face.slug} className="relative" style={{ marginTop: face.lift }}>
            <SocialAvatar
              source={{ name: preset?.name ?? face.slug, avatar: `preset:${face.slug}` }}
              size={face.size}
              state={face.state}
              dnd={face.dnd}
              surface="var(--color-obsidian-900)"
              className="shadow-[0_12px_32px_rgb(34_6_6/0.6)]"
            />
          </span>
        );
      })}
    </div>
  );
}

/** What a failed sign-in says in place (`restricted` has its own panel). */
function failureText(t: TFunction, error: SignInFailure | null): string | null {
  switch (error) {
    case null:
    case "restricted":
      return null;
    case "signupsClosed":
      return t("safety.signupsClosed");
    case "notLinked":
      return t("identity.consent.notLinked");
    case "providerOff":
      return t("identity.consent.providerOff");
    default:
      return t(`social.consent.${error}`);
  }
}

export function ConsentPage() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  // The tile breathes while somebody uses this window. The player confirms in the browser
  // meanwhile, so the window rests and the tile settles: an endless pulse kept a covered window
  // drawing every frame (OPT2).
  const breathe = useAmbientMotion();
  const auth = useSocialUi((s) => s.view.auth);
  const offer = useMicrosoftOffer();
  const signingInWith = useSocialUi((s) => s.view.signingInWith);
  const signInUrl = useSocialUi((s) => s.signInUrl);
  const error = useSocialUi((s) => s.signInError);
  const setSignIn = useSocialUi((s) => s.setSignIn);
  const setConsent = useSocialPrefs((s) => s.setConsent);
  const chatAllowed = useSocialUi((s) => s.view.chatAllowed);
  const restriction = useSocialUi((s) => s.view.restriction);
  const [starting, setStarting] = useState(false);
  const waiting = auth === "signingIn";
  const way: SocialProvider = signingInWith ?? "discord";
  // SOC-ID: which ways sign in now, asked whenever the screen opens (the backend asks at most
  // once a minute).
  useEffect(() => {
    void api.social.refreshProviders().catch(() => undefined);
  }, []);
  // A suspension that ended (or a ban lifted) no longer stands in the way.
  const restricted =
    restriction && (restriction.until === undefined || restriction.until * 1000 > Date.now())
      ? restriction
      : null;
  const errorText = failureText(t, error);

  const start = async (provider: SocialProvider) => {
    setStarting(true);
    setConsent();
    setSignIn(null, null);
    try {
      setSignIn(await api.social.signIn(provider), null);
    } catch (error) {
      // Said in place, under the buttons: the service did not answer, or something else failed.
      const code = error instanceof Error && "code" in error ? String(error.code) : "";
      setSignIn(null, code === "SOSYAL-YOK" ? "unavailable" : "failed");
    } finally {
      setStarting(false);
    }
  };

  const cancel = () => {
    setSignIn(null, null);
    void api.social.cancelSignIn();
  };

  return (
    <div className="flex flex-col items-center px-2 pt-[9vh] pb-6 text-center">
      <Cluster />
      <h2 className="mt-8 font-display text-[22px] leading-tight font-semibold text-bone-50 [text-wrap:balance]">
        {t("social.consent.title")}
      </h2>
      <p className="mt-3 max-w-[36ch] text-[14px] leading-relaxed text-ash-300">
        {offer === "open" ? t("identity.consent.body") : t("social.consent.body")}
        {chatAllowed ? ` ${t("chat.consent")}` : null}
      </p>
      <button
        type="button"
        onClick={() => void api.openUrl(PRIVACY_URL[currentLanguage()])}
        className="mt-2 inline-flex cursor-pointer items-center gap-1 rounded-md px-1.5 py-1 text-[13px] font-medium text-bone-100 underline decoration-white/30 underline-offset-[3px] outline-none transition-colors hover:text-bone-50 hover:decoration-current focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        {t("social.consent.privacy")}
        <ArrowSquareOut size={13} aria-hidden />
      </button>

      <div className="mt-8 w-full max-w-[320px]">
        <AnimatePresence mode="popLayout" initial={false}>
          {waiting ? (
            <motion.div
              key="waiting"
              role="status"
              className="flex flex-col items-center rounded-[var(--radius-panel)] bg-obsidian-950/40 px-5 py-5 ring-1 ring-white/8"
              {...rise(reduce)}
            >
              <motion.span
                animate={breathe ? { opacity: [1, 0.55, 1] } : { opacity: 1 }}
                transition={
                  breathe
                    ? { duration: 1.8, repeat: Number.POSITIVE_INFINITY, ease: "easeInOut" }
                    : { duration: 0.4, ease: "easeOut" }
                }
              >
                {way === "microsoft" ? (
                  <ProviderTile provider="microsoft" size={44} />
                ) : (
                  <DiscordTile size={44} />
                )}
              </motion.span>
              <p className="mt-4 text-[14.5px] font-semibold text-bone-50">
                {way === "microsoft"
                  ? t("identity.consent.waitingMicrosoft")
                  : t("social.consent.waiting")}
              </p>
              <p className="mt-1 text-[13px] text-ash-400">
                {way === "microsoft"
                  ? t("identity.consent.waitingMicrosoftNote")
                  : t("social.consent.waitingNote")}
              </p>
              {way === "microsoft" ? (
                <p className="mt-3 flex items-start gap-1.5 text-left text-[12.5px] text-ash-300">
                  <LockSimple size={14} className="mt-px shrink-0 text-ash-400" aria-hidden />
                  {t("identity.consent.password")}
                </p>
              ) : null}
              <div className="mt-4 flex gap-2">
                {signInUrl ? (
                  <Button size="sm" variant="secondary" onClick={() => void api.openUrl(signInUrl)}>
                    {t("social.consent.reopen")}
                  </Button>
                ) : null}
                <Button size="sm" variant="ghost" onClick={cancel}>
                  {t("social.consent.cancel")}
                </Button>
              </div>
            </motion.div>
          ) : (
            <motion.div key="start" {...rise(reduce)}>
              <AnimatePresence initial={false}>
                {restricted ? (
                  <motion.div
                    key="restricted"
                    role="alert"
                    className="mb-4 flex flex-col items-center gap-2 rounded-[var(--radius-panel)] bg-obsidian-950/45 px-4 py-3.5 ring-1 ring-flare-400/25"
                    {...rise(reduce, 4)}
                  >
                    <p className="flex items-start gap-2 text-left text-[13.5px] text-bone-100">
                      <Prohibit size={17} className="mt-px shrink-0 text-flare-400" aria-hidden />
                      {restricted.until !== undefined
                        ? t("safety.restricted.until", { date: dayMonth(restricted.until) })
                        : t("safety.restricted.forever")}
                    </p>
                    <button
                      type="button"
                      onClick={() => openWebLink(COMMUNITY_RULES_URL)}
                      className="inline-flex cursor-pointer items-center gap-1 rounded-md px-1.5 py-0.5 text-[13px] font-medium text-bone-100 underline decoration-white/30 underline-offset-[3px] outline-none transition-colors hover:text-bone-50 hover:decoration-current focus-visible:ring-2 focus-visible:ring-gold-300"
                    >
                      {t("safety.restricted.rules")}
                      <ArrowSquareOut size={13} aria-hidden />
                    </button>
                  </motion.div>
                ) : errorText ? (
                  <motion.p
                    key={error ?? "error"}
                    role="alert"
                    className="mb-3 flex items-start justify-center gap-2 text-left text-[13px] text-flare-400"
                    {...rise(reduce, 4)}
                  >
                    <WarningCircle size={16} weight="fill" className="mt-px shrink-0" aria-hidden />
                    {errorText}
                  </motion.p>
                ) : null}
              </AnimatePresence>
              <div className="flex flex-col gap-2.5">
                <DiscordButton
                  size="lg"
                  className="w-full transition-[transform,scale,opacity] duration-150 ease-out active:translate-y-0 active:scale-[0.97]"
                  disabled={starting}
                  onClick={() => void start("discord")}
                >
                  {t("social.consent.signIn")}
                </DiscordButton>
                {offer === "open" ? (
                  <MicrosoftButton
                    className="w-full"
                    disabled={starting}
                    onClick={() => void start("microsoft")}
                  >
                    {t("identity.consent.microsoft")}
                  </MicrosoftButton>
                ) : null}
              </div>
              {/* The first answer of the service may bring it in while the screen is open: it
                  rises in below everything, nothing above it moves. */}
              <AnimatePresence initial={false}>
                {offer === "linked" ? (
                  <motion.div key="linked" className="mt-6" {...rise(reduce, 4)}>
                    <p className="flex items-center gap-3 text-[12.5px] text-ash-300">
                      <span aria-hidden className="h-px flex-1 bg-white/10" />
                      {t("identity.consent.linkedWay")}
                      <span aria-hidden className="h-px flex-1 bg-white/10" />
                    </p>
                    <MicrosoftButton
                      size="md"
                      className="mt-3 w-full"
                      disabled={starting}
                      onClick={() => void start("microsoft")}
                    >
                      {t("identity.consent.microsoft")}
                    </MicrosoftButton>
                  </motion.div>
                ) : null}
              </AnimatePresence>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}
