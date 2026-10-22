// A cloud account in words: its state line (the hold wins over the live state, as the contract
// says), what to do about a stop, and its paid time. Pure, so the card, the console and the tests
// read the same thing.
import type { SignalState } from "@pl/ui";
import type { TFunction } from "i18next";
import { formatDuration, uiLocale } from "@/lib/format";
import type { CloudAccount } from "./api";

export type CloudTone = "ok" | "muted" | "warn" | "error";

export interface CloudStatus {
  signal: SignalState;
  text: string;
  tone: CloudTone;
  /** What to do about it (a stop cause, a blocked server, waiting for a slot). */
  hint: string | null;
  /** Milliseconds until the words change by themselves; null when they do not. */
  nextChangeMs: number | null;
}

const CAUSES = [
  "banned",
  "notWhitelisted",
  "wrongPassword",
  "premiumOnly",
  "loggedInElsewhere",
  "alreadyConnected",
  "loginTimeout",
  "unsupportedVersion",
  "secretUnreadable",
  "crashLoop",
  "forbiddenTarget",
];

const HINTS = [...CAUSES.filter((c) => c !== "banned"), "serverBlocked", "noHost"];

/** Text of a stop cause; an unknown cause reads like a plain disconnect. */
export function causeText(cause: string, t: TFunction): string {
  return t(`cloudAfk.cause.${CAUSES.includes(cause) ? cause : "other"}`);
}

function hintText(key: string, t: TFunction): string | null {
  return HINTS.includes(key) ? t(`cloudAfk.hint.${key}`) : null;
}

/** Studio's Turkish or English words, the other language when one is empty. */
export function localized(
  value: { tr: string; en: string } | null | undefined,
  lang: "tr" | "en",
): string | null {
  const text = value?.[lang]?.trim() || value?.tr?.trim() || value?.en?.trim();
  return text || null;
}

/**
 * The account's state at `nowMs` (the service's clock: the UI's own plus the overview's skew).
 * `lang` picks Studio's words of a blocked server.
 */
export function cloudStatus(
  account: CloudAccount,
  nowMs: number,
  t: TFunction,
  lang: "tr" | "en" = "tr",
): CloudStatus {
  const still = (
    signal: SignalState,
    text: string,
    tone: CloudTone,
    hint: string | null = null,
  ) => ({
    signal,
    text,
    tone,
    hint,
    nextChangeMs: null,
  });
  const hold = account.hold;
  if (hold) {
    switch (hold.kind) {
      case "suspended":
        return still("idle", t("cloudAfk.state.suspended"), "error");
      case "restricted":
        return still("idle", t("cloudAfk.state.restricted"), "error");
      case "expired":
        return still("idle", t("cloudAfk.state.expired"), "warn");
      case "paused":
        return still(
          "idle",
          hold.mode === "off" ? t("cloudAfk.state.off") : t("cloudAfk.state.paused"),
          "muted",
        );
      case "no_host":
        return still("idle", t("cloudAfk.state.noHost"), "muted", hintText("noHost", t));
      case "server_blocked": {
        const reason = localized(hold.reason, lang);
        return still(
          "offline",
          reason
            ? t("cloudAfk.state.serverBlockedBecause", { reason })
            : t("cloudAfk.state.serverBlocked"),
          "error",
          hintText("serverBlocked", t),
        );
      }
      case "cause":
        return still(
          "offline",
          t("cloudAfk.state.stoppedBecause", { reason: causeText(hold.cause, t) }),
          "error",
          hintText(hold.cause, t),
        );
      default:
        // A hold this launcher does not know yet: the most generic words of its kind.
        return still("idle", t("cloudAfk.state.stopped"), "muted");
    }
  }
  if (account.expired) return still("idle", t("cloudAfk.state.expired"), "warn");
  const live = account.live;
  switch (live.state) {
    case "running": {
      if (!live.since) return still("online", t("cloudAfk.state.runningNow"), "ok");
      const up = Math.max(0, nowMs / 1000 - live.since);
      // The words change where the shown seconds (first minute) or minutes do.
      const next = up < 60 ? 1000 - (nowMs % 1000) : 60_000 - ((up * 1000) % 60_000);
      return {
        signal: "online",
        text: t("cloudAfk.state.running", {
          uptime: formatDuration(up < 60 ? up : Math.floor(up / 60) * 60),
        }),
        tone: "ok",
        hint: null,
        nextChangeMs: Math.max(250, Math.round(next)),
      };
    }
    case "connecting":
      return still("pending", t("cloudAfk.state.connecting"), "muted");
    case "queued":
      return still(
        "idle",
        live.reason === "target_cap" ? t("cloudAfk.state.queuedFull") : t("cloudAfk.state.queued"),
        "muted",
      );
    case "waiting": {
      if (!live.retryAt) return still("idle", t("cloudAfk.state.waiting"), "muted");
      const left = live.retryAt * 1000 - nowMs;
      if (left <= 1000) return still("pending", t("cloudAfk.state.connecting"), "muted");
      return {
        signal: "idle",
        text: t("cloudAfk.state.waitingIn", { secs: Math.ceil(left / 1000) }),
        tone: "muted",
        hint: null,
        nextChangeMs: left % 1000 || 1000,
      };
    }
    case "stopped":
      return still("idle", t("cloudAfk.state.stopped"), "muted");
    default:
      return still("idle", t("cloudAfk.state.unknown"), "muted");
  }
}

