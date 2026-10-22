// The main button of an AFK account (its card, its console): "Bağla", "Durdur", "İptal" while the
// AFK client is fetched for it, "Tekrar dene" after the client could not be had. The words change
// in place on one button, so the focus stays on it.
import { ArrowsClockwise, Play, Stop, X } from "@phosphor-icons/react";
import { Button, SwapLabel, usePressGuard } from "@pl/ui";
import type { MouseEvent, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { isAfkIdle } from "@/features/accounts/afk-match";
import { type AfkAccount, type AfkState, api } from "@/lib/api";
import { useGuardedConnect } from "./cloud/use-cloud";
import { runAfk } from "./errors";

export type AfkAction = "connect" | "stop" | "cancel" | "retry";

/** What the account's main button does now. */
export function primaryAction(state: AfkState): AfkAction {
  if (state.kind === "preparing") return "cancel";
  if (state.kind === "clientFailed") return "retry";
  return isAfkIdle(state) ? "connect" : "stop";
}

const ICONS: Record<AfkAction, (size: number) => ReactNode> = {
  connect: (size) => <Play size={size} weight="fill" />,
  stop: (size) => <Stop size={size} weight="fill" />,
  cancel: (size) => <X size={size} weight="bold" />,
  retry: (size) => <ArrowsClockwise size={size} weight="bold" />,
};

const ORDER: AfkAction[] = ["connect", "stop", "cancel", "retry"];

export function AfkPrimaryAction({
  account,
  size = "sm",
  className,
}: {
  account: AfkAccount;
  size?: "sm" | "md";
  className?: string;
}) {
  const { t } = useTranslation();
  const action = primaryAction(account.state);
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
  };
  const press = (event: MouseEvent) => {
    if (!guard.take(event)) return;
    if (action === "connect" || action === "retry") {
      connect(account);
    } else {
      runAfk(api.afkStop(account.id), t("afk.stopFailed"));
    }
  };
  return (
    <Button
      size={size}
      variant={action === "connect" || action === "retry" ? "primary" : "secondary"}
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
