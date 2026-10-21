package gg.projects.overlay.dev;

import com.google.gson.JsonObject;
import java.util.Arrays;

/** Frame intervals recorded by the dev mixin on {@code Minecraft.runTick}. Render thread only. */
public final class FrameClock {
    private static final long[] SAMPLES = new long[200_000];
    private static int count;
    private static long last;
    private static boolean recording;

    private FrameClock() {
    }

    public static void onFrameStart() {
        long now = System.nanoTime();
        if (recording && last != 0 && count < SAMPLES.length) {
            SAMPLES[count++] = now - last;
        }
        last = now;
    }

    public static void start() {
        count = 0;
        recording = true;
    }

    /** Stops recording and summarizes the frame times in milliseconds. */
    public static JsonObject stop() {
        recording = false;
        long[] s = Arrays.copyOf(SAMPLES, count);
        Arrays.sort(s);
        JsonObject o = new JsonObject();
        o.addProperty("frames", s.length);
        if (s.length == 0) {
            return o;
        }
        double sum = 0;
        for (long v : s) {
            sum += v;
        }
        double mean = sum / s.length;
        o.addProperty("meanMs", round(mean / 1e6));
        o.addProperty("p50Ms", round(s[(int) (s.length * 0.50)] / 1e6));
        o.addProperty("p95Ms", round(s[Math.min(s.length - 1, (int) (s.length * 0.95))] / 1e6));
        o.addProperty("p99Ms", round(s[Math.min(s.length - 1, (int) (s.length * 0.99))] / 1e6));
        o.addProperty("maxMs", round(s[s.length - 1] / 1e6));
        o.addProperty("fps", round(1e9 / mean));
        return o;
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
