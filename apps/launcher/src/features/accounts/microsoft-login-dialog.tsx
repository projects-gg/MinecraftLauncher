// Microsoft sign-in: starts as soon as it opens (the player already pressed "Microsoft ile giriş
// yap"), follows the browser steps, then says plainly what happened.
import { Check, CircleNotch, Copy } from "@phosphor-icons/react";
import { Button, ContentIcon, cn, easeOutExpo, PlayerHead, springSnappy } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { MicrosoftMark } from "@/features/settings/microsoft-mark";
import { type AccountSummary, api, type LoginStep, type MicrosoftLoginResult } from "@/lib/api";
import { copyDetails } from "@/lib/clipboard";
import { playSound } from "@/lib/sound";
import { AccountDialog } from "./account-dialog";
import { useCalm } from "./calm";
import { useAccountsUi } from "./store";
import { ACCOUNTS_KEY, type FriendlyError, friendlyError } from "./use-accounts";

const ORDER: LoginStep[] = [
  "openingBrowser",
  "waitingForBrowser",
  "microsoft",
  "xboxLive",
  "xsts",
  "minecraft",
  "profile",
  "done",
];
const VISIBLE: LoginStep[] = ["waitingForBrowser", "xboxLive", "minecraft", "profile"];

export type LoginFailure = "timeout" | "noLicense" | "noProfile" | "network" | "other";

/** Which explanation a failed sign-in gets, from its support code. */
export function classifyLoginError(code: string): LoginFailure {
  if (code === "MSA-ZAMANASIMI") return "timeout";
  if (code === "MC-LISANSYOK") return "noLicense";
  if (code === "MC-PROFILYOK") return "noProfile";
  if (code === "AG-BAGLANTI" || /-5\d\d$/.test(code)) return "network";
  return "other";
}

type Phase =
  | { kind: "working"; step: LoginStep }
  | { kind: "success"; result: MicrosoftLoginResult }
  | { kind: "error"; failure: LoginFailure; error: FriendlyError };

