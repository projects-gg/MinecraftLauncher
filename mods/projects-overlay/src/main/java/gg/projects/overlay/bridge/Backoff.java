package gg.projects.overlay.bridge;

/** Reconnect delays: quick after a drop, then slower, capped at a minute (one loopback attempt per minute). */
final class Backoff {
    private static final long[] STEPS_MS = {1_000, 2_000, 4_000, 8_000, 15_000, 30_000, 60_000};

    private Backoff() {
    }

    /**
     * Delay before reconnect attempt {@code failures} (0 for the first retry), spread by up to 20% either way.
     *
     * @param jitter a value in [-1, 1]
     */
    static long delayMs(int failures, double jitter) {
        long base = STEPS_MS[Math.max(0, Math.min(failures, STEPS_MS.length - 1))];
        double spread = Math.max(-1.0, Math.min(1.0, jitter)) * 0.2;
        return Math.round(base * (1.0 + spread));
    }
}
