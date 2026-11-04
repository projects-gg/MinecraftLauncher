package gg.projects.overlay.ui;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * OVL-AVATARS: whose pictures the faces asked the launcher for, what it answered and which are kept. No Minecraft
 * here, so it is tested on its own; {@link Avatars} decodes, uploads and draws. {@code T} is an uploaded picture,
 * {@code P} a decoding on its way.
 *
 * <ul>
 * <li>A face asks for its person the first time it is drawn; the asks go out together, every half second at most
 * ({@link Avatars#tick}). An ask with no answer after {@link #ASK_AGAIN_MS} goes again if its face still shows (the
 * launcher did not hear it).</li>
 * <li>The launcher answers with the picture's version and the picture (none: the letter), and tells a change by
 * itself; an older answer coming after a newer one is dropped.</li>
 * <li>A picture nobody drew for {@link #KEEP_MS} gives its texture back (16 KB each; its few kilobytes of PNG stay,
 * so it comes back without asking); at most {@link #CAP} people are kept, those drawn longest ago go first.</li>
 * <li>A picture the launcher could not have (offline, refused) is asked for again after {@link #RETRY_MS}, if its
 * face still shows.</li>
 * <li>Without the launcher every face asks again once it is back: a new connection tells no changes of the old.</li>
 * </ul>
 */
final class AvatarBook<T, P> {
    /** People kept at most. */
    static final int CAP = 256;
    /** A texture nobody drew for this long is given back (the panel reopened within it shows every picture at once). */
    static final long KEEP_MS = 10 * 60_000;
    /** A picture that could not be had is asked for again after this. */
    static final long RETRY_MS = 10 * 60_000;
    /** An ask the launcher did not answer goes again after this, if its face still shows. */
    static final long ASK_AGAIN_MS = 30_000;
    /** How often {@link #sweep} looks. */
    static final long SWEEP_MS = 1_000;
    /** The launcher's version for no picture. */
    static final String NONE = "-";

    /** What the book lets go of: a texture to give back, a decoding to drop (its picture is freed once it lands). */
    interface Sink<T, P> {
        void release(T texture);

        void abandon(P pending);
    }

    /** One person. */
    static final class Entry<T, P> {
        final String id;
        /** Asked for on this connection (the answer pending or in). */
        boolean asked;
        /** The newest ask went out (else it waits for the next frame), when, and the newest answer's count then. */
        boolean sent;
        long askedAt;
        long askN;
        /** The newest answer's count. */
        long n;
        /** The picture's version, null before the first answer. */
        String version;
        /** The picture (a base64 PNG) to upload, null for the letter. */
        String png;
        /** When an answer said there is a picture but brought none, else 0. */
        long missedAt;
        T texture;
        /** When the texture went up (its picture fades in over the letter). */
        long readyAt;
        P pending;
        long drawnAt;

        Entry(String id) {
            this.id = id;
        }
    }

    // In the order people came: a face's lookup moves nothing (the oldest drawn go first by drawnAt).
    private final Map<String, Entry<T, P>> entries = new LinkedHashMap<>();
    private final List<String> wants = new ArrayList<>();
    private long sweptAt;

    boolean isEmpty() {
        return entries.isEmpty();
    }

    int size() {
        return entries.size();
    }

    /** For the tests: {@code id}'s entry, or null. */
    Entry<T, P> entry(String id) {
        return entries.get(id);
    }

    /** {@code id}'s face is drawn at {@code now}: its entry, made and asked for the first time. */
    Entry<T, P> drawn(String id, long now, Sink<T, P> sink) {
        Entry<T, P> e = entries.get(id);
        if (e == null) {
            e = new Entry<>(id);
            e.drawnAt = now;
            entries.put(id, e);
            trim(sink);
        }
        e.drawnAt = now;
        if (!e.asked) {
            e.asked = true;
            e.sent = false;
            wants.add(id);
        } else if (e.sent && e.n == e.askN && now - e.askedAt >= ASK_AGAIN_MS) {
            // Asked and not answered since (the launcher did not hear it): again.
            e.sent = false;
            wants.add(id);
        } else if (e.missedAt != 0 && now - e.missedAt >= RETRY_MS) {
            e.missedAt = 0;
            e.sent = false;
            wants.add(id);
        }
        return e;
    }

    /**
     * The launcher's answer about {@code id} (an {@code avatar} frame): its version, its count and the picture (null:
     * the letter). Someone let go of meanwhile is not kept.
     */
    void answered(String id, String version, long n, String png, long now, Sink<T, P> sink) {
        Entry<T, P> e = entries.get(id);
        if (e == null || n <= e.n) {
            return;
        }
        e.n = n;
        if (!version.equals(e.version)) {
            // Another picture, or none any more: the old one goes.
            drop(e, sink);
            e.version = version;
            e.png = png;
        } else if (png != null && e.png == null && e.texture == null && e.pending == null) {
            // The same picture, had now (asked again after it could not be had).
            e.png = png;
        }
        e.missedAt = png == null && !NONE.equals(version) && e.texture == null ? now : 0;
    }

    /** Up to {@code max} people to ask for now, taken off the list (their asks go out at {@code now}). */
    List<String> takeWants(int max, long now) {
        if (wants.isEmpty()) {
            return List.of();
        }
        int count = Math.min(max, wants.size());
        List<String> taken = new ArrayList<>(wants.subList(0, count));
        wants.subList(0, count).clear();
        for (String id : taken) {
            Entry<T, P> e = entries.get(id);
            if (e != null) {
                e.sent = true;
                e.askedAt = now;
                e.askN = e.n;
            }
        }
        return taken;
    }

    /** These asks could not go: asked for again when next drawn. */
    void unask(List<String> ids) {
        for (String id : ids) {
            Entry<T, P> e = entries.get(id);
            if (e != null) {
                e.asked = false;
                e.sent = false;
            }
        }
    }

    /** Once a second at most: the textures nobody drew for {@link #KEEP_MS} go back, their pictures stay. */
    void sweep(long now, Sink<T, P> sink) {
        if (now - sweptAt < SWEEP_MS) {
            return;
        }
        sweptAt = now;
        for (Entry<T, P> e : entries.values()) {
            if (now - e.drawnAt >= KEEP_MS) {
                drop(e, sink);
            }
        }
    }

    /** The launcher went away: every face asks again once it is back. */
    void offline() {
        wants.clear();
        for (Entry<T, P> e : entries.values()) {
            e.asked = false;
            e.sent = false;
            e.n = 0;
            e.missedAt = 0;
        }
    }

    /** Every texture goes back (the world closed); the pictures stay. */
    void releaseTextures(Sink<T, P> sink) {
        for (Entry<T, P> e : entries.values()) {
            drop(e, sink);
        }
    }

    /** Over {@link #CAP}: the people drawn longest ago go. */
    private void trim(Sink<T, P> sink) {
        while (entries.size() > CAP) {
            Entry<T, P> oldest = null;
            for (Entry<T, P> e : entries.values()) {
                if (oldest == null || e.drawnAt < oldest.drawnAt) {
                    oldest = e;
                }
            }
            entries.remove(oldest.id);
            wants.remove(oldest.id);
            drop(oldest, sink);
        }
    }

    /**
     * Four ARGB pixels (a 2 x 2 block) as one: their mean weighted by opacity, so a see-through corner neither darkens
     * nor tints the edge beside it.
     */
    static int halve(int p0, int p1, int p2, int p3) {
        int a0 = p0 >>> 24;
        int a1 = p1 >>> 24;
        int a2 = p2 >>> 24;
        int a3 = p3 >>> 24;
        int alpha = a0 + a1 + a2 + a3;
        if (alpha == 0) {
            return 0;
        }
        int r = (((p0 >> 16) & 0xFF) * a0 + ((p1 >> 16) & 0xFF) * a1 + ((p2 >> 16) & 0xFF) * a2
            + ((p3 >> 16) & 0xFF) * a3 + alpha / 2) / alpha;
        int g = (((p0 >> 8) & 0xFF) * a0 + ((p1 >> 8) & 0xFF) * a1 + ((p2 >> 8) & 0xFF) * a2
            + ((p3 >> 8) & 0xFF) * a3 + alpha / 2) / alpha;
        int b = ((p0 & 0xFF) * a0 + (p1 & 0xFF) * a1 + (p2 & 0xFF) * a2 + (p3 & 0xFF) * a3 + alpha / 2) / alpha;
        return ((alpha + 2) / 4) << 24 | r << 16 | g << 8 | b;
    }

    private static <T, P> void drop(Entry<T, P> e, Sink<T, P> sink) {
        if (e.texture != null) {
            sink.release(e.texture);
            e.texture = null;
        }
        if (e.pending != null) {
            sink.abandon(e.pending);
            e.pending = null;
        }
    }
}
