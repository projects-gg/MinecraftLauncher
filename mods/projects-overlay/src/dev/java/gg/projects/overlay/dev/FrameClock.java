package gg.projects.overlay.dev;

import com.google.gson.JsonObject;
import java.util.Arrays;

/**
 * Frame intervals recorded by the dev mixin on {@code Minecraft.runTick}, into one of two slots, so that two ways
 * of drawing can take turns in short slices and be compared without drift between them. Render thread only.
 */
public final class FrameClock {
    private static final int SLOTS = 2;
    private static final long[][] SAMPLES = new long[SLOTS][200_000];
    private static final int[] COUNT = new int[SLOTS];
    private static long last;
    private static int slot = -1;

    private FrameClock() {
    }

    public static void onFrameStart() {
        long now = System.nanoTime();
        int s = slot;
        if (s >= 0 && last != 0 && COUNT[s] < SAMPLES[s].length) {
            SAMPLES[s][COUNT[s]++] = now - last;
        }
        last = now;
    }

    /** Clears both slots and records into slot 0. */
    public static void start() {
        clear();
        slot = 0;
    }

    /** Clears both slots; nothing is recorded until {@link #record}. */
    public static void clear() {
        Arrays.fill(COUNT, 0);
        slot = -1;
    }

    /** Records into {@code s} from now on (keeps what both slots hold), or nothing for -1. */
    public static void record(int s) {
        slot = s;
    }

    /** Frames recorded into slot {@code s} so far. */
    public static int count(int s) {
        return COUNT[s];
    }

    /** Stops recording and summarizes slot 0. */
    public static JsonObject stop() {
        slot = -1;
        return summary(0);
    }

    /** Summarizes a slot's frame times in milliseconds. */
    public static JsonObject summary(int s) {
        long[] v = Arrays.copyOf(SAMPLES[s], COUNT[s]);
        Arrays.sort(v);
        JsonObject o = new JsonObject();
        o.addProperty("frames", v.length);
        if (v.length == 0) {
            return o;
        }
        double sum = 0;
        for (long x : v) {
            sum += x;
        }
        double mean = sum / v.length;
        o.addProperty("meanMs", round(mean / 1e6));
        o.addProperty("p50Ms", round(v[(int) (v.length * 0.50)] / 1e6));
        o.addProperty("p95Ms", round(v[Math.min(v.length - 1, (int) (v.length * 0.95))] / 1e6));
        o.addProperty("p99Ms", round(v[Math.min(v.length - 1, (int) (v.length * 0.99))] / 1e6));
        o.addProperty("maxMs", round(v[v.length - 1] / 1e6));
        o.addProperty("fps", round(1e9 / mean));
        return o;
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
