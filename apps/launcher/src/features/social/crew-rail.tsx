// The crew page's rail (TEAM4): the player's crews as their marks, in the player's order (the first
// is the primary crew, "Ana ekip"), each crew's tag under its mark, and "Ekip kur ya da katıl" at
// the end (dimmed at the crew limit, which its tooltip says; a gold dot while invites to crews wait,
// which it opens even at the limit, to decline them). The crew on screen wears the brand's
// pixel brackets; they travel to the crew the player picks. A crew with a call waiting carries a
// small gold megaphone, a muted one a struck bell, one whose join requests wait for the player a
// gold dot (TEAM5; the add tile also opens while the player's own requests wait). Marks are
// dragged sideways to reorder (the
// others glide aside: the glide answers the player's own hand, so it stays in Performance mode too);
// Alt+←/→ and each mark's menu ("Ana ekip yap", "Sola taşı", "Sağa taşı") do the same without
// dragging. A tab list: ←/→ move between crews, Home/End to the ends.
import { BellSlash, MegaphoneSimple, Plus, UsersThree } from "@phosphor-icons/react";
import { ContextMenu, cn, type MenuItem, PixelBrackets, springSnappy, Tooltip } from "@pl/ui";
import { AnimatePresence, type DragControls, motion, Reorder, useDragControls } from "motion/react";
import {
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
  useEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import type { SocialTeam } from "@/lib/api/social-types";
import { callNames, unansweredCall } from "./calls";
import { waitingRequests } from "./crew-joining";
import { saveCrewOrder } from "./crew-order";
import { crewFirst, moveCrew, NEW_CREW_TAB } from "./crews";
import { useSocialMotion } from "./motion";
import { crewMutedNow } from "./mute";
import { useSocialUi } from "./store";
import { CrewMark } from "./team-emblem";
import { useCallClock } from "./use-call-clock";

const MARK = 36;

/** Where the selection's brackets travel (one rail is on screen at a time). */
const BRACKETS = "crew-rail-brackets";

function Brackets({ travel }: { travel: boolean }) {
  return (
    <motion.span
      layoutId={travel ? BRACKETS : undefined}
      aria-hidden
      className="pointer-events-none absolute -inset-[4px]"
      transition={springSnappy}
      initial={travel ? false : { opacity: 0 }}
      animate={{ opacity: 1 }}
    >
      <PixelBrackets size={8} />
    </motion.span>
  );
}

/** A small round badge on a mark's corner; fades in and out where it is. */
function Badge({
  show,
  className,
  children,
}: {
  show: boolean;
  className: string;
  children: ReactNode;
}) {
  return (
    <AnimatePresence initial={false}>
      {show ? (
        <motion.span
          aria-hidden
          className={cn(
            "absolute flex size-[15px] items-center justify-center rounded-full bg-obsidian-950 ring-1",
            className,
          )}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: { duration: 0.18 } }}
          exit={{ opacity: 0, transition: { duration: 0.12 } }}
        >
          {children}
        </motion.span>
      ) : null}
    </AnimatePresence>
  );
}

