package gg.projects.overlay.toast;

import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.motion.Tween;
import java.util.ArrayList;
import java.util.List;
import java.util.function.UnaryOperator;

/**
 * The toast stack's layout and timing, without any drawing (so it is tested on its own).
 *
 * <p>A conveyor, newest on top: a new toast drops into the top slot, the older ones below glide down as
 * the oldest (at the bottom) leaves, sliding down while it fades. Toasts leave only from the bottom,
 * one after another ({@link #STAGGER_MS} apart), so the stack always reads top to bottom as newest to
 * oldest; a toast whose time is up waits for the ones below it. More than {@link #MAX_VISIBLE} at once
 * push the oldest out straight away. A toast for a conversation or call already on screen updates in
 * place ("Ayşe (3)") and starts its time again. Nor more than its corner has room for ({@link #setRoom}: the
 * screen's height, a minimap in the corner): a new toast pushes the oldest out then too, the newest always shows.
 *
 * <p>Coordinates are GUI pixels relative to the stack's anchor, y downwards: anchored at the bottom the
 * oldest toast ends at 0 and the others sit above it (negative); anchored at the top the newest starts
 * at 0. Positions move on springs (interruptible: a toast pushed down while it still drops in carries
 * on from where it is); opacity on timed fades. Reduced motion keeps the fades and moves nothing.
 *
 * <p>A pinned toast (a voice chat ring, VOICE-O) stays until its own time or {@link #dismiss}: it never waits for the
 * ones below it, the others never wait for it, and it is never pushed out.
 *
 * @param <T> what the drawing side keeps for a toast (its prepared text)
 */
public final class ToastStack<T> {
    public static final int MAX_VISIBLE = 3;
    public static final float GAP = 5f;
    /** A new toast drops from this far above its slot. */
    public static final float ENTER_OFFSET = 10f;
    /** A leaving toast slides this far down while it fades. */
    public static final float EXIT_OFFSET = 14f;
    public static final long FADE_IN_MS = 220;
    public static final long FADE_OUT_MS = 170;
    /** Everything clears quickly when the panel opens. */
    public static final long CLEAR_FADE_MS = 110;
    public static final long REFRESH_MS = 260;
    public static final long STAGGER_MS = 200;
    /**
     * When others must move to make room, a new toast waits this long before it drops in: the older ones
     * start gliding first, so the newcomer never fades in on top of one still in its way.
     */
    public static final long ENTER_DELAY_MS = 70;
    private static final float MAX_STEP_S = 0.05f;

    /** One toast. Fields are read by the drawing side; only the stack changes them. */
    public static final class Entry<T> {
        public final String key;
        private T content;
        private int count = 1;
        private float height;
        private long deadline;
        private boolean leaving;
        private boolean pinned;
        private boolean placed;
        private long enterAt;
        private final Spring y = Spring.snappy(0f);
        private final Tween alpha = new Tween(0f);
        private final Tween refresh = new Tween(1f);

        Entry(String key, T content, float height, long deadline) {
            this.key = key;
            this.content = content;
            this.height = height;
            this.deadline = deadline;
        }

        public T content() {
            return content;
        }

        public int count() {
            return count;
        }

        public float height() {
            return height;
        }

        public boolean leaving() {
            return leaving;
        }

        /** Top edge relative to the anchor, as shown now. */
        public float y() {
            return y.value();
        }

        public float alpha(long now) {
            return alpha.value(now);
        }

        /** 0 right after an update in place, back to 1 over {@link #REFRESH_MS}. */
        public float refresh(long now) {
            return refresh.value(now);
        }

        public long deadline() {
            return deadline;
        }

        /** It stays until its own time or a dismissal (a ring). */
        public boolean pinned() {
            return pinned;
        }
    }

    /** What {@link #push} did. */
    public enum Push { ADDED, UPDATED }

