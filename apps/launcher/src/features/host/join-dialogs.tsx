// The windows of world sharing (HOST1). A join opens one only when the player must decide
// (host1.md §1.4): the world's mods need a copy of the friend's install (§5.3, "Kopyala ve
// katıl"), its Minecraft version is not installed (§5.4, "Kur ve katıl"), or the account's name
// cannot come in (PROTOCOL §6.4: another account joins instead). One question at a time, the
// oldest first; "Vazgeç" leaves that join. Also: the one sentence before "Dünyadan çıkar", and an
// opening that failed while the launcher was in front, with its ways out.
import { Warning } from "@phosphor-icons/react";
import { Button, Chip, cn, Dialog } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AccountFace } from "@/features/accounts/account-face";
import { useAccountsUi } from "@/features/accounts/store";
import { ACCOUNTS_KEY, useAccounts } from "@/features/accounts/use-accounts";
import { attempt } from "@/features/social/actions";
import { crossfade } from "@/features/social/motion";
import { hostApi } from "@/lib/api/host-api";
import type { JoinAnswer, JoinAsk, WorldGuest } from "@/lib/api/host-types";
import type { AccountSummary } from "@/lib/api/types";
import { formatBytes } from "@/lib/format";
import { copyOf, dismissFailure, meetOnProjects, removeGuest, retryOpen } from "./actions";
import { waitingAsk } from "./rules";
import { useHostUi } from "./store";

/** The other accounts the player may join with (not the active one, not the refused name). */
function otherAccounts(accounts: readonly AccountSummary[], refused: string): AccountSummary[] {
  const name = refused.toLowerCase();
  return accounts.filter((a) => !a.active && a.username.toLowerCase() !== name);
}

function AccountChoice({
  accounts,
  chosen,
  onChoose,
}: {
  accounts: readonly AccountSummary[];
  chosen: string | null;
  onChoose: (id: string) => void;
}) {
  const { t } = useTranslation();
  const name = useId();
  return (
    <div
      role="radiogroup"
      aria-label={t("host.name.account")}
      className="-mx-1 flex flex-col gap-1"
    >
      {accounts.map((account) => {
        const on = chosen === account.id;
        return (
          <label
            key={account.id}
            className={cn(
              "flex cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-2.5 py-2 ring-1 transition-[background-color,box-shadow] duration-150",
              on
                ? "bg-gold-500/[0.08] ring-gold-400/35"
                : "ring-transparent hover:bg-white/[0.045]",
            )}
          >
            <input
              type="radio"
              name={name}
              checked={on}
              onChange={() => onChoose(account.id)}
              className="sr-only"
            />
            <AccountFace account={account} size={32} />
            <span className="min-w-0 flex-1 truncate text-[14px] font-semibold text-bone-50">
              {account.username}
            </span>
            {account.kind === "microsoft" ? <Chip>Microsoft</Chip> : null}
            <span
              aria-hidden
              className={cn(
                "flex size-4 items-center justify-center rounded-full ring-2 transition-colors duration-150",
                on ? "ring-gold-400" : "ring-white/25",
              )}
            >
              <span
                className={cn(
                  "size-2 rounded-full bg-gold-400 transition-[opacity,transform,scale] duration-150",
                  on ? "scale-100 opacity-100" : "scale-50 opacity-0",
                )}
              />
            </span>
          </label>
        );
      })}
    </div>
  );
}

function askWords(
  t: (key: string, options?: Record<string, unknown>) => string,
  ask: JoinAsk,
  name: string,
) {
  switch (ask.kind) {
    case "copy":
      // Named, without a case ending (no name-suffix helper; SMALL9 decision 16).
      return {
        title: t("host.copy.title", { name }),
        body: t("host.copy.body"),
        confirm: t("host.copy.confirm"),
      };
    case "install":
      return {
        title: t("host.install.title", { version: ask.mc }),
        body: t("host.install.body"),
        confirm: t("host.install.confirm"),
      };
    case "name":
      return {
        title: ask.verdict === "own" ? t("host.name.own") : t("host.name.taken"),
        body: t("host.name.body"),
        confirm: t("host.name.confirm"),
      };
  }
}

