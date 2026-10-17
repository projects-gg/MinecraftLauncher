// Files dragged over the window, in one place: the drag events (with the cursor), what the files
// are, the zones on screen and which one is under the cursor. DropHost starts the source; places
// that show a zone use `useDropZone`, instance cards `useCardDrop` (card-zone.tsx). The rules
// themselves are pure, in zones.ts and cards.ts.
import { useCallback, useEffect, useId, useRef } from "react";
import { create } from "zustand";
import { api, type DroppedFile, type InstanceSummary } from "@/lib/api";
import { cardTarget } from "./cards";
import {
  activeZones,
  type Box,
  dropTarget,
  hitBox,
  hoveredZone,
  type Point,
  probePoints,
  WINDOW_ZONE,
  windowBox,
  type ZoneBox,
  type ZoneLayer,
  zoneDimmed,
  zoneState,
} from "./zones";

/** Files being dragged over the window; `files` once the backend has read what they are. */
export interface Drag {
  id: number;
  paths: string[];
  files: DroppedFile[] | null;
}

export interface ZoneSpec {
  layer: ZoneLayer;
  /** Whether these files can go here; files still being read should look welcome. */
  accepts: (drag: Drag) => boolean;
  /** Claims: takes the drop, wherever on the window it lands while the place is open. */
  onDrop?: (paths: string[], files: DroppedFile[] | null) => void;
  /** Whether the zone takes part in this drag at all; asked again once the files are read. */
  joins?: (drag: Drag) => boolean;
  /**
   * An instance card: files dropped on it go into this instance. While cards take part the
   * window zone steps aside, and a drop that lands on no card goes into no instance.
   */
  into?: InstanceSummary;
  /** How far the zone's hit box reaches past its element (cards meet in the gaps). */
  reach?: number;
}

/** Where a drop for the window's own handling landed. */
export type Landing =
  /** The window zone or a page zone: the window's target (the open page, else the selection). */
  | { kind: "window" }
  /** An instance card: that instance. */
  | { kind: "card"; into: InstanceSummary }
  /** Cards took part and the drop landed on none of them. */
  | { kind: "missed" };

interface ZoneEntry {
  readonly spec: ZoneSpec;
  element: HTMLElement | null;
  /** Registration order: the place opened last is on top. */
  order: number;
}

/** Zones on screen, by id. Hit-testing reads them; nothing renders from them. */
const zones = new Map<string, ZoneEntry>();
let registered = 0;

interface DropState {
  /** A modpack dropped onto the window, waiting for the import flow. */
  pendingPack: string | null;
  /** The drag in progress. */
  drag: Drag | null;
  /** The zones it talks to, decided as it entered (and again once the files are read). */
  active: string[];
  /** Those zones are instance cards (the library): the window zone steps aside. */
  cards: boolean;
  /** The zone under the cursor. */
  hovered: string | null;
  /** The zone that just took a drop (its short "absorb"), and that drop. */
  absorbing: string | null;
  taken: Drag | null;
  /** One polite status line for screen readers (DropHost shows it). */
  announcement: string;
  announcedFor: number | null;
  setPendingPack: (path: string | null) => void;
}

export const useDrop = create<DropState>((set) => ({
  pendingPack: null,
  drag: null,
  active: [],
  cards: false,
  hovered: null,
  absorbing: null,
  taken: null,
  announcement: "",
  announcedFor: null,
  setPendingPack: (pendingPack) => set({ pendingPack }),
}));

/**
 * Registers a zone while mounted and enabled. `ref` goes on the zone's box (one that does not
 * move with the zone's own motion); `id` is how the store names it.
 */
export function useZone(spec: ZoneSpec, enabled = true) {
  const id = useId();
  const latest = useRef(spec);
  latest.current = spec;
  const element = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (!enabled) return;
    registered += 1;
    const entry: ZoneEntry = {
      get spec() {
        return latest.current;
      },
      element: element.current,
      order: registered,
    };
    zones.set(id, entry);
    return () => {
      if (zones.get(id) === entry) zones.delete(id);
    };
  }, [enabled, id]);
  const ref = useCallback(
    (node: HTMLElement | null) => {
      element.current = node;
      const entry = zones.get(id);
      if (entry) entry.element = node;
    },
    [id],
  );
  return { id, ref };
}

/**
 * A drop zone on screen. `ref` goes on the zone's box (one that does not move with the zone's
 * lift); `state` is its look for `DropZone`; `drag` is the drag it takes part in. A claim (a spec
 * with `onDrop`) takes every drop on the window while it is mounted and enabled, and the
 * window-wide zone steps aside meanwhile.
 */
