package gg.projects.overlay.ui;

/**
 * How long a friend has been playing (CREW-PLUS; the launcher's {@code features/social/played.ts}), coarsely as "son
 * görülme" is: nothing under five minutes, under an hour in five-minute steps ("25 dk"), then whole hours ("1 sa"),
 * always rounded down. The launcher sends when the session began only for a friend who shares "last seen".
 */
public final class PlayTime {
    /** Under this long nothing is said (seconds). */
    public static final long FROM = 300;
    private static final long STEP = 300;
    private static final long HOUR = 3_600;
    /** {@link #bucket}'s hours start here, apart from its minutes. */
    private static final long HOURS = 1_000;

    private PlayTime() {
    }

    /**
     * What the line says for a session that began at {@code sinceMs}, at {@code nowMs}, as one number that changes
     * exactly when the words do: the minutes (5–55), {@code 1000} + the hours, or -1 for nothing (unknown, or under five
     * minutes, or a clock that runs behind).
     */
    public static long bucket(long sinceMs, long nowMs) {
        if (sinceMs <= 0) {
            return -1;
        }
        long elapsed = Math.floorDiv(nowMs - sinceMs, 1000L);
        if (elapsed < FROM) {
            return -1;
        }
        if (elapsed < HOUR) {
            return elapsed / STEP * 5;
        }
        return HOURS + elapsed / HOUR;
    }

    /** "25 dk", "1 sa" for a {@link #bucket}; null for -1. */
    public static String text(long bucket) {
        if (bucket < 0) {
            return null;
        }
        return bucket < HOURS ? Texts.str("friend.played.minutes", bucket)
            : Texts.str("friend.played.hours", bucket - HOURS);
    }

    /** "25 dakikadır oyunda", for the narrator; null when nothing is said. */
    public static String spoken(long sinceMs, long nowMs) {
        long bucket = bucket(sinceMs, nowMs);
        if (bucket < 0) {
            return null;
        }
        if (bucket < HOURS) {
            return Texts.str("friend.played.spoken_minutes", bucket);
        }
        long hours = bucket - HOURS;
        return hours == 1 ? Texts.str("friend.played.spoken_hours_one")
            : Texts.str("friend.played.spoken_hours", hours);
    }
}
