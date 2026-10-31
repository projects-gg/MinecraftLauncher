package gg.projects.overlay.chat;

import java.time.Instant;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.time.ZoneId;

/**
 * Chat times (CHAT-UX; the launcher's {@code chat-time.ts} and the phone keep the same rules): the day line over the
 * first message of each local day ("Bugün", "Dün", "3 gün önce" for two to six days, then "28 Ekim", with the year
 * when it is another year: "28 Ekim 2025"), the group's clock time ("14:05", 24 hours in both languages), the full
 * time the pointer resting on a message shows ("28 Ekim 2026 Çarşamba 14:05") and which messages read as one group.
 * Days are calendar days in the player's zone, never 24-hour spans, so summer time and a message just before midnight
 * read right; a time a little ahead of this computer's clock reads as today. The words come from the overlay's
 * dictionary ({@link Words}: {@code chat.day.*}, {@code chat.month.*}, {@code chat.weekday.*}, {@code chat.time.full}).
 * Pure: the list works them out when it lays out (ChatTimeTest).
 */
public final class ChatTime {
    /** One sender's messages on one local day, each at most this long after the one before, read as one group. */
    public static final long GROUP_GAP_MS = 5 * 60_000L;
    /** Days a day line counts back in words ("6 gün önce"); from the seventh it says the date. */
    public static final int DAYS_IN_WORDS = 6;

    /** The overlay's words in the player's language ({@code Texts::str}): a key without its prefix, its slots. */
    @FunctionalInterface
    public interface Words {
        String get(String key, Object... args);
    }

    private ChatTime() {
    }

    /** The local calendar day of {@code at} in {@code zone}: whole days since 1 January 1970 on that calendar. */
    public static long day(long at, ZoneId zone) {
        return date(at, zone).toEpochDay();
    }

    /** Calendar days from {@code at}'s day to {@code now}'s: 0 today, 1 yesterday; below 0 ahead of the clock. */
    public static long daysAgo(long at, long now, ZoneId zone) {
        return day(now, zone) - day(at, zone);
    }

    /** Two messages of one sender's group: on one local day, the second at most five minutes from the first. */
    public static boolean together(long first, long second, ZoneId zone) {
        return Math.abs(second - first) <= GROUP_GAP_MS && day(first, zone) == day(second, zone);
    }

    /** The moment the local day after {@code now}'s begins (the day lines are laid out again then). */
    public static long nextDay(long now, ZoneId zone) {
        return date(now, zone).plusDays(1).atStartOfDay(zone).toInstant().toEpochMilli();
    }

    /** The day line: "Bugün", "Dün", "3 gün önce", "28 Ekim", "28 Ekim 2025" / "Today", …, "28 October 2025". */
    public static String dayLine(long at, long now, ZoneId zone, Words words) {
        long ago = daysAgo(at, now, zone);
        if (ago <= 0) {
            return words.get("chat.day.today");
        }
        if (ago == 1) {
            return words.get("chat.day.yesterday");
        }
        if (ago <= DAYS_IN_WORDS) {
            return words.get("chat.day.ago", ago);
        }
        LocalDate d = date(at, zone);
        String month = words.get("chat.month." + d.getMonthValue());
        return d.getYear() == date(now, zone).getYear() ? words.get("chat.day.date", d.getDayOfMonth(), month)
            : words.get("chat.day.date_year", d.getDayOfMonth(), month, d.getYear());
    }

    /** A group's time: "14:05", "00:30". */
    public static String clock(long at, ZoneId zone) {
        LocalDateTime t = LocalDateTime.ofInstant(Instant.ofEpochMilli(at), zone);
        return two(t.getHour()) + ":" + two(t.getMinute());
    }

    /** The full time: "28 Ekim 2026 Çarşamba 14:05" / "Wednesday, 28 October 2026 at 14:05". */
    public static String full(long at, ZoneId zone, Words words) {
        LocalDate d = date(at, zone);
        String date = words.get("chat.day.date_year", d.getDayOfMonth(), words.get("chat.month." + d.getMonthValue()),
            d.getYear());
        return words.get("chat.time.full", date, words.get("chat.weekday." + d.getDayOfWeek().getValue()),
            clock(at, zone));
    }

    private static LocalDate date(long at, ZoneId zone) {
        return LocalDate.ofInstant(Instant.ofEpochMilli(at), zone);
    }

    private static String two(int n) {
        return (n < 10 ? "0" : "") + n;
    }
}
