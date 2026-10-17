// Files dragged over the window, in one place: the drag events (with the cursor), what the files
// are, the zones on screen and which one is under the cursor. DropHost starts the source; places
// that show a zone use `useDropZone`. The rules themselves are pure, in zones.ts.
import { useCallback, useEffect, useId, useRef } from "react";
import { create } from "zustand";
import { api, type DroppedFile } from "@/lib/api";
import {
  activeZones,
  dropTarget,
  type Point,
  probePoints,
  WINDOW_ZONE,
  windowBox,
  type ZoneBox,
  type ZoneLayer,
  zoneAt,
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
}

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
  /** The zones it talks to, decided as it entered. */
  active: string[];
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
  hovered: null,
  absorbing: null,
  taken: null,
  announcement: "",
  announcedFor: null,
  setPendingPack: (pendingPack) => set({ pendingPack }),
}));

/**
 * A drop zone on screen. `ref` goes on the zone's box (one that does not move with the zone's
 * lift); `state` is its look for `DropZone`; `drag` is the drag it takes part in. A claim (a spec
 * with `onDrop`) takes every drop on the window while it is mounted and enabled, and the
 * window-wide zone steps aside meanwhile.
 */
export function useDropZone(spec: ZoneSpec, enabled = true) {
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
  onWindowDrop: (paths: string[], files: DroppedFile[] | null) => void;
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

  const boxes = (active: string[]): ZoneBox[] =>
    active.flatMap((id): ZoneBox[] => {
      if (id === WINDOW_ZONE) {
        return [{ id, box: windowBox(window.innerWidth, window.innerHeight), depth: 0 }];
      }
      const entry = zones.get(id);
      const element = entry?.element;
      if (!entry || !element) return [];
      const { left, top, right, bottom } = element.getBoundingClientRect();
      return [{ id, box: { left, top, right, bottom }, depth: depthOf(entry.spec.layer) }];
    });

  // Hit-testing once per frame at most: the cursor reports more often than the screen draws.
  const hitTest = () => {
    frame = 0;
    const { drag, active, hovered } = get();
    if (!drag) return;
    const id = zoneAt(point, boxes(active));
    if (id !== hovered) set({ hovered: id });
  };

  const resolve = () =>
    activeZones(
      [...zones].map(([id, entry]) => ({
        id,
        layer: entry.spec.layer,
        shown: entry.spec.layer === "claim" || isShown(entry.element),
      })),
      options.windowEnabled(),
    );

  const begin = (paths: string[]): Drag => {
    clearTimeout(settle);
    next += 1;
    const drag: Drag = { id: next, paths, files: null };
    set({
      drag,
      active: resolve(),
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
        if (current?.id === drag.id) set({ drag: { ...current, files } });
      });
    return drag;
  };

  const end = () => {
    cancelAnimationFrame(frame);
    frame = 0;
    set({ drag: null, active: [], hovered: null, announcement: "" });
  };

  const drop = (paths: string[]) => {
    cancelAnimationFrame(frame);
    frame = 0;
    // A drop without an enter before it (the browser preview's own drags can do that).
    const drag = get().drag ?? (paths.length > 0 ? begin(paths) : null);
    if (!drag) return;
    const { active } = get();
    const dropped: Drag = { ...drag, paths: paths.length > 0 ? paths : drag.paths };
    const hovered = zoneAt(point, boxes(active));
    const claims = active.filter((id) => zones.get(id)?.spec.layer === "claim");
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
    } else if (active.length > 0) {
      target = dropTarget(active, hovered, null);
      accepted = options.windowAccepts(dropped);
      if (paths.length > 0) options.onWindowDrop(paths, dropped.files);
    }
    set({
      drag: null,
      active: [],
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
    cancelAnimationFrame(frame);
    clearTimeout(settle);
    end();
  };
}
