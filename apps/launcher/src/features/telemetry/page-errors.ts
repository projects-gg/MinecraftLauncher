// The page's uncaught errors for usage data (TELEMETRY): an error or a rejection nothing caught
// goes to the backend, which keeps only its scrubbed shape while usage data is on (and nothing
// while it is off). Expected ones are left out: a page change that skipped an older view
// transition (`@pl/ui` observes those), the browser's ResizeObserver notice, a cancelled fetch.
// At most a few per page load, each text once: an error thrown in a loop is one problem.
import { telemetryApi } from "./api";

/** Reports one page load sends at most. */
export const PAGE_ERRORS_PER_LOAD = 5;

/** What the backend gets: the text and where in our scripts it came from. */
export interface PageError {
  message: string;
  source: string;
  line: number;
  column: number;
}

const EXPECTED = [
  /^AbortError\b/,
  /Transition was skipped/,
  /^ResizeObserver loop/,
  /^Script error\.?$/,
];

function text(value: unknown): string {
  if (value instanceof Error) return `${value.name}: ${value.message}`;
  if (typeof value === "string") return value;
  return typeof value === "object" && value !== null ? "Non-Error object" : String(value);
}

/** The first frame of a stack in our own scripts: `(url, line, column)`. */
function firstFrame(stack: string | undefined): { source: string; line: number; column: number } {
  const match = stack?.match(/((?:https?|tauri|asset):\/\/[^\s)]+):(\d+):(\d+)/);
  if (!match) return { source: "", line: 0, column: 0 };
  return { source: match[1] ?? "", line: Number(match[2]), column: Number(match[3]) };
}

/** An `error` event as the backend takes it, or `null` for an expected one. */
export function fromErrorEvent(event: {
  message?: string;
  error?: unknown;
  filename?: string;
  lineno?: number;
  colno?: number;
}): PageError | null {
  const message = event.error === undefined ? (event.message ?? "") : text(event.error);
  if (!message || EXPECTED.some((pattern) => pattern.test(message))) return null;
  return {
    message,
    source: event.filename ?? "",
    line: event.lineno ?? 0,
    column: event.colno ?? 0,
  };
}

/** An unhandled rejection as the backend takes it, or `null` for an expected one. */
export function fromRejection(reason: unknown): PageError | null {
  // A command's refusal ({ code, message, details }) is the page's to show, not a fault.
  if (typeof reason === "object" && reason !== null && "code" in reason && "details" in reason) {
    return null;
  }
  const message = text(reason);
  if (!message || EXPECTED.some((pattern) => pattern.test(message))) return null;
  const frame = firstFrame(reason instanceof Error ? reason.stack : undefined);
  return { message, ...frame };
}

/** Keeps the first few distinct reports of one page load. */
export function reportLimiter(max = PAGE_ERRORS_PER_LOAD): (error: PageError) => boolean {
  const seen = new Set<string>();
  return (error) => {
    if (seen.size >= max || seen.has(error.message)) return false;
    seen.add(error.message);
    return true;
  };
}

/** Listens for the page's uncaught errors (main.tsx, once, in the app only). */
export function watchPageErrors(target: Window = window): void {
  const allow = reportLimiter();
  const send = (error: PageError | null) => {
    if (error === null || !allow(error)) return;
    void telemetryApi
      .pageError(error.message, error.source, error.line, error.column)
      .catch(() => {});
  };
  target.addEventListener("error", (event) => send(fromErrorEvent(event)));
  target.addEventListener("unhandledrejection", (event) => send(fromRejection(event.reason)));
}
