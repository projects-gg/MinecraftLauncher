// Entrances that never lose their start (G6, docs/design.md §5). Opening a dialog, sheet or
// popover mounts a whole panel: React, style recalculation, layout and the first paint take
// 40–110 ms on the main thread, and the GPU can need as long again to rasterize the new layer
// (pictures, blur) before it is on screen. An animation started meanwhile has its clock running
// (motion pins an animation's start to the frame that created it), so its first part is never
// seen. Layers here wait instead: the panel holds its starting pose until its layer has really
// been drawn, then the entrance plays from its first frame. The panel's content is not rendered
// again for it (a context change reaches only the gated motion elements).
import { type HTMLMotionProps, motion } from "motion/react";
import {
  createContext,
  type ForwardedRef,
  forwardRef,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { flushSync } from "react-dom";

/** The longest a held entrance waits for its layer; it starts then at the latest. */
const GATE_LIMIT_MS = 250;

/**
 * A held panel is shown at this opacity while its layer is drawn: invisible to the eye, but not
 * zero, because the compositor does not rasterize a transparent layer (the work, and the wait,
 * would land in the first frames of the entrance).
 */
const PROBE_OPACITY = 0.01;

type Gate = { ready: boolean; register: (node: Element | null) => void };

const GateContext = createContext<Gate>({ ready: true, register: () => {} });

/**
 * Wrap a layer that mounts on open (a direct child of `AnimatePresence`, keyed): the gated motion
 * elements inside it hold their entrance until the layer is on screen.
 */
export function EntranceGate({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);
  const nodes = useRef(new Set<Element>());
  const register = useCallback((node: Element | null) => {
    if (node) nodes.current.add(node);
  }, []);

  useEffect(() => {
    let finished = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let probes: Animation[] = [];
    const stop = () => {
      finished = true;
      clearTimeout(timer);
      for (const probe of probes) probe.cancel();
    };
    const open = () => {
      if (finished) return;
      stop();
      // Rendering at once (not in a later task) starts the entrance in the very next frame.
      flushSync(() => setReady(true));
    };
    // The first callback runs before the mounting frame is drawn, the second once the main
    // thread has drawn it. The compositor may still be rasterizing the layer then: a tiny
    // animation on each held element starts only when its layer is shown, and says so.
    let second = 0;
    const first = requestAnimationFrame(() => {
      second = requestAnimationFrame(() => {
        probes = [...nodes.current]
          .filter((node) => node.isConnected && typeof node.animate === "function")
          .map((node) =>
            node.animate(
              { opacity: [PROBE_OPACITY, PROBE_OPACITY] },
              { duration: GATE_LIMIT_MS * 4 },
            ),
          );
        if (probes.length === 0) {
          open();
          return;
        }
        timer = setTimeout(open, GATE_LIMIT_MS);
        Promise.all(probes.map((probe) => probe.ready)).then(open, open);
      });
    });
    return () => {
      cancelAnimationFrame(first);
      cancelAnimationFrame(second);
      stop();
    };
  }, []);

  const gate = useMemo(() => ({ ready, register }), [ready, register]);
  return <GateContext.Provider value={gate}>{children}</GateContext.Provider>;
}

/** False while the surrounding gate holds its entrance. */
export function useEntranceReady(): boolean {
  return useContext(GateContext).ready;
}

type Gatable = { initial?: unknown; animate?: unknown };

/** Whether the props describe an entrance to hold (`initial={false}` has none). */
function holds(props: Gatable): boolean {
  return props.initial !== undefined && props.initial !== false;
}

/** Motion props whose `animate` stays at `initial` until `ready`. */
export function gateEntrance<T extends Gatable>(props: T, ready: boolean): T {
  return ready || !holds(props) ? props : { ...props, animate: props.initial };
}

/** The gated props and a ref that also tells the gate which element to wait for. */
function useGated<P extends Gatable, T extends Element>(
  props: P,
  ref: ForwardedRef<T>,
): [P, (node: T | null) => void] {
  const { ready, register } = useContext(GateContext);
  const held = holds(props);
  const setRef = useCallback(
    (node: T | null) => {
      if (held) register(node);
      if (typeof ref === "function") ref(node);
      else if (ref) ref.current = node;
    },
    [held, ref, register],
  );
  return [gateEntrance(props, ready), setRef];
}

/** `motion.div` and `motion.aside` that wait for their `EntranceGate` (Radix `asChild` safe). */
export const GatedMotion = {
  div: forwardRef<HTMLDivElement, HTMLMotionProps<"div">>(function GatedDiv(props, ref) {
    const [gated, setRef] = useGated(props, ref);
    return <motion.div ref={setRef} {...gated} />;
  }),
  aside: forwardRef<HTMLElement, HTMLMotionProps<"aside">>(function GatedAside(props, ref) {
    const [gated, setRef] = useGated(props, ref);
    return <motion.aside ref={setRef} {...gated} />;
  }),
};

/**
 * Fetches and decodes images while the browser is idle, so their first appearance (a picker, a
 * list of icons) is not held up by decoding. Keeps them referenced so they stay cached.
 */
const warmed = new Map<string, HTMLImageElement>();
export function prewarmImages(urls: readonly string[]) {
  const queue = urls.filter((url) => url && !warmed.has(url));
  if (queue.length === 0 || typeof window === "undefined") return;
  const idle: (cb: () => void) => void =
    "requestIdleCallback" in window
      ? (cb) => window.requestIdleCallback(cb, { timeout: 2000 })
      : (cb) => setTimeout(cb, 200);
  const next = () => {
    const url = queue.shift();
    if (!url) return;
    const image = new Image();
    image.decoding = "async";
    image.src = url;
    warmed.set(url, image);
    // One at a time, each in an idle moment: never a burst of decoding.
    image
      .decode()
      .catch(() => undefined)
      .finally(() => idle(next));
  };
  idle(next);
}
