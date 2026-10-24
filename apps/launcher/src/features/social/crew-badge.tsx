// A crew tag before a player's name (TEAM6 §3, §4): the tag's chip and the card ("baloncuk") it
// opens: the crew's mark, name, tag, motto, founding month and size, and one row of what the player
// may do (open the crew, take their own tag off, take an invite, ask to join from a friend's tag,
// withdraw a request), the crew's menu or "Ekibi rapor et" behind its "…". Opening a card asks
// nothing of the service; only "Katılma isteği gönder" does.
//
// In a row the chip is part of the row's button: the mouse resting on it opens the card, a press
// goes to the row (whose own card carries the chip as a button). A standalone chip (a person
// card's title, the player's own card, a conversation's header) is a button: the mouse or the
// keyboard's focus opens its card as a preview, a press or Enter keeps it open (Enter moves the
// focus into it), Esc closes it and the focus comes back; a right click is the card's menu. One
// intent for every chip (badge-intent.ts): one card at a time, chip to chip at once, and nothing
// runs while nobody points at a chip. Names with a tag are drawn here (`BadgedName`), once for every
// place: the chip fades and grows in where it goes, the name glides to its new place.
import {
  CalendarBlank,
  Check,
  Clock,
  DotsThree,
  Flag,
  TagSimple,
  UsersThree,
} from "@phosphor-icons/react";
import {
  Button,
  ContextMenu,
  cn,
  easeOutExpo,
  focusPanel,
  IconButton,
  Menu,
  type MenuItem,
  Tooltip,
} from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { Popover } from "radix-ui";
import {
  type ReactElement,
  type ReactNode,
  type PointerEvent as ReactPointerEvent,
  type SyntheticEvent,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { CrewBadge, SocialTeam } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { attempt } from "./actions";
import { askFromBadge, openCrewPage, sayJoined, useBadgeRefusals, wearTag } from "./badge-actions";
import {
  blurChip,
  closeCard,
  closedWithFocus,
  enterCard,
  focusChip,
  holdCard,
  hoverChip,
  leaveCard,
  leaveChip,
  pressChip,
  registerCard,
  useBadgeIntent,
} from "./badge-intent";
import { type CardRow, cardMenu, cardRow, refusedNow, rowsReachable, wearState } from "./badges";
import { stillWaits } from "./crew-joining";
import { canAddCrew } from "./crews";
import { dayMonth } from "./limits";
import { openMenuFromKey } from "./menu-key";
import { badgeMenu, rowsOf } from "./menus";
import { useSocialMotion } from "./motion";
import { useSocialUi } from "./store";
import { CrewMark, CrewWatermark, type TagSize, TeamTag } from "./team-emblem";
import { useCrewItems } from "./team-section";
import { localDay } from "./teams";

/** How a row names someone who wears a tag: "Ayşe, etiket GK" (the chip itself is not read). */
export function badgeRowLabel(
  name: string,
  badge: CrewBadge | undefined,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  return badge ? t("team6.rowLabel", { name, tag: badge.tag }) : name;
}

/** The tag a profile shows: none while the service offers no tags (or Studio turned them off). */
export function useShownBadge(badge: CrewBadge | undefined): CrewBadge | undefined {
  const on = useSocialUi((s) => s.view.crew.badges);
  return on ? badge : undefined;
}

const TABBABLE =
  'button:not([disabled]):not([data-badge-chip]), [href], input:not([disabled]), [tabindex]:not([tabindex="-1"]):not([data-badge-chip])';

/**
 * A person card's first focus (`onOpenAutoFocus`): its first action, as before tags, not the tag
 * before the name (Shift+Tab reaches it), and without the focus ring after a pointer press.
 */
export function focusFirstAction(event: Event) {
  const content = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
  const first = content?.querySelector<HTMLElement>(TABBABLE);
  if (!first) return;
  event.preventDefault();
  focusPanel(first);
}

// --- the card's motion ------------------------------------------------------------------------------

/**
 * Popovers' motion (docs/design.md §5): 160 ms from 96 % at the chip, 110 ms out to 98 %; under
 * reduced motion a fade only. A card replaced by the next chip's leaves at once (`custom`).
 */
function cardVariants(reduce: boolean) {
  return {
    hidden: reduce ? { opacity: 0 } : { opacity: 0, transform: "scale(0.96)" },
    shown: {
      opacity: 1,
      ...(reduce ? {} : { transform: "scale(1)" }),
      transition: { duration: 0.16, ease: easeOutExpo },
    },
    gone: (swapped: boolean) =>
      swapped
        ? { opacity: 0, transition: { duration: 0 } }
        : {
            opacity: 0,
            ...(reduce ? {} : { transform: "scale(0.98)" }),
            transition: { duration: 0.11, ease: easeOutExpo },
          },
  };
}

/** A card sits in a portal but in its chip's React tree: none of its presses, keys or right
 * clicks may reach the row around the chip (its card, its menu, its drag). */
const stop = (event: SyntheticEvent) => event.stopPropagation();
const KEEP_INSIDE = {
  onClick: stop,
  onDoubleClick: stop,
  onPointerDown: stop,
  onPointerUp: stop,
  onMouseDown: stop,
  onKeyDown: stop,
  onContextMenu: stop,
};

// --- the chip ----------------------------------------------------------------------------------------

export interface BadgeChipProps {
  badge: CrewBadge;
  /** Who wears it (their id). */
  wearer: string;
  size?: TagSize;
  /** `row`: part of a row's button (the mouse opens the card); `button`: a standalone chip; `still`:
   * the tag alone (the crew's own pickers, a dragged row's picture). */
  mode?: "row" | "button" | "still";
  className?: string;
}

export function BadgeChip({ badge, wearer, size = "md", mode = "row", className }: BadgeChipProps) {
  const { t } = useTranslation();
  const id = useId();
  const { reduce } = useSocialMotion();
  const status = useBadgeIntent((s) =>
    s.open === id ? (s.instant ? "instant" : "open") : s.open === null ? "closed" : "other",
  );
  const open = status === "open" || status === "instant";
  const triggerRef = useRef<HTMLButtonElement>(null);
  const anchorRef = useRef<HTMLSpanElement>(null);

  const tag = (
    <TeamTag
      tag={badge.tag}
      color={badge.color}
      size={size}
      className={cn(
        // Over the focus ring and its backing (a standalone chip's).
        "relative transition-[filter] duration-150",
        open ? "brightness-125" : mode === "button" && "group-hover/chip:brightness-125",
      )}
    />
  );
  if (mode === "still") {
    return (
      <span aria-hidden className={cn("inline-flex shrink-0", className)}>
        {tag}
      </span>
    );
  }

  // Only a mouse hovers: a touch is a press (and in a row, the row's).
  const mouse = {
    onPointerEnter: (event: ReactPointerEvent<HTMLElement>) => {
      if (event.pointerType === "mouse") hoverChip(id);
    },
    onPointerLeave: (event: ReactPointerEvent<HTMLElement>) => {
      if (event.pointerType === "mouse") leaveChip(id, { x: event.clientX, y: event.clientY });
    },
  };

  return (
    <Popover.Root
      open={open}
      onOpenChange={(next) => {
        if (!next) closeCard(id);
      }}
    >
      {mode === "button" ? (
        <BadgeMenu badge={badge} wearer={wearer} chip={id} as="context">
          <Popover.Trigger asChild>
            <button
              ref={triggerRef}
              type="button"
              data-badge-chip
              aria-label={t("team6.chipLabel", { crew: badge.name })}
              className={cn(
                "group/chip relative inline-flex shrink-0 cursor-pointer outline-none transition-[transform,scale] duration-150 ease-out active:scale-[0.94]",
                className,
              )}
              onClick={(event) => {
                // The card answers the press itself (a preview gets pinned, not closed).
                event.preventDefault();
                event.stopPropagation();
                pressChip(id, event.detail === 0);
              }}
              onKeyDown={(event) => {
                openMenuFromKey(event);
              }}
              onFocus={(event) => {
                if (event.currentTarget.matches(":focus-visible")) focusChip(id);
              }}
              onBlur={() => blurChip(id)}
              onContextMenu={() => closeCard(id)}
              {...mouse}
            >
              <span
                aria-hidden
                className="chamfer absolute -inset-[2px] bg-gold-300 opacity-0 transition-opacity duration-150 [--cut:6px] group-focus-visible/chip:opacity-100"
              />
              {/* The tint is see-through: a solid backing keeps the ring a ring. */}
              <span
                aria-hidden
                className={cn(
                  "chamfer absolute inset-0 bg-obsidian-900",
                  size === "sm" ? "[--cut:4px]" : "[--cut:5px]",
                )}
              />
              {tag}
            </button>
          </Popover.Trigger>
        </BadgeMenu>
      ) : (
        <Popover.Anchor asChild>
          <span
            ref={anchorRef}
            aria-hidden
            data-badge-chip
            className={cn("inline-flex shrink-0", className)}
            {...mouse}
          >
            {tag}
          </span>
        </Popover.Anchor>
      )}
      <AnimatePresence custom={status === "other"}>
        {open ? (
          <Popover.Portal forceMount>
            <Popover.Content
              forceMount
              asChild
              side="bottom"
              align="start"
              sideOffset={6}
              collisionPadding={12}
              aria-label={badge.name}
              // The focus moves only after keys (the card does it); a preview leaves it alone.
              onOpenAutoFocus={(event) => event.preventDefault()}
              onCloseAutoFocus={(event) => {
                event.preventDefault();
                if (!closedWithFocus(id)) return;
                const back =
                  mode === "button"
                    ? triggerRef.current
                    : anchorRef.current?.closest<HTMLElement>("button, [tabindex]");
                if (back?.isConnected) back.focus({ preventScroll: true });
              }}
            >
              <motion.div
                ref={(element: HTMLDivElement | null) => registerCard(id, element)}
                data-badge-card
                className="z-50 w-[264px] overflow-hidden rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-4 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:bg-obsidian-900 [html[data-perf=on]_&]:backdrop-blur-none"
                style={{ transformOrigin: "var(--radix-popover-content-transform-origin)" }}
                variants={cardVariants(reduce)}
                // Leaving, AnimatePresence's `custom` says whether the next chip's card replaced it.
                custom={false}
                initial={status === "instant" ? false : "hidden"}
                animate="shown"
                exit="gone"
                onPointerEnter={(event) => {
                  if (event.pointerType === "mouse") enterCard(id);
                }}
                onPointerLeave={(event) => {
                  if (event.pointerType === "mouse") leaveCard(id);
                }}
                {...KEEP_INSIDE}
              >
                <BadgeCard badge={badge} wearer={wearer} chip={id} />
              </motion.div>
            </Popover.Content>
          </Popover.Portal>
        ) : null}
      </AnimatePresence>
    </Popover.Root>
  );
}

// --- the card ----------------------------------------------------------------------------------------

type Cell = Exclude<CardRow, null> | "refused";

function BadgeCard({ badge, wearer, chip }: { badge: CrewBadge; wearer: string; chip: string }) {
  const { t } = useTranslation();
  const view = useSocialUi((s) => s.view);
  const refusals = useBadgeRefusals((s) => s.until);
  const [sent, setSent] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const rowRef = useRef<HTMLDivElement>(null);
  const refocus = useRef(false);
  const nowMs = Date.now();
  const team = view.crews.find((crew) => crew.id === badge.crew);
  const waiting = view.teamRequests
    .filter((request) => stillWaits(request, nowMs))
    .map((request) => request.teamId);
  const computed = cardRow({
    badge,
    own: wearer === view.me?.id,
    friend: view.friends.some((friend) => friend.id === wearer),
    crews: view.crews.map((crew) => crew.id),
    crewLimit: view.crewLimit,
    multi: view.crew.multi,
    invites: view.teamInvites.map((invite) => invite.teamId),
    // A request just sent waits until the list has it.
    waiting: sent && !waiting.includes(badge.crew) ? [...waiting, badge.crew] : waiting,
    requestsOn: view.crew.badgeRequests,
    refusedToday: refusedNow(refusals, badge.crew, nowMs),
  });
  const row: Cell | null = refused !== null ? "refused" : computed;
  // Every row the bottom line may turn into shares one cell: the card keeps its size.
  const [cells, setCells] = useState<Cell[]>(() => rowsReachable(computed));
  if (row && !cells.includes(row)) setCells([...cells, row]);
  const full = !canAddCrew(view.crews, view.crewLimit, view.crew.multi);

  // Enter on the chip: the focus goes to the row's first action, else to the card.
  const asked = useBadgeIntent((s) => (s.focus?.id === chip ? s.focus.n : 0));
  const handled = useRef(0);
  useEffect(() => {
    if (asked <= handled.current) return;
    handled.current = asked;
    focusRow(rootRef.current, true);
  }, [asked]);

  // A row that turned into another while it had the focus hands it to the new one (unless the
  // player took the focus somewhere else meanwhile).
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the row changes
  useEffect(() => {
    if (!refocus.current) return;
    refocus.current = false;
    const active = document.activeElement;
    if (active && active !== document.body && !rowRef.current?.contains(active)) return;
    focusRow(rootRef.current, false);
  }, [row]);

  const run = async (work: () => Promise<void>) => {
    if (busy) return;
    refocus.current = Boolean(rowRef.current?.contains(document.activeElement));
    setBusy(true);
    holdCard(chip, true);
    try {
      await work();
    } finally {
      setBusy(false);
      holdCard(chip, false);
      // The row turns with the answer or with the list right after it; not much later.
      window.setTimeout(() => {
        refocus.current = false;
      }, 1500);
    }
  };
  const openCrew = () => {
    closeCard(chip);
    openCrewPage(badge.crew);
  };
  const takeOff = () => {
    closeCard(chip);
    void wearTag(null);
  };
  const accept = () =>
    run(async () => {
      const done = await attempt(() => api.social.acceptTeamInvite(badge.crew));
      if (done !== undefined) sayJoined(t("teams.joined", { name: badge.name }), () => badge.crew);
    });
  const request = () =>
    run(async () => {
      const outcome = await askFromBadge(badge.crew, wearer);
      if (outcome.kind === "sent") setSent(true);
      else if (outcome.kind === "refused") setRefused(outcome.text);
    });
  const withdraw = () =>
    run(async () => {
      const done = await attempt(() => api.social.withdrawTeamRequest(badge.crew));
      if (done === undefined) return;
      setSent(false);
      announce(t("crewWave2.join.withdrawn"));
    });

  const founded = localDay(badge.createdOn);
  const members = team
    ? t("teams.membersOf", { count: team.members.length, limit: team.limit })
    : t("teams.members", { count: badge.members });

  const cell = (kind: Cell): ReactNode => {
    switch (kind) {
      case "own":
        return (
          <>
            <Button
              size="sm"
              variant="ghost"
              className="-ml-2"
              icon={<TagSimple size={14} />}
              onClick={takeOff}
            >
              {t("team6.takeOff")}
            </Button>
            <Button size="sm" variant="secondary" className="ml-auto" onClick={openCrew}>
              {t("teams.open")}
            </Button>
          </>
        );
      case "member":
        return (
          <>
            <span className="min-w-0 flex-1 text-[12.5px] leading-snug text-ash-300">
              {t("team6.card.member")}
            </span>
            <Button size="sm" variant="secondary" onClick={openCrew}>
              {t("teams.open")}
            </Button>
          </>
        );
      case "invited":
        return (
          <>
            <span className="min-w-0 flex-1 text-[12.5px] leading-snug font-medium text-gold-200">
              {t("team6.card.invited")}
            </span>
            <Tooltip content={t("crews.limit", { count: view.crewLimit })} disabled={!full}>
              <span className="inline-flex">
                <Button
                  size="sm"
                  variant="secondary"
                  icon={<Check size={14} weight="bold" />}
                  disabled={busy || full}
                  onClick={() => void accept()}
                >
                  {t("teams.accept")}
                </Button>
              </span>
            </Tooltip>
          </>
        );
      case "waiting":
        return (
          <>
            <span className="flex min-w-0 flex-1 items-center gap-1.5 text-[12.5px] font-medium text-gold-200">
              <Clock size={14} weight="bold" className="shrink-0" aria-hidden />
              <span className="truncate">{t("crewWave2.join.waiting")}</span>
            </span>
            <Button
              size="sm"
              variant="ghost"
              className="-mr-2"
              disabled={busy}
              onClick={() => void withdraw()}
            >
              {t("crewWave2.join.withdraw")}
            </Button>
          </>
        );
      case "ask":
        // Pressed in (97 %) until the service answers; no question first (it can be withdrawn).
        return (
          <Button
            size="sm"
            variant="primary"
            className={cn("w-full", busy && "scale-[0.97]")}
            aria-busy={busy || undefined}
            onClick={() => void request()}
          >
            {t("crewWave2.join.ask")}
          </Button>
        );
      case "refused":
        return <p className="text-[12.5px] leading-snug text-ash-300">{refused}</p>;
    }
  };

  return (
    <div ref={rootRef} className="relative">
      <CrewWatermark look={badge} size={96} className="-top-7 -right-8" />
      <div className="relative flex items-start gap-3 pr-6">
        <CrewMark look={badge} size={44} />
        <div className="min-w-0 flex-1">
          <p className="font-display text-[16px] leading-tight font-semibold text-pretty break-words text-bone-50">
            {badge.name}
            <TeamTag tag={badge.tag} color={badge.color} className="ml-1.5 align-[1px]" />
          </p>
          {badge.motto ? (
            <p
              className="mt-1 line-clamp-2 text-[13px] leading-snug break-words text-ash-300"
              data-selectable
            >
              {badge.motto}
            </p>
          ) : null}
          <p className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[12px] text-ash-400">
            {founded ? (
              <span className="inline-flex items-center gap-1">
                <CalendarBlank size={13} aria-hidden />
                {t("teams.card.founded", {
                  date: dateTimeFormat({ month: "long", year: "numeric" }).format(founded),
                })}
              </span>
            ) : null}
            <span className="inline-flex items-center gap-1">
              <UsersThree size={13} aria-hidden />
              {members}
            </span>
          </p>
        </div>
      </div>
      {cells.length ? (
        <div className="relative mt-3 border-t border-white/7 pt-3">
          <div ref={rowRef} className="grid">
            {cells.map((kind) => {
              const active = kind === row;
              return (
                <div
                  key={kind}
                  data-badge-row={active ? "active" : undefined}
                  aria-hidden={active ? undefined : true}
                  inert={!active}
                  className={cn(
                    "flex min-h-8 items-center gap-2 transition-opacity [grid-area:1/1]",
                    active
                      ? "opacity-100 delay-[60ms] duration-150"
                      : "pointer-events-none opacity-0 duration-100",
                  )}
                >
                  {cell(kind)}
                </div>
              );
            })}
          </div>
        </div>
      ) : null}
      <div className="absolute -top-1.5 -right-2">
        <BadgeMenu badge={badge} wearer={wearer} chip={chip} as="more" />
      </div>
    </div>
  );
}

/** The focus to the active row's first action; else (`orCard`) to the card itself. */
function focusRow(inside: HTMLElement | null, orCard: boolean) {
  const action = inside?.querySelector<HTMLElement>(
    "[data-badge-row=active] button:not([disabled])",
  );
  if (action) {
    action.focus({ preventScroll: true });
    return;
  }
  if (orCard) inside?.closest<HTMLElement>("[data-badge-card]")?.focus({ preventScroll: true });
}

// --- the card's menu ---------------------------------------------------------------------------------

/** Menu rows that close the card before they do what they say (a dialog, the crew page…). */
function closing(items: Array<MenuItem | null>, chip: string): Array<MenuItem | null> {
  return items.map((item) =>
    item
      ? {
          ...item,
          onSelect: () => {
            closeCard(chip);
            item.onSelect();
          },
          items: item.items ? closing(item.items, chip) : undefined,
        }
      : null,
  );
}

interface MenuPlace {
  badge: CrewBadge;
  wearer: string;
  chip: string;
  /** `more`: the card's "…"; `context`: a standalone chip's right click (around `children`). */
  as: "more" | "context";
  children?: ReactElement;
}

/** The crew's whole menu for its members, "Ekibi rapor et" for everyone else (menus.ts). */
function BadgeMenu(props: MenuPlace) {
  const team = useSocialUi((s) => s.view.crews.find((crew) => crew.id === props.badge.crew));
  const row = team ? "member" : null;
  return cardMenu(row) === "crew" && team ? (
    <CrewBadgeMenu {...props} team={team} />
  ) : (
    <OutsiderBadgeMenu {...props} />
  );
}

function CrewBadgeMenu({ team, chip, as, children }: MenuPlace & { team: SocialTeam }) {
  const items = closing(useCrewItems(team, { onOpen: () => openCrewPage(team.id) }), chip);
  return (
    <MenuShell items={items} chip={chip} as={as} name={team.name}>
      {children}
    </MenuShell>
  );
}

function OutsiderBadgeMenu({ badge, wearer, chip, as, children }: MenuPlace) {
  const { t } = useTranslation();
  const items = closing(
    rowsOf(badgeMenu(), () => ({
      label: t("reports.crewAction"),
      icon: <Flag size={16} />,
      onSelect: () =>
        useSocialUi.getState().openReport({
          target: { kind: "crew", id: badge.crew, via: wearer },
          name: badge.name,
          crew: badge,
        }),
    })),
    chip,
  );
  return (
    <MenuShell items={items} chip={chip} as={as} name={badge.name}>
      {children}
    </MenuShell>
  );
}

function MenuShell({
  items,
  chip,
  as,
  name,
  children,
}: {
  items: Array<MenuItem | null>;
  chip: string;
  as: "more" | "context";
  name: string;
  children?: ReactElement;
}) {
  const { t } = useTranslation();
  if (as === "context" && children) return <ContextMenu items={items}>{children}</ContextMenu>;
  return (
    <Menu
      items={items}
      onOpenChange={(open) => holdCard(chip, open)}
      trigger={
        <IconButton
          size="sm"
          label={t("friendMenus.options", { name })}
          icon={<DotsThree size={18} weight="bold" />}
        />
      }
    />
  );
}

// --- wearing it, from the crew's own card ----------------------------------------------------------

/**
 * Beside the tag on the crew page's card (TEAM6 §3.2): puts the crew's tag on, or takes it off while
 * it is worn (`aria-pressed`; gold while worn). One square, so the crew's name keeps its room; its
 * words are its tooltip and its name. Soft, with the reason in the tooltip, while the crew cannot be
 * worn (its leader hid it, moderators locked its texts).
 */
export function WearToggle({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const on = useSocialUi((s) => s.view.crew.badges);
  const worn = useSocialUi((s) =>
    s.badgePending !== undefined ? s.badgePending : (s.view.me?.badge?.crew ?? null),
  );
  if (!on) return null;
  const state = wearState(team, worn, Date.now());
  const pressed = state === "worn";
  const why =
    state === "hidden"
      ? t("team6.hiddenWhy")
      : state === "locked"
        ? t("team6.lockedWhy", { date: dayMonth(team.textLockedUntil ?? 0) })
        : null;
  return (
    <Tooltip content={why ?? (pressed ? t("team6.worn") : t("team6.wear"))}>
      <span className="inline-flex">
        <button
          type="button"
          aria-pressed={pressed}
          aria-label={t("team6.wear")}
          disabled={why !== null}
          onClick={() => void wearTag(pressed ? null : team.id)}
          className="group/wear relative inline-flex size-[22px] shrink-0 cursor-pointer items-center justify-center outline-none transition-[opacity,transform,scale] duration-150 ease-out active:scale-[0.94] disabled:pointer-events-none disabled:opacity-45"
        >
          <span
            aria-hidden
            className="chamfer-ring absolute bg-gold-300 opacity-0 transition-opacity duration-150 [--cut:5px] group-focus-visible/wear:opacity-100"
          />
          <span
            aria-hidden
            className={cn(
              "chamfer absolute inset-0 transition-colors duration-150 [--cut:5px]",
              pressed
                ? "bg-gold-500/16 group-hover/wear:bg-gold-500/24"
                : "bg-obsidian-900 group-hover/wear:bg-obsidian-800 group-focus-visible/wear:bg-obsidian-800",
            )}
          />
          <span aria-hidden className="relative grid">
            <TagSimple
              size={14}
              className={cn(
                "text-ash-300 transition-opacity duration-150 [grid-area:1/1] group-hover/wear:text-bone-100",
                pressed ? "opacity-0" : "opacity-100",
              )}
            />
            <TagSimple
              size={14}
              weight="fill"
              className={cn(
                "text-gold-300 transition-opacity duration-150 [grid-area:1/1]",
                pressed ? "opacity-100" : "opacity-0",
              )}
            />
          </span>
        </button>
      </span>
    </Tooltip>
  );
}

// --- a name with its tag -----------------------------------------------------------------------------

/**
 * `[GK] Ayşe`: a player's name with the tag they wear before it (none while they wear none or the
 * service has no tags), then `children` (a favourite's star, a role's crown). The chip never
 * shrinks; the name truncates. A tag that comes or goes live fades and grows in its place (160 ms;
 * out in 110 ms) and the name glides to where it now starts (200 ms); reduced motion and
 * Performance mode keep the fade and the name simply moves.
 */
export function BadgedName({
  name,
  badge,
  wearer,
  size = "md",
  chip = "row",
  dim = false,
  className,
  nameClassName,
  children,
}: {
  name: string;
  badge: CrewBadge | undefined;
  wearer: string;
  size?: TagSize;
  chip?: "row" | "button" | "still";
  /** The chip quieter, with a dimmed name (someone offline). */
  dim?: boolean;
  className?: string;
  nameClassName?: string;
  children?: ReactNode;
}) {
  const shown = useShownBadge(badge);
  const { reduce, layout } = useSocialMotion();
  const rootRef = useRef<HTMLSpanElement>(null);
  // The chip on screen now: a leaving one must not take the new one's place when it goes.
  const chipRef = useRef<HTMLSpanElement | null>(null);
  const nameRef = useRef<HTMLSpanElement>(null);
  // How far the name stands from the row's start because of the chip (its width and the gap).
  const shift = useRef<number | null>(null);
  const key = shown ? `${shown.crew}:${shown.tag}` : "";
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the tag comes, goes or changes
  useLayoutEffect(() => {
    const before = shift.current;
    let now = 0;
    if (shown && chipRef.current && rootRef.current) {
      const gap = Number.parseFloat(getComputedStyle(rootRef.current).columnGap) || 0;
      now = chipRef.current.offsetWidth + gap;
    }
    shift.current = now;
    // The first paint is simply there; later the name starts where it was and glides.
    if (before === null || before === now || !layout) return;
    nameRef.current?.animate(
      [{ transform: `translateX(${before - now}px)` }, { transform: "translateX(0px)" }],
      { duration: 200, easing: "cubic-bezier(0.16, 1, 0.3, 1)" },
    );
  }, [key]);
  return (
    <span ref={rootRef} className={cn("relative flex min-w-0 items-center gap-1.5", className)}>
      <AnimatePresence initial={false} mode="popLayout">
        {shown ? (
          <motion.span
            key={key}
            className="flex shrink-0"
            style={{ transformOrigin: "left center" }}
            initial={reduce ? { opacity: 0 } : { opacity: 0, transform: "scale(0.96)" }}
            animate={{
              opacity: 1,
              ...(reduce ? {} : { transform: "scale(1)" }),
              transition: { duration: 0.16, ease: easeOutExpo },
            }}
            exit={{
              opacity: 0,
              ...(reduce ? {} : { transform: "scale(0.96)" }),
              transition: { duration: 0.11, ease: easeOutExpo },
            }}
          >
            <span
              ref={(element) => {
                if (element) chipRef.current = element;
              }}
              className={cn("flex transition-opacity duration-150", dim && "opacity-70")}
            >
              <BadgeChip badge={shown} wearer={wearer} size={size} mode={chip} />
            </span>
          </motion.span>
        ) : null}
      </AnimatePresence>
      <span ref={nameRef} className={cn("min-w-0 truncate", nameClassName)}>
        {name}
      </span>
      {children}
    </span>
  );
}
