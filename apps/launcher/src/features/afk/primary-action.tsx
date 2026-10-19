// The main button of an AFK account (its card, its console): "Bağla", "Durdur", "İptal" while the
// AFK client is fetched for it, "Tekrar dene" after the client could not be had. The words change
// in place on one button, so the focus stays on it.
import { ArrowsClockwise, Play, Stop, X } from "@phosphor-icons/react";
import { Button, SwapLabel } from "@pl/ui";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { isAfkIdle } from "@/features/accounts/afk-match";
import { type AfkAccount, type AfkState, api } from "@/lib/api";
import { runAfk } from "./errors";

export type AfkAction = "connect" | "stop" | "cancel" | "retry";

/** What the account's main button does now. */
export function primaryAction(state: AfkState): AfkAction {
  if (state.kind === "preparing") return "cancel";
  if (state.kind === "clientFailed") return "retry";
  return isAfkIdle(state) ? "connect" : "stop";
}

/** Delay before a control that replaced the one just pressed takes presses from the keyboard. */
const ARM_MS = 800;

/**
 * "İptal" and "Tekrar dene" take the place of the button just pressed: they take presses only
 * once the pointer has left the button, or 0.8 s later when it is not on it (the keyboard), so the
 * second press of a double click never cancels the download it just started (as OYNA does,
 * docs/design.md). Returns whether presses count, and the pointer handlers that arm it.
 */
export function useArmed(action: AfkAction) {
  const guarded = action === "cancel" || action === "retry";
  // The action armed last (on the first render, the one shown: nothing was just pressed). A
  // guarded action that takes over from another one is not armed yet.
  const [armedFor, setArmedFor] = useState<AfkAction>(action);
  const inside = useRef(false);
  useEffect(() => {
    if (!guarded) {
      setArmedFor(action);
      return;
    }
    if (armedFor === action) return;
    const id = window.setTimeout(() => {
      if (!inside.current) setArmedFor(action);
    }, ARM_MS);
    return () => window.clearTimeout(id);
  }, [action, armedFor, guarded]);
  return {
    armed: !guarded || armedFor === action,
    onPointerEnter: () => {
      inside.current = true;
    },
    onPointerLeave: () => {
      inside.current = false;
      setArmedFor(action);
    },
  };
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
  const { armed, onPointerEnter, onPointerLeave } = useArmed(action);
  const labels: Record<AfkAction, string> = {
    connect: t("afk.connect"),
    stop: t("afk.stop"),
    cancel: t("afkConnect.cancel"),
    retry: t("common.retry"),
  };
  const press = () => {
    if (!armed) return;
    if (action === "connect" || action === "retry") {
      runAfk(api.afkConnect(account.id), t("afk.connectFailed"));
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
      aria-disabled={armed ? undefined : true}
      onClick={press}
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
      className={className}
    >
      <SwapLabel labels={ORDER.map((a) => labels[a])} active={ORDER.indexOf(action)} />
    </Button>
  );
}