function CrewTab({
  crew,
  index,
  order,
  selected,
  calling,
  muted,
  asking,
  travel,
  controls,
  dragged,
  lifted,
  onSelect,
  onKeyDown,
}: {
  crew: SocialTeam;
  index: number;
  /** The crews' ids in the order shown. */
  order: string[];
  selected: boolean;
  calling: boolean;
  muted: boolean;
  /** Join requests waiting for the player's answer. */
  asking: number;
  travel: boolean;
  controls: DragControls;
  dragged: RefObject<boolean>;
  /** Being dragged: its tooltip waits (the pointer rides along with it). */
  lifted: boolean;
  onSelect: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => void;
}) {
  const { t } = useTranslation();
  const count = order.length;
  const main = index === 0 && count > 1;
  const label = [
    crew.name,
    main ? t("crews.main") : null,
    muted ? t("crews.muted") : null,
    calling ? t("crews.callWaiting") : null,
    asking ? t("crewWave2.requests.waiting", { count: asking }) : null,
  ]
    .filter(Boolean)
    .join(", ");
  const move = (next: string[] | null) => {
    if (next) void saveCrewOrder(next, crew.id);
  };
  const items: Array<MenuItem | null> = [
    { label: t("teams.open"), icon: <UsersThree size={16} />, onSelect },
  ];
  if (count > 1) {
    items.push(
      null,
      ...(index > 0
        ? [{ label: t("crews.makeMain"), onSelect: () => move(crewFirst(order, crew.id)) }]
        : []),
      {
        label: t("crews.moveLeft"),
        shortcut: "Alt ←",
        disabled: index === 0,
        onSelect: () => move(moveCrew(order, crew.id, -1)),
      },
      {
        label: t("crews.moveRight"),
        shortcut: "Alt →",
        disabled: index === count - 1,
        onSelect: () => move(moveCrew(order, crew.id, 1)),
      },
    );
  }
  return (
    <ContextMenu
      items={items}
      wrapTrigger={(trigger) => (
        <Tooltip
          disabled={lifted}
          content={
            <span className="block">
              {crew.name}
              {main ? (
                <span className="block font-normal text-ash-300">{t("crews.main")}</span>
              ) : null}
            </span>
          }
        >
          {trigger}
        </Tooltip>
      )}
    >
      <button
        type="button"
        role="tab"
        id={`crew-tab-${crew.id}`}
        aria-selected={selected}
        aria-label={label}
        tabIndex={selected ? 0 : -1}
        data-crew-tab={crew.id}
        onPointerDown={(event) => {
          if (event.button !== 0 || count < 2) return;
          dragged.current = false;
          controls.start(event);
        }}
        onClick={() => {
          if (dragged.current) return;
          onSelect();
        }}
        onKeyDown={onKeyDown}
        className="group/tab flex w-11 cursor-pointer touch-none flex-col items-center gap-1 rounded-md pt-1 pb-0.5 outline-none select-none focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <span
          className={cn(
            "relative flex transition-[transform,scale,filter] duration-150 ease-out group-active/tab:scale-[0.94]",
            !selected && "brightness-[0.9] group-hover/tab:brightness-110",
          )}
          style={{ width: MARK, height: MARK }}
        >
          <CrewMark look={crew} size={MARK} />
          {selected ? <Brackets travel={travel} /> : null}
          <Badge show={calling} className="-top-1.5 -right-1.5 text-gold-300 ring-gold-400/45">
            <MegaphoneSimple size={9} weight="fill" />
          </Badge>
          <Badge show={muted} className="-right-1.5 -bottom-1.5 text-ash-300 ring-white/15">
            <BellSlash size={9} weight="bold" />
          </Badge>
          <Badge
            show={asking > 0}
            className="-top-1 -left-1 size-[9px] bg-gold-300 ring-obsidian-900 ring-2"
          >
            {null}
          </Badge>
        </span>
        <span
          className={cn(
            "max-w-full truncate font-display text-[11px] leading-none tracking-[0.06em] transition-colors duration-150",
            selected ? "text-bone-50" : "text-ash-400 group-hover/tab:text-ash-300",
          )}
        >
          {crew.tag}
        </span>
      </button>
    </ContextMenu>
  );
}

