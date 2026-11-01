import { ArrowCounterClockwise } from "@phosphor-icons/react";
import { Button, Dialog, Field, Input, Progress, SwapLabel } from "@pl/ui";
import { useMutation, useQuery } from "@tanstack/react-query";
import { type ReactNode, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAccounts } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import { listFormat } from "@/lib/format";
import { ErrorLine, failure } from "../failure";
import { confirmMatches, removedNames } from "./model";

function Line({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[84px_1fr] gap-3 py-1.5">
      <dt className="text-[13px] text-ash-400">{label}</dt>
      <dd className="min-w-0 space-y-0.5 text-[13.5px] break-words text-bone-100">{children}</dd>
    </div>
  );
}

/**
 * "Launcher'ı sıfırla" (Settings → Veri): what goes, the saved names by name, and what stays;
 * the button acts once the player types the word. The launcher then restarts and the start
 * removes the accounts and settings (src-tauri reset.rs).
 */
export function ResetDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const r = "launcherReset";
  const inputId = useId();
  const [typed, setTyped] = useState("");
  const accounts = useAccounts();
  const afk = useQuery({ queryKey: ["afk"], queryFn: () => api.afkAccounts(), enabled: open });
  const reset = useMutation({ mutationFn: () => api.data.reset() });
  const restarting = reset.isSuccess;
  const busy = reset.isPending || restarting;
  const ready = !busy && confirmMatches(typed, t(`${r}.word`));
  const names = removedNames(accounts.data, afk.data);

  const close = () => {
    if (busy) return;
    setTyped("");
    reset.reset();
    onClose();
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) close();
      }}
      title={restarting ? t(`${r}.title`) : t(`${r}.dialogTitle`)}
      description={restarting ? undefined : t(`${r}.dialogBody`)}
      closeLabel={t("common.close")}
      width={480}
      morph
      dismissible={!busy}
      dismissOnOutsideClick={false}
      footer={
        restarting ? null : (
          <>
            <Button variant="ghost" disabled={busy} onClick={close}>
              {t("dataSettings.cancel")}
            </Button>
            <Button
              variant="danger"
              icon={<ArrowCounterClockwise size={16} />}
              disabled={!ready}
              onClick={() => reset.mutate()}
            >
              <SwapLabel
                labels={[t(`${r}.confirm`), t(`${r}.working`)]}
                active={reset.isPending ? 1 : 0}
              />
            </Button>
          </>
        )
      }
    >
      {restarting ? (
        <div className="flex flex-col items-center gap-3 py-4 text-center" aria-live="polite">
          <p className="font-display text-[18px] font-semibold text-bone-50">
            {t(`${r}.restarting`)}
          </p>
          <Progress className="w-56" label={t(`${r}.restarting`)} />
        </div>
      ) : (
        <div className="pt-1">
          <dl className="rounded-[10px] bg-obsidian-950/40 px-4 py-2 ring-1 ring-white/6">
            <Line label={t(`${r}.removed`)}>
              {names.accounts.length > 0 ? (
                <p>{t(`${r}.accounts`, { names: listFormat().format(names.accounts) })}</p>
              ) : null}
              {names.afk.length > 0 ? (
                <p>{t(`${r}.afk`, { names: listFormat().format(names.afk) })}</p>
              ) : null}
              <p>{t(`${r}.settings`)}</p>
            </Line>
            <Line label={t(`${r}.kept`)}>
              <p>{t(`${r}.keptValue`)}</p>
            </Line>
          </dl>
          <Field label={t(`${r}.confirmLabel`)} htmlFor={inputId} className="mt-4">
            <Input
              id={inputId}
              value={typed}
              autoFocus
              autoComplete="off"
              spellCheck={false}
              disabled={busy}
              onChange={(event) => setTyped(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && ready) reset.mutate();
              }}
            />
          </Field>
          {reset.error ? (
            <ErrorLine className="mt-4" error={failure(reset.error, t(`${r}.failed`))} />
          ) : null}
        </div>
      )}
    </Dialog>
  );
}
