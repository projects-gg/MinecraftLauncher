// Launch guard: the server lets one connection per player name. When OYNA starts the game with
// an account whose name has an AFK session running, ask first: stop the AFK and play (it comes
// back by itself when the game closes), keep the AFK running, or cancel.
import { Button, toast } from "@pl/ui";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { type AccountSummary, type AfkAccount, api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { AccountDialog } from "./account-dialog";
import { findAfkSession } from "./afk-match";
import { useAccountsUi } from "./store";

export type AfkChoice = "stop" | "keep" | "cancel";

interface GuardRequest {
  account: AccountSummary;
  session: AfkAccount;
}

interface GuardState {
  request: GuardRequest | null;
  resolve: ((choice: AfkChoice) => void) | null;
  ask: (request: GuardRequest) => Promise<AfkChoice>;
  answer: (choice: AfkChoice) => void;
}

const useAfkGuard = create<GuardState>()((set, get) => ({
  request: null,
  resolve: null,
  ask: (request) =>
    new Promise<AfkChoice>((resolve) => {
      get().resolve?.("cancel");
      set({ request, resolve });
    }),
  answer: (choice) => {
    const { resolve } = get();
    set({ request: null, resolve: null });
    resolve?.(choice);
  },
}));

/** The part of the launch store the guard watches (the game phase). */
export interface LaunchPhaseSource {
  subscribe: (listener: (state: { phase: { kind: string } }) => void) => () => void;
}

/** If the game never leaves "idle" after the AFK was stopped, bring the AFK back after this. */
const NEVER_STARTED_MS = 20_000;

/** Brings the AFK session back once the game has run and closed (or failed to start). */
function restartAfterGame(session: AfkAccount, launch: LaunchPhaseSource) {
  let started = false;
  let done = false;
  const restart = async () => {
    if (done) return;
    done = true;
    unsubscribe();
    clearTimeout(fallback);
    try {
      const current = (await api.afkAccounts()).find((s) => s.id === session.id);
      // Removed meanwhile, or the player already started it again.
      if (!current || (current.state.kind !== "stopped" && current.state.kind !== "error")) return;
      await api.afkConnect(session.id);
      toast(i18n.t("accounts.afkGuard.restarted", { name: session.nickname }));
    } catch {
      // The AFK page shows the session's state; nothing else to do here.
    }
  };
  const unsubscribe = launch.subscribe((state) => {
    const kind = state.phase.kind;
    if (kind !== "idle") started = true;
    if (started && (kind === "idle" || kind === "error")) void restart();
  });
  const fallback = setTimeout(() => {
    if (!started) void restart();
  }, NEVER_STARTED_MS);
}

let asking = false;

/**
 * Called by the launch flow before OYNA starts the game. Resolves to false when the player
 * cancels. Without a mounted account host (first-run flow) it never asks.
 */
export async function confirmAfkBeforePlay(launch: LaunchPhaseSource): Promise<boolean> {
  if (asking) return false;
  if (useAccountsUi.getState().hosts === 0) return true;
  let account: AccountSummary | undefined;
  let session: AfkAccount | null = null;
  try {
    const [accounts, sessions] = await Promise.all([api.accounts(), api.afkAccounts()]);
    account = accounts.find((a) => a.active);
    session = account ? findAfkSession(account.username, sessions) : null;
  } catch {
    return true;
  }
  if (!account || !session) return true;

  asking = true;
  try {
    const choice = await useAfkGuard.getState().ask({ account, session });
    if (choice === "cancel") return false;
    if (choice === "stop") {
      await api.afkStop(session.id).catch(() => {});
      restartAfterGame(session, launch);
    }
    return true;
  } finally {
    asking = false;
  }
}

/** The question itself (one sentence, three answers); mounted once by the account host. */
export function AfkLaunchGuardDialog() {
  const { t } = useTranslation();
  const request = useAfkGuard((s) => s.request);
  const answer = useAfkGuard((s) => s.answer);
  // Keeps the name on screen while the dialog animates out.
  const shown = useRef(request);
  if (request) shown.current = request;
  const stop = useRef<HTMLButtonElement>(null);
  return (
    <AccountDialog
      initialFocus={stop}
      open={Boolean(request)}
      onOpenChange={(open) => {
        if (!open) answer("cancel");
      }}
      title={t("accounts.afkGuard.title", { name: shown.current?.session.nickname ?? "" })}
      description={t("accounts.afkGuard.body")}
      closeLabel={t("accounts.afkGuard.cancel")}
      width={500}
      footer={
        <>
          <Button variant="ghost" className="h-11" onClick={() => answer("cancel")}>
            {t("accounts.afkGuard.cancel")}
          </Button>
          <Button variant="secondary" className="h-11" onClick={() => answer("keep")}>
            {t("accounts.afkGuard.keep")}
          </Button>
          <Button ref={stop} variant="primary" className="h-11" onClick={() => answer("stop")}>
            {t("accounts.afkGuard.stop")}
          </Button>
        </>
      }
    />
  );
}
