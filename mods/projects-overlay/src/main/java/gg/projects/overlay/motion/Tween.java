package gg.projects.overlay.motion;

/**
 * A value that moves to a target over a fixed time on a curve (fades, mostly). A new target starts from
 * the value shown right now, so an interrupted fade turns around without a jump. Times are
 * milliseconds from any monotonic clock. No allocation.
 */
public final class Tween {
    private float from;
    private float to;
    private long start;
    private long duration;
    private Ease ease = Ease.OUT;

    public Tween(float initial) {
        from = initial;
        to = initial;
    }

    /** Animates to {@code target}; nothing happens when it is already heading there. */
    public void animateTo(float target, long now, long durationMs, Ease curve) {
        if (target == to) {
            return;
        }
        from = value(now);
        to = target;
        start = now;
        duration = Math.max(0L, durationMs);
        ease = curve;
    }

    public void snap(float value) {
        from = value;
        to = value;
        duration = 0L;
    }

    public float target() {
        return to;
    }

    public float value(long now) {
        if (duration <= 0L || now >= start + duration) {
            return to;
        }
        if (now <= start) {
            return from;
        }
        float t = (float) (now - start) / duration;
        return from + (to - from) * ease.at(t);
    }

    public boolean done(long now) {
        return duration <= 0L || now >= start + duration;
    }
}
