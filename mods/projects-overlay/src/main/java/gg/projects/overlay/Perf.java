package gg.projects.overlay;

/**
 * Optional render-time counters, enabled with {@code -Dprojects.overlay.perf=true}. When off, the JIT drops
 * the calls entirely. Render thread only.
 */
public final class Perf {
    public static final boolean ENABLED = Boolean.getBoolean("projects.overlay.perf");

    private static long toastNanos;
    private static long toastCalls;
    private static long panelNanos;
    private static long panelCalls;
    private static long layoutsMade;
    private static long layoutsReused;

    private Perf() {
    }

    public static long start() {
        return ENABLED ? System.nanoTime() : 0L;
    }

    public static void toast(long started) {
        if (ENABLED) {
            toastNanos += System.nanoTime() - started;
            toastCalls++;
        }
    }

    public static void panel(long started) {
        if (ENABLED) {
            panelNanos += System.nanoTime() - started;
            panelCalls++;
        }
    }

    /** The game laid out a line of the overlay's text (looked up and placed its glyphs). */
    public static void layoutMade() {
        if (ENABLED) {
            layoutsMade++;
        }
    }

    /** A line drew with the layout it kept from an earlier frame. */
    public static void layoutReused() {
        if (ENABLED) {
            layoutsReused++;
        }
    }

    /**
     * Returns {toastNanos, toastCalls, panelNanos, panelCalls, layoutsMade, layoutsReused} and resets the
     * counters.
     */
    public static long[] drain() {
        long[] out = {toastNanos, toastCalls, panelNanos, panelCalls, layoutsMade, layoutsReused};
        toastNanos = 0;
        toastCalls = 0;
        panelNanos = 0;
        panelCalls = 0;
        layoutsMade = 0;
        layoutsReused = 0;
        return out;
    }
}
