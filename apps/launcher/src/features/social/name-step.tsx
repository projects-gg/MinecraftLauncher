// SOC-ID: a new account opened with Microsoft starts with a placeholder name ("Oyuncu" and four
// digits), so right after its sign-in the panel asks once: "Arkadaşların seni hangi adla görsün?",
// the field filled with the active Minecraft account's name when it passes the rules. "Şimdi değil"
// keeps the placeholder; Arkadaşlar → Ayarlar → "Profil adı" changes it later.
import { Button, Field, Input } from "@pl/ui";
import { type FormEvent, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce, useAccounts } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { NameResult } from "@/lib/api/social-types";
import { cleanName, suggestedName } from "./identity";
import { useIdentityUi } from "./identity-store";
import { SocialAvatar } from "./social-face";
import { useSocialUi } from "./store";

/** The sentence of a name the service did not take (null: saved). */
export function nameError(result: NameResult): string | null {
  return result === "saved" ? null : `identity.name.${result}`;
}

export function NameStep() {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me);
  const setNameStep = useIdentityUi((s) => s.setNameStep);
  const { data: accounts } = useAccounts();
  const [value, setValue] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const id = useId();
  // The step ends with the panel, too: it asks once, right after the sign-in.
  useEffect(() => () => setNameStep(false), [setNameStep]);
  const shown = value ?? suggestedName(accounts);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const name = cleanName(shown);
    if (!name) {
      setError("identity.name.invalid");
      return;
    }
    setBusy(true);
    const result = await api.social.setName(name).catch((): NameResult => "failed");
    setBusy(false);
    const failure = nameError(result);
    if (failure) {
      setError(failure);
      return;
    }
    announce(t("identity.name.saved"));
    setNameStep(false);
  };

  return (
    <form
      onSubmit={(event) => void submit(event)}
      className="flex flex-col items-center px-2 pt-[9vh] pb-6 text-center"
    >
      {me ? (
        <SocialAvatar
          source={me}
          size={64}
          surface="var(--color-obsidian-900)"
          className="shadow-[0_12px_32px_rgb(34_6_6/0.6)]"
        />
      ) : null}
      <h2 className="mt-7 max-w-[24ch] font-display text-[22px] leading-tight font-semibold text-bone-50 [text-wrap:balance]">
        {t("identity.name.title")}
      </h2>
      <div className="mt-6 w-full max-w-[320px] text-left">
        <Field
          label={t("identity.name.label")}
          htmlFor={id}
          help={t("identity.name.rules")}
          error={error ? t(error) : null}
        >
          <Input
            id={id}
            value={shown}
            maxLength={16}
            autoComplete="off"
            spellCheck={false}
            // biome-ignore lint/a11y/noAutofocus: the step is this field; the panel opened on it.
            autoFocus
            invalid={Boolean(error)}
            disabled={busy}
            onChange={(event) => {
              setValue(event.target.value);
              setError(null);
            }}
          />
        </Field>
        <div className="mt-5 flex flex-col gap-2">
          <Button type="submit" variant="primary" size="lg" className="w-full" disabled={busy}>
            {t("identity.name.save")}
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="lg"
            className="w-full"
            disabled={busy}
            onClick={() => setNameStep(false)}
          >
            {t("identity.name.close")}
          </Button>
        </div>
      </div>
    </form>
  );
}
