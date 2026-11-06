// The copy of "Ekip bul"'s listing this computer keeps (CREW-DISCOVER). Asked for only when the
// page opens: kept until the next wave it names, asked again with its ETag (a quick "unchanged")
// once it is a quarter of an hour old. Nothing polls. A card whose crew left the listing meanwhile
// (`gone` from a join) goes from the kept copy at once.
import { toast } from "@pl/ui";
import { create } from "zustand";
import { discoverApi } from "@/lib/api/discover-api";
import i18n from "@/lib/i18n";
import { freshness, type KeptListing } from "./model";

export type ListingState = "idle" | "loading" | "ready" | "error" | "off";

interface ListingStore {
  kept: KeptListing | null;
  state: ListingState;
}

export const useListing = create<ListingStore>(() => ({ kept: null, state: "idle" }));

/** The command's refusal while Studio's switch is off (commands/social_discover.rs). */
const OFF_CODE = "SOSYAL-KESIF-KAPALI";

function isOff(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    (error as { code: unknown }).code === OFF_CODE
  );
}

/**
 * For `attempt`: Studio's switch went off while the page was open. Not the player's failure, so a
 * calm notice (no error mark), and the page says it is off; the next opening asks again.
 */
export function discoverOff(error: unknown): boolean {
  if (!isOff(error)) return false;
  toast(i18n.t("discover.off"));
  useListing.setState({ kept: null, state: "off" });
  return true;
}

let inFlight: Promise<void> | null = null;

/** Brings the kept copy up to date as `freshness` says (`force`: "Tekrar dene"). */
export function loadListing(force = false, nowMs = Date.now()): Promise<void> {
  const { kept } = useListing.getState();
  const mode = force ? "fetch" : freshness(kept, nowMs);
  if (mode === "keep") return Promise.resolve();
  if (inFlight) return inFlight;
  if (!kept) useListing.setState({ state: "loading" });
  inFlight = (async () => {
    try {
      const page = await discoverApi.list(kept?.etag ?? null);
      const fetchedAt = Date.now();
      if (page) {
        useListing.setState({
          kept: { listing: page.listing, etag: page.etag, fetchedAt },
          state: "ready",
        });
      } else if (kept) {
        useListing.setState({ kept: { ...kept, fetchedAt }, state: "ready" });
      }
    } catch (error) {
      useListing.setState({ state: isOff(error) ? "off" : kept ? "ready" : "error" });
    } finally {
      inFlight = null;
    }
  })();
  return inFlight;
}

/** A crew left the listing (a join answered `gone`): its card goes from the kept copy. */
export function dropFromListing(teamId: string): void {
  const { kept } = useListing.getState();
  if (!kept) return;
  useListing.setState({
    kept: {
      ...kept,
      listing: {
        ...kept.listing,
        crews: kept.listing.crews.filter((crew) => crew.id !== teamId),
      },
    },
  });
}