    private final List<Entry<T>> entries = new ArrayList<>();
    private boolean anchoredAtTop;
    private boolean reducedMotion;
    private float room = Float.MAX_VALUE;
    private long lastLeave = Long.MIN_VALUE / 2;
    private long lastUpdate = Long.MIN_VALUE;

    /** Top to bottom: newest first, leaving toasts included until their fade ends. */
    public List<Entry<T>> entries() {
        return entries;
    }

    public boolean isEmpty() {
        return entries.isEmpty();
    }

    /**
     * Every toast's content changed by {@code change} where it stands (KEY-HINT: another hint line). The change keeps
     * the height: nothing moves, nothing starts again.
     */
    public void replaceAll(UnaryOperator<T> change) {
        for (Entry<T> e : entries) {
            e.content = change.apply(e.content);
        }
    }

    /** Toasts that are not leaving. */
    public int visibleCount() {
        int n = 0;
        for (Entry<T> e : entries) {
            if (!e.leaving) {
                n++;
            }
        }
        return n;
    }

    public void setAnchoredAtTop(boolean top) {
        anchoredAtTop = top;
    }

    /** How tall the stack may grow from its anchor, GUI pixels (taken into account by the next {@link #push}). */
    public void setRoom(float room) {
        this.room = room;
    }

    /** The stack's height as laid out: the toasts not leaving, with the gaps between them. */
    float laidOutHeight() {
        float height = 0f;
        int n = 0;
        for (Entry<T> e : entries) {
            if (!e.leaving) {
                height += e.height;
                n++;
            }
        }
        return n == 0 ? 0f : height + GAP * (n - 1);
    }

    /**
     * Shows a toast, or updates the one already showing for {@code key}.
     *
     * @param height its height in GUI pixels
     * @param durationMs how long it stays from now (after any update)
     */
    public Push push(String key, T content, float height, long durationMs, long now) {
        return push(key, content, height, durationMs, now, false);
    }

    /**
     * Shows a toast that stays for {@code durationMs} or until {@link #dismiss}, whatever the others do (a ring), or
     * updates the one showing for {@code key}.
     */
    public Push pin(String key, T content, float height, long durationMs, long now) {
        return push(key, content, height, durationMs, now, true);
    }

    /** The toast for {@code key} fades out now, sliding down as a leaving one does. */
    public void dismiss(String key, long now) {
        for (Entry<T> e : entries) {
            if (!e.leaving && e.key.equals(key)) {
                leave(e, now, FADE_OUT_MS, true);
                return;
            }
        }
    }

    private Push push(String key, T content, float height, long durationMs, long now, boolean pinned) {
        for (Entry<T> e : entries) {
            if (!e.leaving && e.key.equals(key)) {
                e.content = content;
                e.count++;
                e.height = height;
                e.deadline = now + durationMs;
                e.pinned = pinned;
                e.refresh.snap(0f);
                e.refresh.animateTo(1f, now, REFRESH_MS, Ease.OUT);
                return Push.UPDATED;
            }
        }
        Entry<T> e = new Entry<>(key, content, height, now + durationMs);
        e.pinned = pinned;
        long delay = visibleCount() > 0 && !reducedMotion ? ENTER_DELAY_MS : 0L;
        entries.add(0, e);
        e.enterAt = now + delay;
        e.deadline += delay;
        e.alpha.animateTo(1f, e.enterAt, FADE_IN_MS, Ease.OUT);
        int visible = visibleCount();
        while (visible > MAX_VISIBLE) {
            Entry<T> oldest = bottomUnpinned();
            if (oldest == null) {
                break;
            }
            leave(oldest, now, FADE_OUT_MS, true);
            visible--;
        }
        // Past the corner's room the oldest goes as well (the newest always shows; a ring is never pushed out).
        while (visible > 1 && laidOutHeight() > room) {
            Entry<T> oldest = bottomUnpinned();
            if (oldest == null || oldest == e) {
                break;
            }
            leave(oldest, now, FADE_OUT_MS, true);
            visible--;
        }
        return Push.ADDED;
    }