/** A join's question (one at a time, the oldest first). */
export function JoinAskDialog() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const invites = useHostUi((s) => s.view.invites);
  const waiting = waitingAsk(invites);
  const { data: accounts = [] } = useAccounts();
  // The window keeps its words while it fades out.
  const last = useRef(waiting);
  if (waiting) last.current = waiting;
  const shown = waiting ?? last.current;
  const ask = shown?.join.ask;
  const others = ask?.kind === "name" ? otherAccounts(accounts, ask.name) : [];
  const [chosen, setChosen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const key = waiting ? `${waiting.session}:${waiting.join.ask.kind}` : null;
  // biome-ignore lint/correctness/useExhaustiveDependencies: each new question starts afresh
  useEffect(() => {
    setBusy(false);
    setChosen(null);
  }, [key]);
  const pick = chosen ?? others[0]?.id ?? null;

  if (!shown || !ask) return null;
  const words = askWords(t, ask, shown.name);
  const session = shown.session;

  const leave = () => {
    if (!waiting) return;
    void attempt(() => hostApi.cancelJoin(session));
  };
  const answer = async () => {
    let reply: JoinAnswer;
    if (ask.kind === "name") {
      if (!pick) return;
      reply = { kind: "account", accountId: pick };
    } else if (ask.kind === "copy") {
      reply = { kind: "copy", ...copyOf(shown) };
    } else {
      reply = { kind: "install" };
    }
    setBusy(true);
    const done = await attempt(() => hostApi.answer(session, reply));
    if (done === undefined) setBusy(false);
    if (reply.kind === "account") void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
  };
  const addAccount = () => {
    leave();
    useAccountsUi.getState().openCenter({ focus: "add" });
  };

  return (
    <Dialog
      open={waiting !== null}
      onOpenChange={(open) => {
        if (!open) leave();
      }}
      title={words.title}
      description={words.body}
      closeLabel={t("common.close")}
      width={440}
      morph
      footer={
        <>
          <Button variant="ghost" onClick={leave}>
            {t("fix2.connect.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={busy || (ask.kind === "name" && !pick)}
            onClick={() => void answer()}
          >
            {words.confirm}
          </Button>
        </>
      }
    >
      <AnimatePresence initial={false} mode="popLayout">
        <motion.div key={ask.kind} {...crossfade()}>
          {ask.kind === "copy" ? (
            // What the copy takes, as a quiet line under the sentence (a boxed single line with a
            // chest read as a card of its own; SMALL10). Missing mods only when there are some.
            <div className="space-y-2">
              <p className="text-meta text-ash-300 tabular-nums">
                {t("host.copy.size", { count: ask.mods, size: formatBytes(ask.bytes) })}
              </p>
              {ask.unknown > 0 ? (
                <p className="flex items-start gap-2.5 text-[12.5px] text-ash-300">
                  <Warning size={16} className="mt-px shrink-0 text-gold-300" aria-hidden />
                  {t("host.join.missing", { count: ask.unknown })}
                </p>
              ) : null}
            </div>
          ) : ask.kind === "name" ? (
            others.length ? (
              <AccountChoice accounts={others} chosen={pick} onChoose={setChosen} />
            ) : (
              <Button variant="secondary" onClick={addAccount}>
                {t("host.name.add")}
              </Button>
            )
          ) : null}
        </motion.div>
      </AnimatePresence>
    </Dialog>
  );
}

/** "Dünyadan çıkar": one sentence first (the friend needs a new invite to come back). */
export function RemoveGuestDialog() {
  const { t } = useTranslation();
  const removing = useHostUi((s) => s.removing);
  const last = useRef<WorldGuest | null>(null);
  if (removing) last.current = removing;
  const shown = removing ?? last.current;
  if (!shown) return null;
  const close = () => useHostUi.setState({ removing: null });
  return (
    <Dialog
      open={removing !== null}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      title={t("host.remove.ask", { name: shown.name })}
      description={t("host.remove.body")}
      closeLabel={t("common.close")}
      width={420}
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t("fix2.connect.cancel")}
          </Button>
          <Button
            variant="danger"
            onClick={() => {
              close();
              void removeGuest(shown);
            }}
          >
            {t("host.remove.action")}
          </Button>
        </>
      }
    />
  );
}

/** An opening that failed while the launcher was in front: why, and the two ways out. */
export function OpenFailedDialog() {
  const { t } = useTranslation();
  const failure = useHostUi((s) => s.failure);
  const session = useHostUi((s) => s.view.session);
  const failed = session?.phase === "failed" ? session : null;
  const open = failure !== null && failed !== null;
  const last = useRef(failed);
  if (failed) last.current = failed;
  const shown = failed ?? last.current;
  if (!shown) return null;
  const close = () => useHostUi.getState().showFailure(null);
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) close();
      }}
      title={t("host.strip.failed")}
      description={t(`host.failed.${shown.reason}`)}
      closeLabel={t("common.close")}
      width={440}
      footer={
        <>
          <Button variant="ghost" className="mr-auto" onClick={dismissFailure}>
            {t("host.sheet.dismiss")}
          </Button>
          {shown.guests.length ? (
            <Button
              variant="secondary"
              onClick={() => void meetOnProjects(shown.guests.map((g) => g.friend))}
            >
              {t("host.sheet.meet")}
            </Button>
          ) : null}
          {shown.reason === "unavailable" ? (
            <Button variant="primary" onClick={() => retryOpen(shown)}>
              {t("host.sheet.retry")}
            </Button>
          ) : null}
        </>
      }
    />
  );
}
