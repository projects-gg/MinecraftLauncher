import { Check, CircleNotch, Trash, UserPlus } from "@phosphor-icons/react";
import { Button, Chip, cn, Dialog, Field, IconButton, Input, PlayerHead, toast } from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type LoginStep } from "@/lib/api";
import { MicrosoftMark } from "./microsoft-mark";

const OFFLINE_NAME = /^[A-Za-z0-9_]{3,16}$/;

const visibleSteps: LoginStep[] = ["waitingForBrowser", "xboxLive", "minecraft", "profile"];

function stepIndex(step: LoginStep | null): number {
  const order: LoginStep[] = [
    "openingBrowser",
    "waitingForBrowser",
    "microsoft",
    "xboxLive",
    "xsts",
    "minecraft",
    "profile",
    "done",
  ];
  return step ? order.indexOf(step) : -1;
}

function MicrosoftLoginDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [step, setStep] = useState<LoginStep | null>(null);

  const login = useMutation({
    mutationFn: () => api.loginMicrosoft(setStep),
    onSuccess: (account) => {
      void queryClient.invalidateQueries({ queryKey: ["accounts"] });
      toast.success(t("settings.accounts.added", { name: account.username }));
      setTimeout(() => onOpenChange(false), 700);
    },
    onError: () => {
      setStep(null);
    },
  });

  const start = () => {
    setStep("openingBrowser");
    login.mutate();
  };

  const close = (next: boolean) => {
    if (!next && login.isPending) void api.cancelMicrosoftLogin();
    if (!next) {
      setStep(null);
      login.reset();
    }
    onOpenChange(next);
  };

  const current = stepIndex(step);
  // The first visible step that is not finished yet is the one in progress.
  const activeStep =
    step === null || step === "done"
      ? null
      : (visibleSteps.find((s) => stepIndex(s) >= current) ?? null);

  return (
    <Dialog
      open={open}
      onOpenChange={close}
      title={t("settings.accounts.login.title")}
      description={t("settings.accounts.login.description")}
      closeLabel={t("common.close")}
      width={480}
      footer={
        <>
          <Button variant="ghost" onClick={() => close(false)}>
            {t("settings.accounts.login.cancel")}
          </Button>
          {step === null ? (
            <Button variant="primary" icon={<MicrosoftMark size={15} />} onClick={start}>
              {t("settings.accounts.addMicrosoft")}
            </Button>
          ) : null}
        </>
      }
    >
      {login.isError ? (
        <p role="alert" className="mb-3 text-[13px] text-flare-400">
          {t("settings.accounts.login.failed")}
        </p>
      ) : null}
      <ol className="space-y-3 pt-1" aria-live="polite">
        {visibleSteps.map((s) => {
          const index = stepIndex(s);
          const done = step === "done" || (step !== null && current > index);
          const active = s === activeStep;
          return (
            <li key={s} className="flex items-center gap-3">
              <span
                className={cn(
                  "flex size-7 items-center justify-center rounded-full ring-1 transition-colors duration-300",
                  done
                    ? "bg-jade-400/15 text-jade-400 ring-jade-400/40"
                    : active
                      ? "text-gold-300 ring-gold-400/60"
                      : "text-ash-500 ring-white/10",
                )}
              >
                <AnimatePresence mode="wait" initial={false}>
                  {done ? (
                    <motion.span
                      key="done"
                      initial={{ scale: 0.4, opacity: 0 }}
                      animate={{ scale: 1, opacity: 1 }}
                    >
                      <Check size={14} weight="bold" />
                    </motion.span>
                  ) : active ? (
                    <motion.span
                      key="spin"
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
                  "text-[14px]",
                  done ? "text-ash-300" : active ? "font-medium text-bone-50" : "text-ash-500",
                )}
              >
                {t(`settings.accounts.login.steps.${s}`)}
              </span>
            </li>
          );
        })}
      </ol>
    </Dialog>
  );
}