    /** Every toast fades where it is (the panel opened and shows all of it). */
    public void clear(long now) {
        for (Entry<T> e : entries) {
            if (!e.leaving) {
                leave(e, now, CLEAR_FADE_MS, false);
            }
        }
    }

    /** Drops everything at once (the launcher went away, the world closed). */
    public void reset() {
        entries.clear();
    }

    /**
     * Advances timers, layout and motion to {@code now}.
     *
     * @param paused the game window is not in front: time does not run out meanwhile
     * @param reducedMotion fades only
     * @return true while something still moves or fades (a frame should follow)
     */
    public boolean update(long now, boolean paused, boolean reducedMotion) {
        this.reducedMotion = reducedMotion;
        long dtMs = lastUpdate == Long.MIN_VALUE ? 0L : Math.max(0L, now - lastUpdate);
        lastUpdate = now;
        if (paused && dtMs > 0) {
            for (Entry<T> e : entries) {
                // A ring ends on the launcher's clock, whoever looks.
                if (!e.leaving && !e.pinned) {
                    e.deadline += dtMs;
                }
            }
            lastLeave += dtMs;
        }
        Entry<T> bottom = bottomUnpinned();
        if (bottom != null && now >= bottom.deadline && now - lastLeave >= STAGGER_MS) {
            leave(bottom, now, FADE_OUT_MS, true);
        }
        for (Entry<T> e : entries) {
            if (e.pinned && !e.leaving && now >= e.deadline) {
                leave(e, now, FADE_OUT_MS, true);
            }
        }
        layout(reducedMotion);
        float dt = Math.min(MAX_STEP_S, dtMs / 1000f);
        boolean moving = false;
        for (int i = entries.size() - 1; i >= 0; i--) {
            Entry<T> e = entries.get(i);
            if (reducedMotion) {
                e.y.snap(e.y.target());
            } else if (now < e.enterAt) {
                moving = true;
            } else if (e.y.step(dt)) {
                moving = true;
            }
            if (e.leaving && e.alpha.done(now)) {
                entries.remove(i);
            } else if (!e.alpha.done(now) || !e.refresh.done(now)) {
                moving = true;
            }
        }
        return moving;
    }

    private void layout(boolean reducedMotion) {
        float edge = 0f;
        if (anchoredAtTop) {
            for (Entry<T> e : entries) {
                if (!e.leaving) {
                    place(e, edge, reducedMotion);
                    edge += e.height + GAP;
                }
            }
        } else {
            for (int i = entries.size() - 1; i >= 0; i--) {
                Entry<T> e = entries.get(i);
                if (!e.leaving) {
                    place(e, edge - e.height, reducedMotion);
                    edge -= e.height + GAP;
                }
            }
        }
    }

    private void place(Entry<T> e, float top, boolean reducedMotion) {
        if (!e.placed) {
            e.placed = true;
            e.y.snap(reducedMotion ? top : top - ENTER_OFFSET);
        }
        e.y.setTarget(top);
    }

    private void leave(Entry<T> e, long now, long fadeMs, boolean slide) {
        e.leaving = true;
        e.alpha.animateTo(0f, now, fadeMs, Ease.OUT);
        e.y.setTarget(e.y.target() + (slide && !reducedMotion ? EXIT_OFFSET : 0f));
        lastLeave = now;
    }

    /** Sets the motion choice before the first {@link #update} (pushes may come first). */
    public void setReducedMotion(boolean reduced) {
        reducedMotion = reduced;
    }

    /** The oldest toast that is neither leaving nor pinned: the conveyor's next to go. */
    private Entry<T> bottomUnpinned() {
        for (int i = entries.size() - 1; i >= 0; i--) {
            Entry<T> e = entries.get(i);
            if (!e.leaving && !e.pinned) {
                return e;
            }
        }
        return null;
    }
}
