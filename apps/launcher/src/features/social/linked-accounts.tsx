// SOC-ID in Arkadaşlar → Ayarlar: "Profil adı" (the name friends see, changed in place) and "Bağlı
// hesaplar" right above "Oturumlar": each way into the account with its mark, the Discord handle
// beside Discord's name, when it was linked ("25 Ekim'de bağlandı") under it; "Kaldır" on all but
// the last one (asked first; a Discord removal says what goes with it), "Discord hesabını bağla" or
// "Microsoft hesabını bağla" for a way the service offers that is not linked. A link, or a removal
// that needs a proof, waits in a calm card while the browser is open; what went wrong is said under
// the rows. No "Yeni" or "Bu cihaz" chips (projects-taste): the date says when, and "Oturumlar"
// says which way each session came in by.
import { LockSimple, WarningCircle } from "@phosphor-icons/react";
import { Button, Dialog, Field, Input, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  type FormEvent,
  type ReactNode,
  type Ref,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { MicrosoftMark } from "@/features/settings/microsoft-mark";
import { api } from "@/lib/api";
import type {
  IdentityStep,
  NameResult,
  SocialIdentity,
  SocialMe,
  SocialProvider,
} from "@/lib/api/social-types";
import { currentLanguage } from "@/lib/i18n";
import { DiscordMark } from "./discord-button";
import {
  cleanName,
  discordIsAccount,
  failureKey,
  identitiesOf,
  linkableOf,
  linkedDate,
  proofProvider,
} from "./identity";
import { useIdentityUi, useMicrosoftOffer } from "./identity-store";
import { rise, useSocialMotion } from "./motion";
import { nameError } from "./name-step";
import { ProviderTile } from "./provider-mark";
import { useSocialUi } from "./store";

function Section({
  title,
  children,
  sectionRef,
}: {
  title: string;
  children: ReactNode;
  sectionRef?: Ref<HTMLElement>;
}) {
  return (
    <section ref={sectionRef} className="border-b border-white/7 py-5 first:pt-2 last:border-b-0">
      <h3 className="mb-3 font-display text-[15px] font-semibold text-bone-50">{title}</h3>
      {children}
    </section>
  );
}

/** "Profil adı": the name friends see, changed in place (three changes a day). */
export function ProfileNameSection({ me }: { me: SocialMe }) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(me.name);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const id = useId();
  const { reduce } = useSocialMotion();

  const save = async (event: FormEvent) => {
    event.preventDefault();
    const name = cleanName(value);
    if (!name) {
      setError("identity.name.invalid");
      return;
    }
    if (name === me.name) {
      setEditing(false);
      return;
    }
    setBusy(true);
    const result = await api.social.setName(name).catch((): NameResult => "failed");
    setBusy(false);
    if (result === "providerOff") {
      // The section goes with the service's new answer: said once, outside it.
      setEditing(false);
      toast(t("identity.name.providerOff"));
      return;
    }
    const failure = nameError(result);
    if (failure) {
      setError(failure);
      return;
    }
    announce(t("identity.name.saved"));
    setEditing(false);
  };

  return (
    <Section title={t("identity.settings.name")}>
      <AnimatePresence mode="popLayout" initial={false}>
        {editing ? (
          <motion.form key="edit" onSubmit={(event) => void save(event)} {...rise(reduce, 4)}>
            <Field
              label={<span className="sr-only">{t("identity.name.label")}</span>}
              htmlFor={id}
              help={t("identity.name.rules")}
              error={error ? t(error) : null}
            >
              <Input
                id={id}
                value={value}
                maxLength={16}
                autoComplete="off"
                spellCheck={false}
                // The player just pressed "Değiştir" to type a name.
                autoFocus
                invalid={Boolean(error)}
                disabled={busy}
                onChange={(event) => {
                  setValue(event.target.value);
                  setError(null);
                }}
              />
            </Field>
            <div className="mt-3 flex gap-2">
              <Button type="submit" size="sm" variant="secondary" disabled={busy}>
                {t("identity.settings.save")}
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={busy}
                onClick={() => {
                  setEditing(false);
                  setError(null);
                }}
              >
                {t("identity.settings.cancel")}
              </Button>
            </div>
          </motion.form>
        ) : (
          <motion.div key="show" className="flex items-center gap-3" {...rise(reduce, 4)}>
            <span className="min-w-0 flex-1 truncate text-[14px] font-medium text-bone-50">
              {me.name}
            </span>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setValue(me.name);
                setError(null);
                setEditing(true);
              }}
            >
              {t("identity.settings.change")}
            </Button>
          </motion.div>
        )}
      </AnimatePresence>
    </Section>
  );
}