export function useDropZone(spec: ZoneSpec, enabled = true) {
  const { id, ref } = useZone(spec, enabled);
  const drag = useDrop((s) => s.drag);
  const active = useDrop((s) => s.drag !== null && s.active.includes(id));
  const hovered = useDrop((s) => s.hovered);
  const absorbing = useDrop((s) => s.absorbing);
  const taken = useDrop((s) => s.taken);
  const accepts = active && drag ? spec.accepts(drag) : true;
  const other = hovered && hovered !== id ? zones.get(hovered)?.spec : undefined;
  const hoveredAccepts = drag && other ? other.accepts(drag) : true;
  const state = zoneState(id, { active, accepts, hovered, hoveredAccepts, absorbing });
  return {
    ref,
    state,
    dimmed: zoneDimmed(id, state, hovered),
    // While it absorbs, the zone still reads the files it took (its text does not change).
    drag: active ? drag : state === "absorb" ? taken : null,
  };
}

/** Says once per drag what the zone will do (DropHost shows it in a polite status line). */
export function useDropAnnouncement(text: string | null, dragId: number | undefined) {
  useEffect(() => {
    if (!text || dragId === undefined) return;
    const { drag, announcedFor } = useDrop.getState();
    if (drag?.id !== dragId || announcedFor === dragId) return;
    useDrop.setState({ announcement: text, announcedFor: dragId });
  }, [text, dragId]);
}

/** How long a zone's "absorb" shows before it settles or goes. */
const ABSORB_MS = 320;

const depthOf = (layer: ZoneLayer) => (layer === "claim" ? 2 : 1);

const boxOf = (element: Element): Box => {
  const { left, top, right, bottom } = element.getBoundingClientRect();
  return { left, top, right, bottom };
};

/** On screen and not under a dialog or a sheet: one of its probe points hits the zone itself. */
function isShown(element: HTMLElement | null): boolean {
  if (!element?.isConnected) return false;
  const box = element.getBoundingClientRect();
  if (box.width < 8 || box.height < 8) return false;
  return probePoints(box, window.innerWidth, window.innerHeight).some((p) =>
    element.contains(document.elementFromPoint(p.x, p.y)),
  );
}

export interface DragSourceOptions {
  /** The window's own drops are on (off during the welcome flow). */
  windowEnabled: () => boolean;
  /** Whether the window's own handling would take these files. */
  windowAccepts: (drag: Drag) => boolean;
  /** A drop for the window's own handling (no place with its own handling is open). */
  onWindowDrop: (paths: string[], files: DroppedFile[] | null, landing: Landing) => void;
}

/**
 * Listens to the window's drag events (DropHost, once): the zones a drag talks to, the zone
 * under the cursor, what the files are, and who takes the drop. Returns the unsubscribe.
 */
