package gg.projects.overlay.voice;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Who the HUD's corner shows as speaking (VOICE-O; voice.md §5.3): someone fades in as they start (100 ms), stays while
 * they speak and a second after (a breath between words never blinks the row), then fades out (300 ms). Opacity only,
 * in the order they started. Nothing to draw once the last one faded: the corner draws nothing then. Pure (times are
 * passed in), so it is tested on its own.
 */
public final class Speakers {
    public static final long IN_MS = 100;
    /** A speaker stays this long after they stop. */
    public static final long HOLD_MS = 1_000;
    public static final long OUT_MS = 300;
    /** Rows the corner shows at most; the rest are counted ("+2"). */
    public static final int MAX_ROWS = 4;

    /** Someone shown, at an opacity. */
    public record Shown(String id, float alpha) {
    }

    /** When they started, and when they stopped (0 while they speak). */
    private static final class Entry {
        long since;
        long stopped;

        Entry(long since) {
            this.since = since;
        }
    }

    private final Map<String, Entry> entries = new LinkedHashMap<>();

    /** Who speaks from {@code now} on. */
    public void update(Collection<String> speaking, long now) {
        for (Map.Entry<String, Entry> e : entries.entrySet()) {
            Entry entry = e.getValue();
            boolean speaks = speaking.contains(e.getKey());
            if (!speaks && entry.stopped == 0) {
                entry.stopped = now;
            } else if (speaks && entry.stopped != 0) {
                // Back before they faded: they come in again from where the fade has them.
                float a = alpha(entry, now);
                entry.stopped = 0;
                entry.since = now - Math.round(a * IN_MS);
            }
        }
        for (String id : speaking) {
            entries.computeIfAbsent(id, k -> new Entry(now));
        }
    }

    /** Everyone gone at once (out of the room, the launcher went away). */
    public void clear() {
        entries.clear();
    }

    /** Nothing to draw at {@code now} (the ones faded out are forgotten). */
    public boolean idle(long now) {
        prune(now);
        return entries.isEmpty();
    }

    /** The rows to draw at {@code now}, at most {@link #MAX_ROWS}, in the order they started. */
    public List<Shown> shown(long now) {
        prune(now);
        List<Shown> out = new ArrayList<>(Math.min(MAX_ROWS, entries.size()));
        for (Map.Entry<String, Entry> e : entries.entrySet()) {
            if (out.size() >= MAX_ROWS) {
                break;
            }
            out.add(new Shown(e.getKey(), alpha(e.getValue(), now)));
        }
        return out;
    }

    /** How many speak (or fade) beyond the rows shown. */
    public int beyond(long now) {
        prune(now);
        return Math.max(0, entries.size() - MAX_ROWS);
    }

    private void prune(long now) {
        Iterator<Entry> it = entries.values().iterator();
        while (it.hasNext()) {
            Entry e = it.next();
            if (e.stopped != 0 && now - e.stopped >= HOLD_MS + OUT_MS) {
                it.remove();
            }
        }
    }

    private static float alpha(Entry e, long now) {
        float in = Math.min(1f, Math.max(0f, (now - e.since) / (float) IN_MS));
        if (e.stopped == 0) {
            return in;
        }
        float out = 1f - Math.min(1f, Math.max(0f, (now - e.stopped - HOLD_MS) / (float) OUT_MS));
        return Math.min(in, out);
    }
}
