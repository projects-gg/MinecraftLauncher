// "Ayarları kopyala" (CAFK-FEAT §3.4): one account's anti-AFK, commands, replies and in-game name
// onto the chosen ones; the server and version only when ticked; never its name, language or
// inventory choice. Secret rows are sealed again for every target: from this computer's copy, or
// from what the player types here (per account unless "Hepsine aynısını yaz" is picked: accounts
// rarely share a password, AFK-CLEAR look 2). Each target restarts with the new settings, as after
// any change. A source whose password the server turned down gives nothing while this computer's
// copy of that password would go along (it would lock every target out too).
import { Button, Dialog, Field, Segmented, Select } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check } from "@/features/settings/cleanup/check";
import { AfkHead } from "../afk-head";
import { SecretField } from "../secret";
import { type CloudAccount, type CloudCopySecret, type CloudCopySource, cloudApi } from "./api";
import { commandOf, lineOf, rowName } from "./bulk";
import { useBulkToast } from "./bulk-bar";
import { looksSensitive } from "./sensitive";
import { serverLabel, stoppedOnPassword } from "./status";
import { storeAccount } from "./use-cloud";

type SecretMode = "same" | "each";

export function CopyDialog({
  open,
  accounts,
  sources,
  initialSource,
  initialTargets,
  onClose,
}: {
  open: boolean;
  /** Every account of the player, in the list's order. */
  accounts: CloudAccount[];
  /** The accounts the settings may come from (the card's own, or the chosen ones). */
  sources: CloudAccount[];
  initialSource: string | null;
  initialTargets: string[];
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const report = useBulkToast();
  const [sourceId, setSourceId] = useState<string | null>(initialSource);
  const [targets, setTargets] = useState<Set<string>>(new Set(initialTargets));
  const [withServer, setWithServer] = useState(false);
  const [plan, setPlan] = useState<CloudCopySource | null>(null);
  const [planFailed, setPlanFailed] = useState(false);
  const [mode, setMode] = useState<SecretMode>("each");
  const [typed, setTyped] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const serverId = useId();

  // A fresh open starts from what the caller chose.
  // biome-ignore lint/correctness/useExhaustiveDependencies: resets on every open
  useEffect(() => {
    if (!open) return;
    setSourceId(initialSource);
    setTargets(new Set(initialTargets.filter((id) => id !== initialSource)));
    setWithServer(false);
    setMode("each");
    setTyped({});
  }, [open]);

  // The source's secret rows this computer has no words for.
  useEffect(() => {
    if (!open || !sourceId) return;
    let live = true;
    setPlan(null);
    setPlanFailed(false);
    cloudApi
      .copySource(sourceId)
      .then((next) => live && setPlan(next))
      .catch(() => live && setPlanFailed(true));
    return () => {
      live = false;
    };
  }, [open, sourceId]);

  const source = accounts.find((account) => account.id === sourceId);
  const others = accounts.filter((account) => account.id !== sourceId);
  const chosen = others.filter((account) => targets.has(account.id));
  const toType = plan?.toType ?? [];
  // One account: its own field, by name (a nameless one read as every account's password).
  const each = mode === "each" || chosen.length === 1;
  const key = (row: string, target: string | null) => `${row}|${target ?? "*"}`;
  const missing = toType.some((row) =>
    each
      ? chosen.some((target) => !(typed[key(row.row, target.id)] ?? "").trim())
      : !(typed[key(row.row, null)] ?? "").trim(),
  );
  // Its refused login line would go along from this computer's copy (none of its secret rows is
  // to be typed here, where new words would take its place).
  const refused =
    source !== undefined &&
    plan !== null &&
    stoppedOnPassword(source) &&
    !toType.some((row) => looksSensitive(row.preview));
  // "Kopyala" waits for every password, the footer saying why, as the panels' change bars wait for
  // a marked field: enabled over empty fields it read as copying the accounts in without one
  // (AFK-CLEAR look 3).
  const blocked = !source || !plan || chosen.length === 0 || busy || refused || missing;

  const toggle = (id: string, on: boolean) =>
    setTargets((current) => {
      const next = new Set(current);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });

  const copy = async () => {
    if (!source || blocked) return;
    setBusy(true);
    const secrets = toType.flatMap((row): CloudCopySecret[] =>
      each
        ? chosen.map((target) => ({
            row: row.row,
            target: target.id,
            text: lineOf(row.preview, typed[key(row.row, target.id)] ?? ""),
          }))
        : [
            {
              row: row.row,
              target: null,
              text: lineOf(row.preview, typed[key(row.row, null)] ?? ""),
            },
          ],
    );
    try {
      const results = await cloudApi.copySettings(
        source.id,
        chosen.map((target) => target.id),
        withServer,
        secrets,
      );
      for (const result of results) {
        if (result.account) storeAccount(queryClient, result.account);
      }
      report(
        "copy",
        results.map((result) => ({ ok: Boolean(result.account), code: result.code })),
      );
      onClose();
    } catch {
      report(
        "copy",
        chosen.map(() => ({ ok: false, code: null })),
      );
    } finally {
      setBusy(false);
    }
  };

  const secretField = (row: string, preview: string, target: CloudAccount | null) => {
    // The command stays in view before the field (`/login`), so only the password is typed.
    const command = commandOf(preview);
    return (
      <Field
        key={key(row, target?.id ?? null)}
        htmlFor={`copy-secret-${key(row, target?.id ?? null)}`}
        label={
          target
            ? t("cloudAfkFeat.copy.secretFor", { name: target.name, row: rowName(preview) })
            : t("cloudAfkFeat.copy.secretField", { row: rowName(preview) })
        }
      >
        {/* The AFK surfaces' one look for secret lines (secret.tsx): the command before the
            field, only the password typed. */}
        <SecretField
          id={`copy-secret-${key(row, target?.id ?? null)}`}
          placeholder={t("afkSettings.login.newSecret")}
          command={command}
          maxLength={256 - (command ? command.length + 1 : 0)}
          value={typed[key(row, target?.id ?? null)] ?? ""}
          onChange={(text) =>
            setTyped((current) => ({
              ...current,
              [key(row, target?.id ?? null)]: text,
            }))
          }
          className="h-[34px]"
        />
      </Field>
    );
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && !busy && onClose()}
      title={t("cloudAfkFeat.copy.title", { name: source?.name ?? "" })}
      closeLabel={t("cloudAfkFeat.copy.cancel")}
      width={520}
      morph
      footer={
        <>
          {missing && chosen.length > 0 && !refused ? (
            <p className="mr-auto text-[12.5px] text-ash-300">{t("afkClear.copy.typeFirst")}</p>
          ) : null}
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {t("cloudAfkFeat.copy.cancel")}
          </Button>
          <Button variant="primary" disabled={blocked} onClick={() => void copy()}>
            {t("cloudAfkFeat.copy.action")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {sources.length > 1 ? (
          <Field label={t("cloudAfkFeat.copy.source")}>
            <Select
              value={sourceId ?? ""}
              onValueChange={(id) => {
                setSourceId(id);
                // Typed words belong to the source's rows (row ids are per account).
                setTyped({});
                setTargets((current) => {
                  const next = new Set(current);
                  next.delete(id);
                  if (sourceId && sources.some((one) => one.id === sourceId)) next.add(sourceId);
                  return next;
                });
              }}
              options={sources.map((account) => ({ value: account.id, label: account.name }))}
              label={t("cloudAfkFeat.copy.source")}
            />
          </Field>
        ) : null}
        {refused && source ? (
          <p role="alert" className="text-[12.5px] text-flare-400">
            {t("afkClear.copy.refused", { name: source.name })}
          </p>
        ) : null}
        <p className="text-[13px] text-ash-300">{t("cloudAfkFeat.copy.what")}</p>
        <fieldset className="flex flex-col gap-1">
          <legend className="mb-1.5 text-[13px] font-medium text-bone-100">
            {t("cloudAfkFeat.copy.targets")}
          </legend>
          {others.map((account) => {
            const id = `copy-${account.id}`;
            return (
              <label
                key={account.id}
                htmlFor={id}
                className="flex cursor-pointer items-center gap-3 rounded-md px-2 py-1.5 transition-colors duration-150 hover:bg-white/[0.04]"
              >
                <Check
                  id={id}
                  checked={targets.has(account.id)}
                  onCheckedChange={(on) => toggle(account.id, on)}
                />
                <AfkHead nickname={account.name} size={24} />
                <span className="min-w-0 flex-1 truncate text-[13px] text-bone-50">
                  {account.name}
                </span>
                <span className="shrink-0 truncate text-[12.5px] text-ash-400">
                  {serverLabel(account.server)}
                </span>
              </label>
            );
          })}
        </fieldset>
        {/* What happens to the ticked accounts, under them (it sat under the passwords). */}
        <p className="-mt-2 px-2 text-[12.5px] text-ash-400">{t("cloudAfkFeat.copy.confirm")}</p>
        <label htmlFor={serverId} className="flex cursor-pointer items-center gap-3 px-2">
          <Check id={serverId} checked={withServer} onCheckedChange={setWithServer} />
          <span className="text-[13px] text-bone-100">{t("cloudAfkFeat.copy.withServer")}</span>
        </label>
        {toType.length && chosen.length ? (
          <div className="flex flex-col gap-3 border-t border-white/7 pt-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="text-[13px] font-medium text-bone-100">
                {t("cloudAfkFeat.copy.secrets")}
              </p>
              {chosen.length > 1 ? (
                <Segmented
                  // A quiet plate: gold stays "Kopyala"'s.
                  className="[--segmented-ink:var(--color-bone-50)] [--segmented-plate:rgb(255_255_255/0.16)]"
                  value={mode}
                  onValueChange={(next) => setMode(next as SecretMode)}
                  label={t("cloudAfkFeat.copy.secrets")}
                  options={[
                    { value: "same", label: t("cloudAfkFeat.copy.sameForAll") },
                    { value: "each", label: t("cloudAfkFeat.copy.perAccount") },
                  ]}
                />
              ) : null}
            </div>
            {toType.flatMap((row) =>
              each
                ? chosen.map((target) => secretField(row.row, row.preview, target))
                : [secretField(row.row, row.preview, null)],
            )}
          </div>
        ) : null}
        {planFailed ? (
          <p role="alert" className="text-[12.5px] text-flare-400">
            {t("cloudAfkFeat.copy.loadFailed")}
          </p>
        ) : null}
      </div>
    </Dialog>
  );
}