/** What the browser shows while a link or a removal waits: the way whose page it is. */
function WaitingCard({
  change,
}: {
  change: { provider: SocialProvider; unlink: boolean; url: string };
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const proof = proofProvider(change);
  return (
    <motion.div
      role="status"
      className="mt-3 flex flex-col items-center rounded-[var(--radius-panel)] bg-obsidian-950/40 px-5 py-4 text-center ring-1 ring-white/8"
      {...rise(reduce)}
    >
      <ProviderTile provider={proof} size={40} />
      <p className="mt-3 text-[14px] font-semibold text-bone-50">
        {t(`identity.waiting.${proof}`)}
      </p>
      <p className="mt-1 text-[12.5px] text-ash-400">{t(`identity.waiting.${proof}Note`)}</p>
      {proof === "microsoft" ? (
        <p className="mt-2.5 flex items-start gap-1.5 text-left text-[12.5px] text-ash-300">
          <LockSimple size={14} className="mt-px shrink-0 text-ash-400" aria-hidden />
          {t("identity.waiting.password")}
        </p>
      ) : null}
      <div className="mt-3.5 flex gap-2">
        <Button size="sm" variant="secondary" onClick={() => void api.openUrl(change.url)}>
          {t("identity.waiting.reopen")}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => void api.social.cancelIdentityChange()}>
          {t("identity.waiting.cancel")}
        </Button>
      </div>
    </motion.div>
  );
}

