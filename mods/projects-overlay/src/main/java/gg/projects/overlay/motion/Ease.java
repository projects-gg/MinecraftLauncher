package gg.projects.overlay.motion;

/**
 * CSS-style cubic Bézier timing curves, the launcher's own (docs/design.md §5). Solved once into a
 * lookup table, so evaluating one per frame is a table read and a lerp.
 */
public final class Ease {
    /** Entrances: {@code cubic-bezier(0.16, 1, 0.3, 1)}, easeOutExpo. */
    public static final Ease OUT_EXPO = new Ease(0.16, 1.0, 0.3, 1.0);
    /** Sheets and drawers: {@code cubic-bezier(0.32, 0.72, 0, 1)}. */
    public static final Ease DRAWER = new Ease(0.32, 0.72, 0.0, 1.0);
    /** Fades and small state changes: a strong ease-out. */
    public static final Ease OUT = new Ease(0.23, 1.0, 0.32, 1.0);
    public static final Ease LINEAR = new Ease(0.0, 0.0, 1.0, 1.0);

    private static final int SAMPLES = 128;

    private final float[] table = new float[SAMPLES + 1];

    public Ease(double x1, double y1, double x2, double y2) {
        for (int i = 0; i <= SAMPLES; i++) {
            double x = (double) i / SAMPLES;
            table[i] = (float) bezier(solveT(x, x1, x2), y1, y2);
        }
    }

    /** The curve's value at progress {@code t} in [0, 1]. */
    public float at(float t) {
        if (t <= 0f) {
            return 0f;
        }
        if (t >= 1f) {
            return 1f;
        }
        float pos = t * SAMPLES;
        int i = (int) pos;
        float frac = pos - i;
        return table[i] + (table[i + 1] - table[i]) * frac;
    }

    private static double bezier(double t, double p1, double p2) {
        double u = 1.0 - t;
        return 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t;
    }

    private static double slope(double t, double p1, double p2) {
        double u = 1.0 - t;
        return 3.0 * u * u * p1 + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2);
    }

    /** The curve parameter whose x is {@code x}: Newton steps, bisection when the slope is flat. */
    private static double solveT(double x, double x1, double x2) {
        double t = x;
        for (int i = 0; i < 8; i++) {
            double err = bezier(t, x1, x2) - x;
            if (Math.abs(err) < 1e-7) {
                return t;
            }
            double d = slope(t, x1, x2);
            if (Math.abs(d) < 1e-6) {
                break;
            }
            t -= err / d;
        }
        double lo = 0.0;
        double hi = 1.0;
        t = x;
        for (int i = 0; i < 40; i++) {
            double v = bezier(t, x1, x2);
            if (Math.abs(v - x) < 1e-7) {
                break;
            }
            if (v < x) {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.0;
        }
        return t;
    }
}
