// "Hesap ekle": the Microsoft sign-in, or an offline name checked as it is typed.
import { Check, CircleNotch, UserPlus } from "@phosphor-icons/react";
import { Button, cn, easeOutExpo, Input } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, type RefObject, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { MicrosoftMark } from "@/features/settings/microsoft-mark";
import { type AccountSummary, api } from "@/lib/api";
import { playSound } from "@/lib/sound";
import { useCalm } from "./calm";
import { checkOfflineName } from "./name-rules";
import { useAccountsUi } from "./store";
import { ACCOUNTS_KEY, announce, friendlyError, switchAccount } from "./use-accounts";

function OfflineForm({
  accounts,
  inputRef,
}: {
  accounts: readonly AccountSummary[];
  inputRef: RefObject<HTMLInputElement | null>;
}) {
  const { t } = useTranslation();
  const calm = useCalm();
  const queryClient = useQueryClient();
  const markFresh = useAccountsUi((s) => s.markFresh);
  const inputId = useId();
  const messageId = useId();
  const [value, setValue] = useState("");
  const [final, setFinal] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);
  const [state, setState] = useState<"idle" | "saving" | "added">("idle");
  const check = checkOfflineName(value, accounts, { final });
  const problem = serverError ?? (check.problem ? t(`accounts.name.${check.problem}`) : null);

  useEffect(() => {
    if (state !== "added") return;
    const timer = setTimeout(() => setState("idle"), 1600);
    return () => clearTimeout(timer);
  }, [state]);

  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    if (state === "saving") return;
    const strict = checkOfflineName(value, accounts, { final: true });
    setFinal(true);
    if (strict.problem) return;
    setState("saving");
    try {
      const account = await api.addOfflineAccount(strict.name);
      await queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
      markFresh(account.id);
      setValue("");
      setFinal(false);
      setState("added");
      playSound("success");
      announce(t("accounts.center.added", { name: account.username }));
      // The list grew above the field: once it has settled, bring the field back into view.
      setTimeout(
        () => inputRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" }),
        380,
      );
    } catch (error) {
      setState("idle");
      setServerError(friendlyError(error, t("accounts.name.taken")).message);
    }
  };

  const taken = check.takenBy ? accounts.find((a) => a.id === check.takenBy?.id) : undefined;

  return (
    <form onSubmit={(event) => void submit(event)} noValidate>
      <label htmlFor={inputId} className="text-[13px] font-medium text-bone-100">
        {t("accounts.center.offlineLabel")}
      </label>
      <div className="mt-1.5 flex gap-2">
        <Input
          ref={inputRef}
          data-add-offline
          id={inputId}
          value={value}
          invalid={Boolean(problem)}
          aria-describedby={messageId}
          autoComplete="off"
          spellCheck={false}
          className="h-11"
          onChange={(event) => {
            setValue(event.target.value);
            setServerError(null);
            // "Too short" waits until the player leaves the field or presses Enter.
            setFinal(false);
            if (state === "added") setState("idle");
          }}
          onBlur={() => {
            if (value.trim()) setFinal(true);
          }}
          // The field sits at the bottom of the sheet: bring its message line into view with it,
          // so a problem with the name (and its suggestion) is never typed below the edge.
          onFocus={() =>
            document
              .getElementById(messageId)
              ?.scrollIntoView({ block: "nearest", behavior: "smooth" })
          }
        />
        <Button
          type="submit"
          variant="secondary"
          className="h-11 min-w-[108px]"
          aria-disabled={state === "saving" || undefined}
          icon={
            <AnimatePresence mode="popLayout" initial={false}>
              <motion.span
                key={state}
                className="flex"
                initial={calm({ opacity: 0, scale: 0.6, filter: "blur(2px)" })}
                animate={calm({ opacity: 1, scale: 1, filter: "blur(0px)" })}
                exit={calm({ opacity: 0, scale: 0.6, filter: "blur(2px)" })}
                transition={{ duration: 0.2, ease: easeOutExpo }}
              >
                {state === "saving" ? (
                  <CircleNotch size={16} className="animate-spin" />
                ) : state === "added" ? (
                  <Check size={16} weight="bold" className="text-jade-400" />
                ) : (
                  <UserPlus size={16} />
                )}
              </motion.span>
            </AnimatePresence>
          }
        >
          {state === "added" ? t("accounts.center.addedOffline") : t("accounts.center.addOffline")}
        </Button>
      </div>
      {/* Only problems are spelled out; the line keeps its height so nothing jumps. */}
      <p
        id={messageId}
        aria-live="polite"
        className="mt-1.5 min-h-[20px] text-[12.5px] leading-snug text-flare-400"
      >
        {problem}
        {problem && check.suggestion ? (
          <>
            {" "}
            <button
              type="button"
              className="cursor-pointer font-semibold text-gold-300 underline-offset-2 hover:underline focus-visible:underline"
              onClick={() => {
                setValue(check.suggestion ?? value);
                setServerError(null);
                inputRef.current?.focus();
              }}
            >
              {t("accounts.center.useSuggestion", { name: check.suggestion })}
            </button>
          </>
        ) : null}
        {check.problem === "taken" && taken && !taken.active ? (
          <>
            {" "}
            <button
              type="button"
              className="cursor-pointer font-semibold text-gold-300 underline-offset-2 hover:underline focus-visible:underline"
              onClick={() => {
                void switchAccount(queryClient, taken);
                setValue("");
                setFinal(false);
              }}
            >
              {t("accounts.center.switchToTaken")}
            </button>
          </>
        ) : null}
      </p>
    </form>
  );
}

/** The add section. `prominent` (no accounts yet) makes the Microsoft sign-in the main action. */
export function AddAccount({
  accounts,
  prominent,
  inputRef,
}: {
  accounts: readonly AccountSummary[];
  prominent: boolean;
  inputRef: RefObject<HTMLInputElement | null>;
}) {
  const { t } = useTranslation();
  const openLogin = useAccountsUi((s) => s.openLogin);
  const headingId = useId();
  return (
    <section aria-labelledby={headingId}>
      {/* With no account yet the empty state above already says it; keep it for screen readers. */}
      <h3
        id={headingId}
        className={cn(
          "font-display text-[15px] font-semibold text-bone-50",
          prominent && "sr-only",
        )}
      >
        {t("accounts.center.addTitle")}
      </h3>
      <Button
        variant={prominent ? "primary" : "secondary"}
        size="lg"
        className="mt-3 w-full"
        icon={<MicrosoftMark size={16} />}
        onClick={() => openLogin(null)}
      >
        {t("settings.accounts.addMicrosoft")}
      </Button>
      <div className="my-4 flex items-center gap-3 text-[12.5px] text-ash-400">
        <span className="h-px flex-1 bg-white/10" />
        {t("accounts.center.or")}
        <span className="h-px flex-1 bg-white/10" />
      </div>
      <OfflineForm accounts={accounts} inputRef={inputRef} />
    </section>
  );
}