function IdentityRow({
  way,
  removable,
  onRemove,
}: {
  way: SocialIdentity;
  removable: boolean;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const date = linkedDate(way.linkedOn, currentLanguage());
  const handle = way.provider === "discord" && way.hint ? way.hint : null;
  return (
    <div className="flex min-h-14 items-center gap-3 py-2">
      <ProviderTile provider={way.provider} />
      <span className="min-w-0 flex-1">
        <span className="flex min-w-0 items-baseline gap-2">
          <span className="shrink-0 text-[14px] font-medium text-bone-50">
            {t(`identity.providers.${way.provider}`)}
          </span>
          {handle ? (
            <span className="min-w-0 truncate text-[13px] text-ash-400" data-selectable>
              @{handle}
            </span>
          ) : null}
        </span>
        {date ? (
          <span className="block truncate text-[12.5px] text-ash-400">
            {t("identity.settings.linkedOn", { date })}
          </span>
        ) : null}
      </span>
      {removable ? (
        <Button size="sm" variant="ghost" onClick={onRemove}>
          {t("identity.settings.remove")}
        </Button>
      ) : null}
    </div>
  );
}

/** "Bağlı hesaplar": the ways into the account, right above "Oturumlar". */
export function LinkedAccountsSection({ me }: { me: SocialMe }) {
  const { t } = useTranslation();
  const offer = useMicrosoftOffer();
  const change = useSocialUi((s) => s.view.identityChange);
  const failure = useIdentityUi((s) => s.failure);
  const setFailure = useIdentityUi((s) => s.setFailure);
  const cue = useIdentityUi((s) => s.linkedCue);
  const { reduce } = useSocialMotion();
  // The removal asked about; `provider` stays while the dialog closes (its title fades with it).
  const [removal, setRemoval] = useState<{ open: boolean; provider: SocialProvider }>({
    open: false,
    provider: "microsoft",
  });
  const [busy, setBusy] = useState(false);
  const section = useRef<HTMLElement>(null);
  const ways = identitiesOf(me);
  // Discord is always offered; Microsoft while the service signs in with it.
  const linkable = linkableOf(me, offer === "none" ? ["discord"] : ["discord", "microsoft"]);

  // The panel asks which ways sign in now (at most once a minute).
  useEffect(() => {
    void api.social.refreshProviders().catch(() => undefined);
  }, []);
  // "Bağlı hesapları aç" (a note's action): the section comes on screen.
  useEffect(() => {
    if (!cue) return;
    section.current?.scrollIntoView({ block: "start", behavior: reduce ? "auto" : "smooth" });
  }, [cue, reduce]);
  // Leaving the settings forgets what went wrong, also what an answer still on its way would say
  // (after a sign-out it would show to the next account).
  const shown = useRef(true);
  useEffect(() => {
    shown.current = true;
    return () => {
      shown.current = false;
      setFailure(null);
    };
  }, [setFailure]);
  const closeRemoval = () => setRemoval((now) => ({ ...now, open: false }));

  const step = async (run: () => Promise<IdentityStep>, provider: SocialProvider) => {
    setBusy(true);
    setFailure(null);
    const answer = await run().catch((): IdentityStep => ({ kind: "refused", reason: "failed" }));
    setBusy(false);
    if (answer.kind === "refused" && shown.current) {
      setFailure({ provider, reason: answer.reason });
    }
  };

  return (
    <Section title={t("identity.settings.linked")} sectionRef={section}>
      <div className="divide-y divide-white/6">
        {ways.map((way) => (
          <IdentityRow
            key={way.provider}
            way={way}
            removable={ways.length > 1 && !change}
            onRemove={() => setRemoval({ open: true, provider: way.provider })}
          />
        ))}
      </div>
      <AnimatePresence mode="popLayout" initial={false}>
        {change ? (
          <WaitingCard key="waiting" change={change} />
        ) : linkable.length ? (
          <motion.div key="link" className="mt-3 flex flex-wrap gap-2" {...rise(reduce, 4)}>
            {linkable.map((provider) => (
              <Button
                key={provider}
                size="sm"
                variant="secondary"
                disabled={busy}
                icon={
                  provider === "microsoft" ? (
                    <MicrosoftMark size={13} />
                  ) : (
                    <DiscordMark height={11} />
                  )
                }
                onClick={() => void step(() => api.social.linkIdentity(provider), provider)}
              >
                {t(`identity.settings.link.${provider}`)}
              </Button>
            ))}
          </motion.div>
        ) : null}
      </AnimatePresence>
      <AnimatePresence initial={false}>
        {failure ? (
          <motion.p
            key={`${failure.provider}-${failure.reason}`}
            role="alert"
            className="mt-3 flex items-start gap-2 text-[13px] text-flare-400"
            {...rise(reduce, 4)}
          >
            <WarningCircle size={16} weight="fill" className="mt-px shrink-0" aria-hidden />
            {t(failureKey(failure.reason), {
              provider: t(`identity.providers.${failure.provider}`),
            })}
          </motion.p>
        ) : null}
      </AnimatePresence>
      <Dialog
        open={removal.open}
        onOpenChange={(open) => !busy && !open && closeRemoval()}
        title={t("identity.remove.title", {
          provider: t(`identity.providers.${removal.provider}`),
        })}
        closeLabel={t("common.close")}
        width={420}
        dismissible={!busy}
        footer={
          <>
            <Button variant="ghost" disabled={busy} onClick={closeRemoval}>
              {t("identity.remove.cancel")}
            </Button>
            <Button
              variant="danger"
              disabled={busy}
              onClick={() => {
                const provider = removal.provider;
                void step(() => api.social.unlinkIdentity(provider), provider).then(closeRemoval);
              }}
            >
              {t("identity.remove.confirm")}
            </Button>
          </>
        }
      >
        {removal.provider === "discord" ? (
          <p className="text-[14px] leading-relaxed text-bone-100">
            {t(discordIsAccount(me) ? "identity.remove.discordNumber" : "identity.remove.discord")}
          </p>
        ) : null}
      </Dialog>
    </Section>
  );
}