function Steps({ step }: { step: LoginStep }) {
  const { t } = useTranslation();
  const calm = useCalm();
  const current = ORDER.indexOf(step);
  // The first visible step that is not finished yet is the one in progress.
  const active = VISIBLE.find((s) => ORDER.indexOf(s) >= current) ?? null;
  return (
    <ol className="space-y-3" aria-live="polite">
      {VISIBLE.map((s) => {
        const done = ORDER.indexOf(s) < current;
        const now = s === active;
        return (
          <li key={s} className="flex items-center gap-3">
            <span
              className={cn(
                "flex size-7 items-center justify-center rounded-full ring-1 transition-colors duration-300",
                done
                  ? "bg-jade-400/15 text-jade-400 ring-jade-400/40"
                  : now
                    ? "text-gold-300 ring-gold-400/60"
                    : "text-ash-400 ring-white/10",
              )}
            >
              <AnimatePresence mode="wait" initial={false}>
                {done ? (
                  <motion.span
                    key="done"
                    className="flex"
                    initial={calm({ scale: 0.5, opacity: 0 })}
                    animate={calm({ scale: 1, opacity: 1 })}
                    transition={springSnappy}
                  >
                    <Check size={14} weight="bold" />
                  </motion.span>
                ) : now ? (
                  <motion.span
                    key="now"
                    className="flex animate-spin"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                  >
                    <CircleNotch size={15} weight="bold" />
                  </motion.span>
                ) : (
                  <span className="size-1.5 rounded-full bg-current" />
                )}
              </AnimatePresence>
            </span>
            <span
              className={cn(
                "text-[14px] transition-colors duration-300",
                done ? "text-ash-300" : now ? "font-medium text-bone-50" : "text-ash-400",
              )}
            >
              {t(`accounts.login.steps.${s}`)}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

function Success({ result }: { result: MicrosoftLoginResult }) {
  const { t } = useTranslation();
  const calm = useCalm();
  const name = result.account.username;
  // A second line only where the outcome is not what the player may have expected.
  const copy: Record<MicrosoftLoginResult["outcome"], [string, string | null]> = {
    added: [t("accounts.login.success.added", { name }), null],
    alreadyAdded: [
      t("accounts.login.success.alreadyAdded", { name }),
      t("accounts.login.success.alreadyAddedBody"),
    ],
    relogged: [t("accounts.login.success.relogged"), null],
    otherAccount: [
      t("accounts.login.success.otherAccount"),
      t("accounts.login.success.otherAccountBody", { name }),
    ],
  };
  const [title, body] = copy[result.outcome];
  return (
    <div className="flex flex-col items-center text-center" role="status">
      <span className="relative">
        <motion.span
          className="flex"
          initial={calm({ scale: 0.86, opacity: 0 })}
          animate={calm({ scale: 1, opacity: 1 })}
          transition={springSnappy}
        >
          <PlayerHead name={name} src={result.account.headUrl} size={64} />
        </motion.span>
        <motion.span
          className="absolute -right-2 -bottom-2 flex size-7 items-center justify-center rounded-full bg-jade-400 text-obsidian-950 ring-4 ring-obsidian-900"
          initial={calm({ scale: 0.4, opacity: 0 })}
          animate={calm({ scale: 1, opacity: 1 })}
          transition={{ ...springSnappy, delay: 0.12 }}
        >
          <Check size={15} weight="bold" />
        </motion.span>
      </span>
      <p className="mt-5 font-display text-[19px] font-semibold text-bone-50">{title}</p>
      {body ? (
        <p className="mt-1.5 max-w-[38ch] text-[13.5px] leading-relaxed text-ash-300">{body}</p>
      ) : null}
    </div>
  );
}

function Failure({ failure, error }: { failure: LoginFailure; error: FriendlyError }) {
  const { t } = useTranslation();
  const calm = useCalm();
  const title = t(`accounts.login.errors.${failure}Title`);
  const body = failure === "other" ? error.message : t(`accounts.login.errors.${failure}Body`);
  return (
    <div className="flex flex-col items-center text-center" role="alert">
      <motion.span
        className="flex size-14 items-center justify-center rounded-full bg-flare-400/12 text-flare-400 ring-1 ring-flare-400/30"
        initial={calm({ scale: 0.8, opacity: 0 })}
        animate={calm({ scale: 1, opacity: 1 })}
        transition={springSnappy}
      >
        <ContentIcon kind="failed" />
      </motion.span>
      <p className="mt-5 font-display text-[19px] font-semibold text-bone-50">{title}</p>
      <p className="mt-1.5 max-w-[40ch] text-[13.5px] leading-relaxed text-ash-300">{body}</p>
    </div>
  );
}

/**
 * The Microsoft sign-in dialog. Without `relogin` it adds an account (or renews one that is
 * already saved, never adding a duplicate); with `relogin` it renews that account's session.
 */
export function MicrosoftLoginDialog({
  open,
  onOpenChange,
  relogin = null,
  onUseOffline,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  relogin?: AccountSummary | null;
  /** Offered when the account has no Minecraft: switch to adding an offline account. */
  onUseOffline?: () => void;
}) {
  const { t } = useTranslation();
  const calm = useCalm();
  const queryClient = useQueryClient();
  const markFresh = useAccountsUi((s) => s.markFresh);
  const [phase, setPhase] = useState<Phase>({ kind: "working", step: "openingBrowser" });
  const run = useRef(0);
  const reloginId = relogin?.id;

  const start = useCallback(() => {
    const id = ++run.current;
    setPhase({ kind: "working", step: "openingBrowser" });
    api
      .loginMicrosoft(
        (step) => {
          if (run.current === id) setPhase({ kind: "working", step });
        },
        reloginId ? { relogin: reloginId } : undefined,
      )
      .then((result) => {
        if (run.current !== id) return;
        void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
        markFresh(result.account.id);
        setPhase({ kind: "success", result });
        playSound("success");
      })
      .catch((error: unknown) => {
        if (run.current !== id) return;
        const friendly = friendlyError(error, t("accounts.login.errors.otherTitle"));
        if (friendly.code === "IPTAL") return;
        setPhase({ kind: "error", failure: classifyLoginError(friendly.code), error: friendly });
        playSound("error");
      });
  }, [queryClient, markFresh, reloginId, t]);

  // Opening the dialog is the request to sign in (once per opening).
  const wasOpen = useRef(false);
  useEffect(() => {
    if (open && !wasOpen.current) start();
    if (!open) run.current++;
    wasOpen.current = open;
  }, [open, start]);

  // Successes close by themselves, except the surprising one.
  useEffect(() => {
    if (phase.kind !== "success" || phase.result.outcome === "otherAccount") return;
    const timer = setTimeout(() => onOpenChange(false), 1700);
    return () => clearTimeout(timer);
  }, [phase, onOpenChange]);

  const close = (next: boolean) => {
    if (next) return onOpenChange(true);
    if (phase.kind === "working") void api.cancelMicrosoftLogin();
    run.current++;
    onOpenChange(false);
  };

  // Focus follows the phase to its main action, so Enter and Escape act at once (the close
  // button would open its tooltip and swallow the first Escape).
  const action = useRef<HTMLButtonElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: each phase brings its own main action
  useEffect(() => {
    if (open) action.current?.focus();
  }, [open, phase.kind]);

  const footer =
    phase.kind === "working" ? (
      <Button ref={action} variant="ghost" className="h-11" onClick={() => close(false)}>
        {t("accounts.login.cancel")}
      </Button>
    ) : phase.kind === "success" ? (
      <Button ref={action} variant="primary" className="h-11" onClick={() => close(false)}>
        {t("accounts.login.done")}
      </Button>
    ) : (
      <>
        {phase.failure === "other" || phase.failure === "network" ? (
          <Button
            variant="ghost"
            className="mr-auto h-11"
            icon={<Copy size={15} />}
            onClick={() => void copyDetails(phase.error.details ?? phase.error.code)}
          >
            {t("common.copyDetails")}
          </Button>
        ) : null}
        <Button variant="ghost" className="h-11" onClick={() => close(false)}>
          {t("accounts.login.close")}
        </Button>
        {phase.failure === "noLicense" && onUseOffline ? (
          <Button
            variant="secondary"
            className="h-11"
            onClick={() => {
              close(false);
              onUseOffline();
            }}
          >
            {t("accounts.offline")}
          </Button>
        ) : null}
        <Button
          ref={action}
          variant="primary"
          className="h-11"
          icon={<MicrosoftMark size={15} />}
          onClick={start}
        >
          {t("accounts.login.retry")}
        </Button>
      </>
    );

  return (
    <AccountDialog
      open={open}
      onOpenChange={close}
      initialFocus={action}
      title={relogin ? t("accounts.login.reloginTitle") : t("accounts.login.title")}
      // Only a renewal needs a word: it must be the same Microsoft account.
      description={
        relogin ? t("accounts.login.reloginDescription", { name: relogin.username }) : undefined
      }
      closeLabel={t("common.close")}
      width={480}
      footer={footer}
    >
      <MotionConfig reducedMotion="user">
        <div className="flex min-h-[196px] flex-col justify-center py-2">
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={phase.kind}
              initial={calm({ opacity: 0, y: 6 })}
              animate={calm({
                opacity: 1,
                y: 0,
                transition: { duration: 0.22, ease: easeOutExpo },
              })}
              exit={calm({ opacity: 0, y: -4, transition: { duration: 0.14 } })}
            >
              {phase.kind === "working" ? (
                <Steps step={phase.step} />
              ) : phase.kind === "success" ? (
                <Success result={phase.result} />
              ) : (
                <Failure failure={phase.failure} error={phase.error} />
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      </MotionConfig>
    </AccountDialog>
  );
}
