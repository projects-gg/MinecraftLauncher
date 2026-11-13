// "Sunucu Hesapların" (SRV-ACC): the account center's one calm row for the player's accounts on the
// Projects server's site that the launcher does not have yet, like the rows beside it (56 px, the
// same ring). Shown only while signed in to the friends feature, the signed config's
// `features.serverAccounts` is on and the list has a player name the launcher lacks; never at the
// launcher's start (asked when the center opens, at most every five minutes), never a popup.
// "Hepsini Ekle" adds every name as a username account (the active account stays); the row then
// says "Eklendi" with "Geri Al" for a few seconds (held while pointed at or focused) and goes.
// "Şimdi Değil" keeps it quiet until the list changes. The two states share one cell each side and
// cross-fade (opacity only), so nothing beside them moves.
import { UserList } from "@phosphor-icons/react";
import { Button, cn, easeOutExpo, exitOf, springSnappy, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import {
  type ReactNode,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { ACCOUNTS_KEY, announce, friendlyError } from "@/features/accounts/use-accounts";
import { useSocialUi } from "@/features/social/store";
import { type AccountSummary, api } from "@/lib/api";
import { ADDED_MS, fitNames, listDigest, missingNames, SERVER_ACCOUNTS_STALE_MS } from "./model";
import { useServerAccountsPrefs } from "./store";

/** No list yet (one array for every render, so nothing reacts to a new empty one). */
const NO_NAMES: readonly string[] = [];

/** The signed config's "Sunucu Hesapları" (with the friends feature), from the home content. */
function useServerAccountsOn(): boolean {
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  return Boolean(data?.serverAccounts);
}

/** The list for the signed-in player; asked only while `enabled`, at most every five minutes. */
function useServerAccounts(meId: string | null, enabled: boolean) {
  return useQuery({
    queryKey: ["server-accounts", meId ?? ""],
    queryFn: () => api.serverAccounts(),
    enabled: enabled && meId !== null,
    staleTime: SERVER_ACCOUNTS_STALE_MS,
    gcTime: 2 * SERVER_ACCOUNTS_STALE_MS,
    retry: false,
  });
}

/** The first control after `element` that takes focus (the row is about to go). */
function nextFocusable(element: HTMLElement): HTMLElement | null {
  const scope = element.closest<HTMLElement>("[role=dialog]") ?? document.body;
  const all = scope.querySelectorAll<HTMLElement>(
    "button:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex='-1'])",
  );
  // A control after the row that is neither inside it nor around it compares as just "following".
  for (const candidate of all) {
    if (element.compareDocumentPosition(candidate) === Node.DOCUMENT_POSITION_FOLLOWING) {
      return candidate;
    }
  }
  return null;
}

/** Text measured as `element` draws it (its font), for fitting the names line. */
function measureWith(element: HTMLElement): (text: string) => number {
  const context = document.createElement("canvas").getContext("2d");
  if (!context) return (text) => text.length * 7;
  const style = getComputedStyle(element);
  context.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
  return (text) => context.measureText(text).width;
}

/** The names that fit the line, with "+N" for the rest. */
function NamesLine({ names }: { names: readonly string[] }) {
  const { t } = useTranslation();
  const ref = useRef<HTMLSpanElement>(null);
  const [fit, setFit] = useState(() => ({ shown: [...names], more: 0 }));
  useLayoutEffect(() => {
    const line = ref.current;
    if (!line) return;
    const width = measureWith(line);
    const more = (count: number) => t("serverAccounts.more", { count });
    const place = () => {
      const next = fitNames(names, line.clientWidth, width, more);
      setFit((now) =>
        now.more === next.more && now.shown.join("\n") === next.shown.join("\n") ? now : next,
      );
    };
    place();
    const observer = new ResizeObserver(place);
    observer.observe(line);
    return () => observer.disconnect();
  }, [names, t]);
  return (
    <span
      ref={ref}
      className="block truncate text-[12.5px] text-ash-400"
      title={fit.more > 0 ? names.join(", ") : undefined}
      data-selectable
    >
      {/* Every name for screen readers; the line shows what fits. */}
      <span className="sr-only">{names.join(", ")}</span>
      <span aria-hidden>
        {fit.shown.join(", ")}
        {fit.more > 0 ? ` ${t("serverAccounts.more", { count: fit.more })}` : null}
      </span>
    </span>
  );
}

/** One of two stacked states: the other one's place, faded out and out of reach. */
function Layer({
  on,
  className,
  children,
}: {
  on: boolean;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      aria-hidden={!on}
      inert={!on}
      className={cn(
        "col-start-1 row-start-1 transition-opacity duration-200 ease-out",
        on ? "opacity-100" : "pointer-events-none opacity-0",
        className,
      )}
    >
      {children}
    </div>
  );
}

export function ServerAccountsRow({ accounts }: { accounts: readonly AccountSummary[] }) {
  const { t } = useTranslation();
  const calm = useCalm();
  const queryClient = useQueryClient();
  const view = useSocialUi((s) => s.view);
  const on = useServerAccountsOn();
  const me = view.enabled && view.auth === "signedIn" ? (view.me ?? null) : null;
  const { data: list } = useServerAccounts(me?.id ?? null, on);
  const dismissed = useServerAccountsPrefs((s) => s.dismissed);
  const dismiss = useServerAccountsPrefs((s) => s.dismiss);
  const names = list ?? NO_NAMES;
  const missing = useMemo(() => missingNames(names, accounts), [names, accounts]);
  const digest = useMemo(() => listDigest(names), [names]);
  // The accounts the last press added, while "Eklendi" shows.
  const [added, setAdded] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  // "Eklendi" waits while the row is pointed at, or holds the keyboard's focus.
  const [hovered, setHovered] = useState(false);
  const [keyboard, setKeyboard] = useState(false);
  const held = hovered || keyboard;
  const rowRef = useRef<HTMLFieldSetElement>(null);
  const addRef = useRef<HTMLButtonElement>(null);
  const undoRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (added === null || held) return;
    const timer = window.setTimeout(() => {
      // The row goes: focus left on "Geri Al" moves on instead of falling to the page.
      const row = rowRef.current;
      if (row?.contains(document.activeElement)) nextFocusable(row)?.focus({ preventScroll: true });
      setAdded(null);
    }, ADDED_MS);
    return () => window.clearTimeout(timer);
  }, [added, held]);

  const offered = on && me !== null && missing.length > 0 && dismissed !== digest;
  const shown = added !== null || offered;
  const titleId = useId();

  const addAll = async () => {
    if (busy || missing.length === 0) return;
    setBusy(true);
    const focused = rowRef.current?.contains(document.activeElement) ?? false;
    try {
      const made = await api.addOfflineAccounts(missing);
      queryClient.setQueryData<AccountSummary[]>(ACCOUNTS_KEY, (current) =>
        current
          ? [...current, ...made.filter((a) => !current.some((c) => c.id === a.id))]
          : current,
      );
      // Every name was added meanwhile some other way: the row simply goes.
      if (made.length === 0) return;
      setAdded(made.map((account) => account.id));
      announce(t("serverAccounts.addedAnnounce", { count: made.length }));
      if (focused) requestAnimationFrame(() => undoRef.current?.focus());
    } catch (error) {
      toast.error(friendlyError(error, t("serverAccounts.addFailed")).message);
    } finally {
      setBusy(false);
      void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
    }
  };

  const undo = async () => {
    if (busy || added === null) return;
    setBusy(true);
    const focused = rowRef.current?.contains(document.activeElement) ?? false;
    try {
      const { activeId } = await api.removeAddedAccounts(added);
      queryClient.setQueryData<AccountSummary[]>(ACCOUNTS_KEY, (current) =>
        current
          ?.filter((account) => !added.includes(account.id))
          .map((account) => ({ ...account, active: account.id === activeId })),
      );
      setAdded(null);
      announce(t("serverAccounts.undoneAnnounce"));
      if (focused) requestAnimationFrame(() => addRef.current?.focus());
    } catch (error) {
      toast.error(friendlyError(error, t("serverAccounts.undoFailed")).message);
    } finally {
      setBusy(false);
      void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
    }
  };

  const notNow = () => {
    const row = rowRef.current;
    if (row?.contains(document.activeElement)) nextFocusable(row)?.focus({ preventScroll: true });
    dismiss(digest);
  };

  const done = added !== null;
  return (
    // The row's exit leaves the flow at once, so the rows below glide up (their LayoutGroup).
    <AnimatePresence initial={false} mode="popLayout">
      {shown ? (
        <motion.div
          key="server-accounts"
          className="mb-3"
          layout="position"
          transition={{ layout: springSnappy }}
          initial={calm({ opacity: 0, y: 6 })}
          animate={calm({ opacity: 1, y: 0, transition: { duration: 0.3, ease: easeOutExpo } })}
          exit={calm({ opacity: 0, transition: { duration: exitOf(0.2) } })}
        >
          <fieldset
            ref={rowRef}
            aria-labelledby={titleId}
            aria-busy={busy}
            onPointerEnter={() => setHovered(true)}
            onPointerLeave={() => setHovered(false)}
            onFocus={(event) => setKeyboard(event.target.matches(":focus-visible"))}
            onBlur={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
                setKeyboard(false);
              }
            }}
            className="flex min-h-14 w-full min-w-0 items-center gap-3 rounded-[var(--radius-panel)] px-3.5 py-2.5 ring-1 ring-white/[0.07]"
          >
            <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-white/[0.05] text-ash-300">
              <UserList size={19} aria-hidden />
            </span>
            <div className="min-w-0 flex-1">
              <span id={titleId} className="block text-[14px] font-medium text-bone-50">
                {t("serverAccounts.title")}
              </span>
              <div className="grid grid-cols-[minmax(0,1fr)]">
                <Layer on={!done} className="min-w-0">
                  <NamesLine names={missing.length > 0 ? missing : names} />
                </Layer>
                <Layer on={done}>
                  <span className="block text-[12.5px] text-jade-400">
                    {t("serverAccounts.added")}
                  </span>
                </Layer>
              </div>
            </div>
            <div className="grid shrink-0 justify-items-end">
              <Layer on={!done} className="flex items-center gap-1.5">
                <Button ref={addRef} size="sm" variant="secondary" onClick={() => void addAll()}>
                  {missing.length === 1 ? t("serverAccounts.addOne") : t("serverAccounts.addAll")}
                </Button>
                <Button size="sm" variant="ghost" onClick={notNow}>
                  {t("serverAccounts.notNow")}
                </Button>
              </Layer>
              <Layer on={done}>
                <Button ref={undoRef} size="sm" variant="ghost" onClick={() => void undo()}>
                  {t("serverAccounts.undo")}
                </Button>
              </Layer>
            </div>
          </fieldset>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