/** One crew as a reorderable item: its drag starts from a press on its mark. */
function RailItem({
  id,
  onDragEnd,
  children,
}: {
  id: string;
  onDragEnd: () => void;
  children: (controls: DragControls, dragged: RefObject<boolean>, lifted: boolean) => ReactNode;
}) {
  const controls = useDragControls();
  const dragged = useRef(false);
  const [lifted, setLifted] = useState(false);
  const calm = useCalm();
  return (
    <Reorder.Item
      as="li"
      value={id}
      role="presentation"
      layout="position"
      drag="x"
      dragListener={false}
      dragControls={controls}
      dragElastic={0.12}
      onDragStart={() => {
        dragged.current = true;
        setLifted(true);
      }}
      onDragEnd={() => {
        setLifted(false);
        onDragEnd();
      }}
      transition={{ layout: springSnappy }}
      // Reorder.Item writes its own z-index while dragging; the lifted mark grows a touch.
      className="relative flex"
      whileDrag={calm({ scale: 1.08 })}
      initial={calm({ opacity: 0, scale: 0.9 })}
      animate={calm({ opacity: 1, scale: 1 })}
      exit={calm({ opacity: 0, scale: 0.9, transition: { duration: 0.16 } })}
    >
      {children(controls, dragged, lifted)}
    </Reorder.Item>
  );
}