export function AccountsSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: accounts = [] } = useQuery({
    queryKey: ["accounts"],
    queryFn: () => api.accounts(),
  });
  const [name, setName] = useState("");
  const [nameError, setNameError] = useState<string | null>(null);
  const [loginOpen, setLoginOpen] = useState(false);
  const refresh = () => void queryClient.invalidateQueries({ queryKey: ["accounts"] });

  const addOffline = useMutation({
    mutationFn: (n: string) => api.addOfflineAccount(n),
    onSuccess: (account) => {
      refresh();
      setName("");
      toast.success(t("settings.accounts.added", { name: account.username }));
    },
  });

  const submitOffline = () => {
    const trimmed = name.trim();
    if (!OFFLINE_NAME.test(trimmed)) {
      setNameError(t("settings.accounts.invalidName"));
      return;
    }
    addOffline.mutate(trimmed);
  };

  return (
    <div className="pt-2">
      {accounts.length === 0 ? (
        <p className="py-3 text-ash-400">{t("settings.accounts.empty")}</p>
      ) : null}
      <ul className="space-y-1.5">
        {accounts.map((a) => (
          <li
            key={a.id}
            className={cn(
              "flex items-center gap-3.5 rounded-[var(--radius-panel)] px-3 py-2.5",
              a.active ? "bg-white/6 ring-1 ring-gold-400/30" : "hover:bg-white/4",
            )}
          >
            <PlayerHead name={a.username} src={a.headUrl} size={40} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="truncate text-[14.5px] font-semibold text-bone-50">
                  {a.username}
                </span>
                {a.active ? <Chip tone="gold">{t("settings.accounts.active")}</Chip> : null}
              </div>
              <p className="text-[12.5px] text-ash-400">
                {a.kind === "microsoft" ? t("accounts.microsoft") : t("accounts.offline")}
              </p>
            </div>
            {!a.active ? (
              <Button
                size="sm"
                variant="secondary"
                onClick={() => {
                  void api.setActiveAccount(a.id).then(refresh);
                }}
              >
                {t("settings.accounts.makeActive")}
              </Button>
            ) : null}
            <IconButton
              label={t("settings.accounts.remove")}
              icon={<Trash size={17} />}
              size="sm"
              onClick={() => {
                void api.removeAccount(a.id).then(() => {
                  refresh();
                  toast(t("settings.accounts.removed", { name: a.username }));
                });
              }}
            />
          </li>
        ))}
      </ul>

      <div className="mt-6 grid grid-cols-2 gap-5 border-t border-white/7 pt-5">
        <div>
          <p className="text-[14px] font-medium text-bone-50">
            {t("settings.accounts.addMicrosoft")}
          </p>
          <p className="mt-1 text-[12.5px] text-ash-400">
            {t("settings.accounts.addMicrosoftHelp")}
          </p>
          <Button
            className="mt-3"
            variant="secondary"
            icon={<MicrosoftMark size={15} />}
            onClick={() => setLoginOpen(true)}
          >
            {t("settings.accounts.addMicrosoft")}
          </Button>
        </div>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            submitOffline();
          }}
        >
          <Field
            label={t("settings.accounts.offlineName")}
            help={t("settings.accounts.offlineHelp")}
            error={nameError}
            htmlFor="offline-name"
          >
            <div className="flex gap-2">
              <Input
                id="offline-name"
                value={name}
                invalid={Boolean(nameError)}
                maxLength={16}
                autoComplete="off"
                spellCheck={false}
                onChange={(e) => {
                  setName(e.target.value);
                  setNameError(null);
                }}
              />
              <Button
                type="submit"
                variant="secondary"
                icon={<UserPlus size={16} />}
                disabled={addOffline.isPending}
              >
                {t("settings.accounts.add")}
              </Button>
            </div>
          </Field>
        </form>
      </div>
      <MicrosoftLoginDialog open={loginOpen} onOpenChange={setLoginOpen} />
    </div>
  );
}