export function startDragSource(options: DragSourceOptions): () => void {
  const { getState: get, setState: set } = useDrop;
  let next = 0;
  let point: Point | null = null;
  let frame = 0;
  let settle: ReturnType<typeof setTimeout> | undefined;

  const boxes = (active: string[]): ZoneBox[] => {
    const clips = new Map<Element, Box>();
    return active.flatMap((id): ZoneBox[] => {
      if (id === WINDOW_ZONE) {
        return [{ id, box: windowBox(window.innerWidth, window.innerHeight), depth: 0 }];
      }
      const entry = zones.get(id);
      const element = entry?.element;
      if (!entry || !element) return [];
      // Cut to the scroll area that shows the zone: a card scrolled under the page's header is
      // not under the cursor there.
      const scroller = element.closest("[data-drop-clip]");
      let clip: Box | null = null;
      if (scroller) {
        clip = clips.get(scroller) ?? boxOf(scroller);
        clips.set(scroller, clip);
      }
      const box = hitBox(boxOf(element), entry.spec.reach ?? 0, clip);
      return box ? [{ id, box, depth: depthOf(entry.spec.layer) }] : [];
    });
  };

  // Hit-testing once per frame at most: the cursor reports more often than the screen draws.
  const hitTest = () => {
    frame = 0;
    const { drag, active, hovered } = get();
    if (!drag) return;
    const id = hoveredZone(point, boxes(active), hovered);
    if (id !== hovered) set({ hovered: id });
  };

  const resolve = (drag: Drag) => {
    const active = activeZones(
      [...zones]
        .filter(([, entry]) => entry.spec.joins?.(drag) ?? true)
        .map(([id, entry]) => ({
          id,
          layer: entry.spec.layer,
          shown: entry.spec.layer === "claim" || isShown(entry.element),
        })),
      options.windowEnabled(),
    );
    return { active, cards: active.some((id) => Boolean(zones.get(id)?.spec.into)) };
  };

  const begin = (paths: string[]): Drag => {
    clearTimeout(settle);
    next += 1;
    const drag: Drag = { id: next, paths, files: null };
    set({
      drag,
      ...resolve(drag),
      hovered: null,
      absorbing: null,
      taken: null,
      announcement: "",
      announcedFor: null,
    });
    hitTest();
    void api
      .classifyFiles(paths)
      .catch(() => [] as DroppedFile[])
      .then((files) => {
        const current = get().drag;
        if (current?.id !== drag.id) return;
        const known = { ...current, files };
        // Some zones take part only for some files (instance cards): they are asked again.
        set({ drag: known, ...resolve(known) });
        hitTest();
      });
    return drag;
  };

  const end = () => {
    cancelAnimationFrame(frame);
    frame = 0;
    set({ drag: null, active: [], cards: false, hovered: null, announcement: "" });
  };

  const drop = (paths: string[]) => {
    cancelAnimationFrame(frame);
    frame = 0;
    // A drop without an enter before it (the browser preview's own drags can do that).
    const drag = get().drag ?? (paths.length > 0 ? begin(paths) : null);
    if (!drag) return;
    const { active, hovered: shown } = get();
    const dropped: Drag = { ...drag, paths: paths.length > 0 ? paths : drag.paths };
    const hovered = hoveredZone(point, boxes(active), shown);
    const claims = active.filter((id) => zones.get(id)?.spec.layer === "claim");
    const cards = active.filter((id) => zones.get(id)?.spec.into);
    let target: string | null = null;
    let accepted = false;
    if (claims.length > 0) {
      const newest = claims.reduce((a, b) =>
        (zones.get(b)?.order ?? 0) > (zones.get(a)?.order ?? 0) ? b : a,
      );
      target = dropTarget(claims, hovered, newest);
      const spec = target ? zones.get(target)?.spec : undefined;
      accepted = spec?.accepts(dropped) ?? false;
      if (paths.length > 0) spec?.onDrop?.(paths, dropped.files);
    } else if (cards.length > 0) {
      // The card under the cursor takes the files; between the cards they go nowhere.
      target = cardTarget(cards, hovered);
      const spec = target ? zones.get(target)?.spec : undefined;
      accepted = spec?.accepts(dropped) ?? false;
      const landing: Landing = spec?.into ? { kind: "card", into: spec.into } : { kind: "missed" };
      if (paths.length > 0) options.onWindowDrop(paths, dropped.files, landing);
    } else if (active.length > 0) {
      target = dropTarget(active, hovered, null);
      accepted = options.windowAccepts(dropped);
      if (paths.length > 0) options.onWindowDrop(paths, dropped.files, { kind: "window" });
    }
    set({
      drag: null,
      active: [],
      cards: false,
      hovered: null,
      absorbing: accepted ? target : null,
      taken: accepted ? dropped : null,
      announcement: "",
    });
    if (accepted && target) {
      const zone = target;
      settle = setTimeout(() => {
        if (get().absorbing === zone) set({ absorbing: null, taken: null });
      }, ABSORB_MS);
    }
  };

  // A list scrolled during the drag brings other cards under a cursor that may not move: which
  // zones take part, and which one is under the cursor, are worked out again.
  let rescan = 0;
  const onScroll = () => {
    if (!get().drag || rescan) return;
    rescan = requestAnimationFrame(() => {
      rescan = 0;
      const drag = get().drag;
      if (!drag) return;
      set(resolve(drag));
      cancelAnimationFrame(frame);
      hitTest();
    });
  };
  window.addEventListener("scroll", onScroll, { capture: true, passive: true });

  const unsubscribe = api.onFileDrag((event) => {
    switch (event.type) {
      case "enter":
        point = event.position ?? null;
        // Text or links dragged from a browser carry no files: nothing to show.
        if (event.paths.length > 0) begin(event.paths);
        break;
      case "over":
        if (event.position) point = event.position;
        if (get().drag && !frame) frame = requestAnimationFrame(hitTest);
        break;
      case "drop":
        if (event.position) point = event.position;
        drop(event.paths);
        break;
      default:
        end();
    }
  });
  return () => {
    unsubscribe();
    window.removeEventListener("scroll", onScroll, { capture: true });
    cancelAnimationFrame(frame);
    cancelAnimationFrame(rescan);
    clearTimeout(settle);
    end();
  };
}