export function CrewRail({
  crews,
  shown,
  canAdd,
  limit,
}: {
  crews: SocialTeam[];
  /** The crew on screen, or the add view. */
  shown: string;
  canAdd: boolean;
  limit: number;
}) {
  const { t } = useTranslation();
  const { layout } = useSocialMotion();
  const setTab = useSocialUi((s) => s.setCrewTab);
  const calls = useSocialUi((s) => s.view.calls);
  const all = useSocialUi((s) => s.view.crewNotifications);
  const invites = useSocialUi((s) => s.view.teamInvites.length);
  const asked = useSocialUi((s) => s.view.teamRequests.length);
  const now = useCallClock((s) => s.now);
  const listRef = useRef<HTMLUListElement>(null);
  const addRef = useRef<HTMLButtonElement>(null);

  // While a mark is dragged its order lives here; the saved order takes over afterwards.
  const ids = crews.map((crew) => crew.id);
  const saved = ids.join();
  const [dragOrder, setDragOrder] = useState<string[] | null>(null);
  const dragging = useRef(false);
  const latest = useRef<string[] | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs whenever the saved order changes
  useEffect(() => {
    if (dragging.current) return;
    latest.current = null;
    setDragOrder(null);
  }, [saved]);
  const order = (dragOrder ?? ids).filter((id) => ids.includes(id));
  const shownCrews = order
    .map((id) => crews.find((crew) => crew.id === id))
    .filter((crew): crew is SocialTeam => Boolean(crew));

  const focusTab = (id: string) =>
    requestAnimationFrame(() =>
      (id === NEW_CREW_TAB
        ? addRef.current
        : listRef.current?.querySelector<HTMLElement>(`[data-crew-tab="${id}"]`)
      )?.focus(),
    );
  const go = (id: string | undefined) => {
    if (!id) return;
    setTab(id);
    focusTab(id);
  };
  const addSelected = shown === NEW_CREW_TAB;
  // At the limit the add view still opens while invites wait (to see and decline them), and while
  // the player's own join requests wait (to withdraw them).
  const addOpen = canAdd || invites > 0 || asked > 0 || addSelected;
  const targets = [...order, ...(addOpen ? [NEW_CREW_TAB] : [])];

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, id: string) => {
    // Not with Ctrl: Windows reports AltGr as Ctrl+Alt (Turkish Q types with it).
    if (id !== NEW_CREW_TAB && event.altKey && !event.ctrlKey) {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault();
      const next = moveCrew(order, id, event.key === "ArrowLeft" ? -1 : 1);
      if (!next) return;
      void saveCrewOrder(next, id);
      focusTab(id);
      return;
    }
    const at = targets.indexOf(id);
    const step: Record<string, number | undefined> = {
      ArrowRight: (at + 1) % targets.length,
      ArrowLeft: (at - 1 + targets.length) % targets.length,
      Home: 0,
      End: targets.length - 1,
    };
    const to = step[event.key];
    if (to === undefined) return;
    event.preventDefault();
    go(targets[to]);
  };

  const endDrag = (id: string) => {
    const next = dragging.current ? latest.current : null;
    dragging.current = false;
    if (!next) return;
    void saveCrewOrder(next, id).then(() => {
      // A failure brings the saved order back.
      if (
        useSocialUi
          .getState()
          .view.crews.map((c) => c.id)
          .join() !== next.join()
      ) {
        setDragOrder(null);
      }
    });
  };

  const addLabel = canAdd ? t("crews.add") : t("crews.limit", { count: limit });

  return (
    <div className="-mx-1 mb-3 flex items-start gap-1.5 overflow-x-auto px-1 pt-1.5 pb-1 [scrollbar-width:none]">
      <Reorder.Group
        as="ul"
        axis="x"
        ref={listRef}
        values={order}
        onReorder={(next: string[]) => {
          dragging.current = true;
          latest.current = next;
          setDragOrder(next);
        }}
        role="tablist"
        aria-label={t("crews.rail")}
        aria-orientation="horizontal"
        className="relative flex items-start gap-1.5"
      >
        <AnimatePresence initial={false}>
          {shownCrews.map((crew, index) => (
            <RailItem key={crew.id} id={crew.id} onDragEnd={() => endDrag(crew.id)}>
              {(controls, dragged, lifted) => (
                <CrewTab
                  crew={crew}
                  index={index}
                  order={order}
                  selected={shown === crew.id}
                  calling={Boolean(
                    unansweredCall(
                      calls.filter((call) => callNames(call, crew, crews)),
                      now,
                    ),
                  )}
                  muted={crewMutedNow(crew, all, now)}
                  asking={waitingRequests(crew, now).length}
                  travel={layout}
                  controls={controls}
                  dragged={dragged}
                  lifted={lifted}
                  onSelect={() => setTab(crew.id)}
                  onKeyDown={(event) => onKeyDown(event, crew.id)}
                />
              )}
            </RailItem>
          ))}
        </AnimatePresence>
      </Reorder.Group>
      <Tooltip content={addLabel}>
        <button
          ref={addRef}
          type="button"
          role="tab"
          id={`crew-tab-${NEW_CREW_TAB}`}
          aria-selected={addSelected}
          aria-disabled={!addOpen}
          aria-label={invites ? `${addLabel}, ${t("teams.invites")}` : addLabel}
          tabIndex={addSelected ? 0 : -1}
          data-crew-tab={NEW_CREW_TAB}
          onClick={() => {
            if (addOpen) setTab(NEW_CREW_TAB);
          }}
          onKeyDown={(event) => onKeyDown(event, NEW_CREW_TAB)}
          className={cn(
            "group/add flex w-11 shrink-0 flex-col items-center rounded-md pt-1 pb-0.5 outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
            addOpen ? "cursor-pointer" : "cursor-default",
          )}
        >
          <span
            className={cn(
              "relative flex items-center justify-center transition-[transform,scale] duration-150 ease-out [--cut:7px]",
              canAdd && "group-active/add:scale-[0.94]",
            )}
            style={{ width: MARK, height: MARK }}
          >
            {/* At the limit the tile dims; the invite dot and the brackets stay bright. */}
            <span
              aria-hidden
              className={cn(
                "absolute inset-0 flex items-center justify-center transition-opacity duration-150",
                !canAdd && "opacity-45",
              )}
            >
              <span className="chamfer absolute inset-0 bg-white/14" />
              <span
                className={cn(
                  "chamfer absolute inset-px bg-obsidian-900/90 transition-colors duration-150 [--cut:6px]",
                  canAdd && "group-hover/add:bg-obsidian-800",
                )}
              />
              <Plus
                size={16}
                weight="bold"
                className={cn(
                  "relative transition-colors duration-150",
                  addSelected ? "text-gold-300" : "text-ash-300 group-hover/add:text-bone-100",
                )}
              />
            </span>
            {addSelected ? <Brackets travel={layout} /> : null}
            <Badge
              show={invites > 0}
              className="-top-1 -right-1 size-[9px] bg-gold-300 ring-obsidian-900 ring-2"
            >
              {null}
            </Badge>
          </span>
        </button>
      </Tooltip>
    </div>
  );
}