/** In the game, or on its way there: a stop makes sense, a console can type. */
export function isCloudActive(account: CloudAccount): boolean {
  return account.desired === "run" && !account.hold && !account.expired;
}

/** The client is in the game now (the console can send). */
export function isInGame(account: CloudAccount): boolean {
  return isCloudActive(account) && account.live.state === "running";
}

/** What the card's main button does. */
export type CloudPrimary = "start" | "stop" | "renew" | "none";

export function cloudPrimary(account: CloudAccount): CloudPrimary {
  const hold = account.hold?.kind;
  if (account.expired || hold === "expired") return "renew";
  if (hold === "suspended" || hold === "restricted") return "none";
  // A stop for good ends with the next start (the player fixed it; the service clears the hold).
  const stoppedForGood = hold === "cause" || hold === "server_blocked";
  return account.desired === "run" && !stoppedForGood ? "stop" : "start";
}

/** The paid time left in words; for an expired account how long its settings are kept. */
export function timeLeft(account: CloudAccount, nowMs: number, t: TFunction): string {
  const now = nowMs / 1000;
  if (account.expired) {
    return t("cloudAfk.time.graceLeft", {
      count: Math.max(0, Math.ceil((account.graceUntil - now) / 86_400)),
    });
  }
  const left = account.expiresAt - now;
  if (left >= 86_400) return t("cloudAfk.time.daysLeft", { count: Math.floor(left / 86_400) });
  if (left >= 3600) return t("cloudAfk.time.hoursLeft", { count: Math.floor(left / 3600) });
  return t("cloudAfk.time.minutesLeft", { count: Math.max(1, Math.ceil(left / 60)) });
}

/** "60.00" → "60 TL", "59.90" → "59,90 TL" (in English "59.90 TL"). */
export function formatPrice(priceTl: string): string {
  const value = Number(priceTl);
  if (!Number.isFinite(value)) return `${priceTl} TL`;
  const whole = Math.round(value * 100) % 100 === 0;
  const number = new Intl.NumberFormat(uiLocale(), {
    minimumFractionDigits: whole ? 0 : 2,
    maximumFractionDigits: 2,
  }).format(value);
  return `${number} TL`;
}

/** `play.projects.gg`, or `host:port` when a port other than the default is set. */
export function serverLabel(server: { host: string; port: number }): string {
  return server.port && server.port !== 25565 ? `${server.host}:${server.port}` : server.host;
}
