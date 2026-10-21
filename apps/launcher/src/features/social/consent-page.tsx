// The opt-in screen (S3): what is shared, in two sentences (a third about chat while chat may
// show), a link to the privacy page, and Discord sign-in. Pressing the button after reading is the
// consent. While the browser is open the button gives way to a calm waiting card; an expired or
// failed sign-in says so in place, and so do a suspended or banned account (with the community
// rules) and a day without new accounts (MOD1, SPAM1).
import { ArrowSquareOut, Prohibit, WarningCircle } from "@phosphor-icons/react";
import { Button, useAmbientMotion } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { presetAvatar } from "@/features/avatars/presets";
import { api } from "@/lib/api";
import { currentLanguage } from "@/lib/i18n";
import { openWebLink } from "@/lib/links";
import { DiscordButton, DiscordTile } from "./discord-button";
import { COMMUNITY_RULES_URL, dayMonth } from "./limits";
import { rise, useSocialMotion } from "./motion";
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

export function ConsentPage() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  // The Discord tile breathes while somebody uses this window. The player confirms in the
  // browser meanwhile, so the window rests and the tile settles: an endless pulse kept a covered
  // window drawing every frame (OPT2).
  const breathe = useAmbientMotion();
  const auth = useSocialUi((s) => s.view.auth);
  const signInUrl = useSocialUi((s) => s.signInUrl);
  const error = useSocialUi((s) => s.signInError);
  const setSignIn = useSocialUi((s) => s.setSignIn);
  const setConsent = useSocialPrefs((s) => s.setConsent);
  const chatAllowed = useSocialUi((s) => s.view.chatAllowed);
  const restriction = useSocialUi((s) => s.view.restriction);
  const [starting, setStarting] = useState(false);
  const waiting = auth === "signingIn";
  // A suspension that ended (or a ban lifted) no longer stands in the way.
  const restricted =
    restriction && (restriction.until === undefined || restriction.until * 1000 > Date.now())
      ? restriction
      : null;
  const errorText =
    error === "restricted"
      ? null
      : error === "signupsClosed"
        ? t("safety.signupsClosed")
        : error
          ? t(`social.consent.${error}`)
          : null;

  const start = async () => {
    setStarting(true);
    setConsent();
    setSignIn(null, null);
    try {
      setSignIn(await api.social.signIn(), null);
    } catch (error) {
      // Said in place, under the button: the service did not answer, or something else failed.
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
        {t("social.consent.body")}
        {chatAllowed ? ` ${t("chat.consent")}` : null}
      </p>
      <button
        type="button"
        onClick={() => void api.openUrl(PRIVACY_URL[currentLanguage()])}
        className="mt-2 inline-flex cursor-pointer items-center gap-1 rounded-md px-1.5 py-1 text-[13px] font-medium text-gold-300 underline-offset-4 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
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
                <DiscordTile size={44} />
              </motion.span>
              <p className="mt-4 text-[14.5px] font-semibold text-bone-50">
                {t("social.consent.waiting")}
              </p>
              <p className="mt-1 text-[13px] text-ash-400">{t("social.consent.waitingNote")}</p>
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
                      className="inline-flex cursor-pointer items-center gap-1 rounded-md px-1.5 py-0.5 text-[13px] font-medium text-gold-300 underline-offset-4 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
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
              <DiscordButton
                size="lg"
                className="w-full"
                disabled={starting}
                onClick={() => void start()}
              >
                {t("social.consent.signIn")}
              </DiscordButton>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}
