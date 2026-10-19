// What an AFK account's card says about its client and its session's failures, from the backend's
// view (`AfkAccountView`). While a connect waits for the client to be fetched, or after it could
// not get it (pl_afk::client), that is the account's state; otherwise its session speaks. A
// failure is worded in the interface's language and never carries the backend's technical text.
import type { ClientStep, FailureReason, SessionState } from "@/bindings";
import type { AfkState } from "./types";

/**
 * Failed AFK commands no toast speaks of: a client that could not be had is shown on the account's
 * card (and in a notice while the AFK page is not in front, features/afk/watch.ts), a cancel is
 * the player's own doing.
 */
export function shownInPlace(error: unknown): boolean {
  const code =
    error && typeof error === "object" && "code" in error && typeof error.code === "string"
      ? error.code
      : null;
  return code === "AFK-ISTEMCI" || code === "IPTAL";
}

/** The download's percent, once its size is known. */
export function downloadPercent(done: number, total: number): number | undefined {
  if (!(total > 0)) return undefined;
  return Math.min(100, Math.max(0, (done / total) * 100));
}

/**
 * The account's state from its client step, or `null` when its session speaks: an account that
 * runs, connects or waits to reconnect is past the client (a step only belongs to an idle one).
 */
export function clientState(
  step: ClientStep | null | undefined,
  session: SessionState["state"],
): AfkState | null {
  if (!step || (session !== "stopped" && session !== "error")) return null;
  switch (step.step) {
    case "failed":
      return { kind: "clientFailed", problem: step.problem };
    case "downloading":
      return {
        kind: "preparing",
        step: "downloading",
        percent: downloadPercent(step.done, step.total),
      };
    default:
      return { kind: "preparing", step: step.step };
  }
}

/** Drops with words of their own (`afk.failure.*`). */
const DROPS = new Set<FailureReason["kind"]>([
  "kicked",
  "loginRejected",
  "connectionLost",
  "loggedOut",
  "disconnected",
  "unsupportedVersion",
]);

/** Failures to start the client (`afkConnect.failure.*`): their own words, nothing else. */
const LAUNCH_FAILURES = new Set<FailureReason["kind"]>([
  "launchFailed",
  "clientMissing",
  "secretsUnavailable",
  "unexpectedExit",
]);

/**
 * Why an account stopped, with the server's (or the client's) own words in brackets. Kicks
 * reconnecting does not fix (`refused`) say why the account stopped (`afk.refusal.*`). A launch
 * failure never shows the backend's technical message. `standalone`: the words say it all, so the
 * state line drops "Bağlantı kesildi" before them.
 */
export function failureText(
  reason: FailureReason | null | undefined,
  t: (key: string) => string,
): { reason: string; standalone: boolean } {
  if (!reason) return { reason: t("afk.failure.disconnected"), standalone: true };
  if (LAUNCH_FAILURES.has(reason.kind)) {
    return { reason: t(`afkConnect.failure.${reason.kind}`), standalone: true };
  }
  let base: string;
  let standalone = false;
  if (reason.kind === "refused") {
    base = t(`afk.refusal.${reason.cause}`);
  } else if (reason.kind === "failed") {
    base = t("afkConnect.failure.failed");
    standalone = true;
  } else if (DROPS.has(reason.kind)) {
    base = t(`afk.failure.${reason.kind}`);
    standalone = reason.kind === "disconnected";
  } else {
    base = t("afk.failure.disconnected");
    standalone = true;
  }
  const message = "message" in reason ? reason.message : null;
  return { reason: message ? `${base} (${message})` : base, standalone };
}
