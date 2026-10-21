package gg.projects.overlay.motion;

/**
 * A value that follows its target like a damped spring (mass 1). Each step is the exact solution of
 * the spring for the elapsed time, so a long frame never overshoots or explodes, and a new target keeps
 * the current position and speed: motion that is interrupted carries on from where it is. No
 * allocation, render thread only.
 */
public final class Spring {
    /** The launcher's sliding markers (tabs, segmented plates): quick, a hair of overshoot. */
    public static final float SNAPPY_STIFFNESS = 480f;
    public static final float SNAPPY_DAMPING = 38f;
    /** The launcher's gentle spring (sheets settling): no visible overshoot. */
    public static final float GENTLE_STIFFNESS = 160f;
    public static final float GENTLE_DAMPING = 24f;

    private static final float REST_DISTANCE = 0.02f;
    private static final float REST_SPEED = 0.05f;

    private final float omega;
    private final float zeta;
    private float value;
    private float velocity;
    private float target;

    public Spring(float stiffness, float damping, float initial) {
        this.omega = (float) Math.sqrt(stiffness);
        this.zeta = damping / (2f * omega);
        this.value = initial;
        this.target = initial;
    }

    public static Spring snappy(float initial) {
        return new Spring(SNAPPY_STIFFNESS, SNAPPY_DAMPING, initial);
    }

    public static Spring gentle(float initial) {
        return new Spring(GENTLE_STIFFNESS, GENTLE_DAMPING, initial);
    }

    public float value() {
        return value;
    }

    public float target() {
        return target;
    }

    public float velocity() {
        return velocity;
    }

    public void setTarget(float next) {
        target = next;
    }

    /** Jumps to a value and rests there (reduced motion, first layout). */
    public void snap(float next) {
        value = next;
        target = next;
        velocity = 0f;
    }

    /** Moves the value by {@code offset} without changing the target (a list that reflowed under it). */
    public void shift(float offset) {
        value += offset;
    }

    public boolean atRest() {
        return Math.abs(value - target) < REST_DISTANCE && Math.abs(velocity) < REST_SPEED;
    }

    /** Advances by {@code dt} seconds. Returns true while it still moves. */
    public boolean step(float dt) {
        if (dt <= 0f) {
            return !atRest();
        }
        if (atRest()) {
            value = target;
            velocity = 0f;
            return false;
        }
        double x0 = value - target;
        double v0 = velocity;
        double w = omega;
        double x;
        double v;
        if (zeta < 0.9999f) {
            double wd = w * Math.sqrt(1.0 - zeta * zeta);
            double decay = Math.exp(-zeta * w * dt);
            double cos = Math.cos(wd * dt);
            double sin = Math.sin(wd * dt);
            double b = (v0 + zeta * w * x0) / wd;
            x = decay * (x0 * cos + b * sin);
            v = decay * ((b * wd - zeta * w * x0) * cos - (x0 * wd + zeta * w * b) * sin);
        } else if (zeta <= 1.0001f) {
            double decay = Math.exp(-w * dt);
            double b = v0 + w * x0;
            x = (x0 + b * dt) * decay;
            v = (v0 - w * b * dt) * decay;
        } else {
            double root = w * Math.sqrt(zeta * zeta - 1.0);
            double r1 = -zeta * w + root;
            double r2 = -zeta * w - root;
            double c1 = (v0 - r2 * x0) / (r1 - r2);
            double c2 = x0 - c1;
            double e1 = Math.exp(r1 * dt);
            double e2 = Math.exp(r2 * dt);
            x = c1 * e1 + c2 * e2;
            v = c1 * r1 * e1 + c2 * r2 * e2;
        }
        value = (float) (target + x);
        velocity = (float) v;
        if (atRest()) {
            value = target;
            velocity = 0f;
            return false;
        }
        return true;
    }
}
