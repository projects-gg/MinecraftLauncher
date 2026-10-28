// The main button of an AFK account (its card, its console): "Bağla", "Durdur", "İptal" while the
// AFK client is fetched for it, "Tekrar dene" after the client could not be had, and "Şifreyi
// düzelt" after the server turned its password down (the settings open on the login line; a new
// "Bağla" would only be refused again). The words change in place on one button, so the focus stays
// on it.
import { ArrowsClockwise, Key, Play, Stop, X } from "@phosphor-icons/react";
import { Button, SwapLabel, usePressGuard } from "@pl/ui";
import type { MouseEvent, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { isAfkIdle } from "@/features/accounts/afk-match";
import { type AfkAccount, type AfkState, api } from "@/lib/api";
import { useGuardedConnect } from "./cloud/use-cloud";
import { runAfk } from "./errors";

export type AfkAction = "connect" | "stop" | "cancel" | "retry" | "fixPassword";

/** What the account's main button does now. */
export function primaryAction(state: AfkState): AfkAction {
  if (state.kind === "preparing") return "cancel";
  if (state.kind === "clientFailed") return "retry";
  if (state.kind === "error" && state.cause === "wrongPassword") return "fixPassword";
  return isAfkIdle(state) ? "connect" : "stop";
}

/** Stopped because the server turned its password down. */
export function stoppedOnPassword(state: AfkState): boolean {
  return state.kind === "error" && state.cause === "wrongPassword";
}

const ICONS: Record<AfkAction, (size: number) => ReactNode> = {
  connect: (size) => <Play size={size} weight="fill" />,
  stop: (size) => <Stop size={size} weight="fill" />,
  cancel: (size) => <X size={size} weight="bold" />,
  retry: (size) => <ArrowsClockwise size={size} weight="bold" />,
  fixPassword: (size) => <Key size={size} weight="bold" />,
};

const ORDER: AfkAction[] = ["connect", "stop", "cancel", "retry", "fixPassword"];

export function AfkPrimaryAction({
  account,
  size = "sm",
  quiet = false,
  onFixPassword,
  className,
}: {
  account: AfkAccount;
  size?: "sm" | "md";
  /** Not the card's next step (a secondary look). */
  quiet?: boolean;
  /** Opens the settings on the login line; without it the button connects as before. */
  onFixPassword?: () => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const wanted = primaryAction(account.state);
  const action: AfkAction = wanted === "fixPassword" && !onFixPassword ? "connect" : wanted;
  // Each action takes the place of the one just pressed ("İptal" or "Durdur" after "Bağla",
  // "Bağla" after "İptal"): it never takes the rest of that double click, and takes presses once
  // the pointer has left the button or 0.8 s later, also under a resting pointer (@pl/ui
  // press-guard; E2E2).
  const guard = usePressGuard(action);
  // Asks first when the same account runs in 7/24 Bulut.
  const connect = useGuardedConnect();
  const labels: Record<AfkAction, string> = {
    connect: t("afk.connect"),
    stop: t("afk.stop"),
    cancel: t("afkConnect.cancel"),
    retry: t("common.retry"),
    fixPassword: t("afkClear.fixPassword"),
  };
  const press = (event: MouseEvent) => {
    if (!guard.take(event)) return;
    if (action === "fixPassword") onFixPassword?.();
    else if (action === "connect" || action === "retry") connect(account);
    else runAfk(api.afkStop(account.id), t("afk.stopFailed"));
  };
  const gold = action === "connect" || action === "retry" || action === "fixPassword";
  return (
    <Button
      size={size}
      variant={gold && !quiet ? "primary" : "secondary"}
      icon={ICONS[action](size === "sm" ? 14 : 15)}
      // Already on its way out: it can be connected again once it has stopped.
      disabled={account.state.kind === "stopping"}
      aria-disabled={guard.armed ? undefined : true}
      onClick={press}
      {...guard.handlers}
      className={className}
    >
      <SwapLabel labels={ORDER.map((a) => labels[a])} active={ORDER.indexOf(action)} />
    </Button>
  );
}
