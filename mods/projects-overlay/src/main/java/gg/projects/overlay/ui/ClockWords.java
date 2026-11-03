package gg.projects.overlay.ui;

import java.time.Instant;
import java.time.LocalDate;
import java.time.ZoneId;
import java.time.ZonedDateTime;
import java.time.format.TextStyle;
import java.util.Locale;

/**
 * Until when something holds, as the launcher says it (OVL-SOC 2; its {@code untilText} and {@code timeDative} in
 * {@code features/social/crew-notify.tsx} and {@code mute.ts}): "17:30'a kadar", "yarın 08:00'e kadar", "5 Kasım
 * 08:00'e kadar"; in English "until 17:30", "until 08:00 tomorrow", "until 5 November, 08:00". The Turkish ending
 * follows how the time is read: the minutes' last word, or the hour's on the hour ("14:30'a", "14:00'e", "16:00'ya").
 */
public final class ClockWords {
    /** Bir, iki, üç… dokuz: their dative endings ("bire", "ikiye", "üçe"). */
    private static final String[] ONES = {"a", "e", "ye", "e", "e", "e", "ya", "ye", "e", "a"};
    /** On, yirmi, otuz, kırk, elli. */
    private static final String[] TENS = {"a", "a", "ye", "a", "a", "ye"};
    private static final Locale TR = Locale.forLanguageTag("tr");

    private ClockWords() {
    }

    /** The Turkish dative ending after a clock time's apostrophe. */
    public static String dative(int hour, int minute) {
        int read = minute == 0 ? hour : minute;
        int ones = read % 10;
        // Sıfır (00:00) reads like dokuz: "sıfıra".
        if (read == 0) {
            return "a";
        }
        return ones != 0 ? ONES[ones] : TENS[(read / 10) % 10];
    }

    /** Until {@code untilMs} (Unix ms) as seen at {@code nowMs}, on this computer's clock and calendar. */
    public static String until(long untilMs, long nowMs) {
        return until(untilMs, nowMs, ZoneId.systemDefault(), Texts.turkish());
    }

    static String until(long untilMs, long nowMs, ZoneId zone, boolean turkish) {
        ZonedDateTime end = Instant.ofEpochMilli(untilMs).atZone(zone);
        LocalDate today = Instant.ofEpochMilli(nowMs).atZone(zone).toLocalDate();
        LocalDate day = end.toLocalDate();
        String time = String.format(Locale.ROOT, "%02d:%02d", end.getHour(), end.getMinute());
        String ending = dative(end.getHour(), end.getMinute());
        if (!day.isAfter(today)) {
            return turkish ? time + "'" + ending + " kadar" : "until " + time;
        }
        if (day.equals(today.plusDays(1))) {
            return turkish ? "yarın " + time + "'" + ending + " kadar" : "until " + time + " tomorrow";
        }
        String date = day.getDayOfMonth() + " " + day.getMonth().getDisplayName(TextStyle.FULL,
            turkish ? TR : Locale.ENGLISH);
        return turkish ? date + " " + time + "'" + ending + " kadar" : "until " + date + ", " + time;
    }
}
