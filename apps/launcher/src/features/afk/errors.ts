// Failed AFK commands: one short sentence in a toast (the backend's own when it sent one, else the
// caller's), with the technical details one click away for support.
import { toast } from "@pl/ui";
import { copyErrorDetails } from "@/features/content/jobs";
import i18n from "@/lib/i18n";

/** A command error the backend worded for the player (it carries `details` for support). */
export function isFriendly(error: unknown): error is Error & { details: string } {
  return error instanceof Error && "details" in error && typeof error.details === "string";
}

export function afkFailed(error: unknown, fallback: string) {
  toast.error(isFriendly(error) ? error.message : fallback, {
    action: {
      label: i18n.t("common.copyDetails"),
      onClick: () => void copyErrorDetails(error),
    },
  });
}

/** Runs an AFK command in the background; a failure becomes the toast above. */
export function runAfk(work: Promise<unknown>, fallback: string) {
  work.catch((error: unknown) => afkFailed(error, fallback));
}
