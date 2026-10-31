package gg.projects.overlay.chat;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.lang.Dictionary;
import gg.projects.overlay.lang.Language;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.time.ZonedDateTime;
import org.junit.jupiter.api.Test;

/** CHAT-UX: the day lines, clock and groups, the same cases as the launcher's chat-time.test.ts. */
class ChatTimeTest {
    private static final ZoneId ISTANBUL = ZoneId.of("Europe/Istanbul");
    private static final ZoneId BERLIN = ZoneId.of("Europe/Berlin");
    private static final ZoneId NEW_YORK = ZoneId.of("America/New_York");
    private static final ZoneId AUCKLAND = ZoneId.of("Pacific/Auckland");
    private static final long MINUTE = 60_000L;
    private static final long HOUR = 60 * MINUTE;
    /** The jar's own words, as the overlay shows them. */
    private static final ChatTime.Words TR = words(Language.TURKISH);
    private static final ChatTime.Words EN = words(Language.ENGLISH);

    private static ChatTime.Words words(Language language) {
        return (key, args) -> Dictionary.shared().format(language, "projects_overlay." + key, args);
    }

    /** A wall time in {@code zone} (month from 1). */
    private static long at(ZoneId zone, int year, int month, int day, int hour, int minute) {
        return ZonedDateTime.of(LocalDateTime.of(year, month, day, hour, minute), zone).toInstant().toEpochMilli();
    }

    @Test
    void saysTodayYesterdayTwoToSixDaysAgoThenTheDate() {
        long now = at(ISTANBUL, 2026, 10, 31, 12, 0);
        assertEquals("Bugün", ChatTime.dayLine(at(ISTANBUL, 2026, 10, 31, 0, 5), now, ISTANBUL, TR));
        assertEquals("Dün", ChatTime.dayLine(at(ISTANBUL, 2026, 10, 30, 23, 59), now, ISTANBUL, TR));
        assertEquals("2 gün önce", ChatTime.dayLine(at(ISTANBUL, 2026, 10, 29, 12, 0), now, ISTANBUL, TR));
        assertEquals("3 gün önce", ChatTime.dayLine(at(ISTANBUL, 2026, 10, 28, 9, 0), now, ISTANBUL, TR));
        assertEquals("6 gün önce", ChatTime.dayLine(at(ISTANBUL, 2026, 10, 25, 23, 59), now, ISTANBUL, TR));
        assertEquals("24 Ekim", ChatTime.dayLine(at(ISTANBUL, 2026, 10, 24, 23, 59), now, ISTANBUL, TR));
        assertEquals("3 Ocak", ChatTime.dayLine(at(ISTANBUL, 2026, 1, 3, 8, 0), now, ISTANBUL, TR));
        assertEquals("31 Aralık 2025", ChatTime.dayLine(at(ISTANBUL, 2025, 12, 31, 22, 0), now, ISTANBUL, TR));
    }

    @Test
    void speaksBritishEnglish() {
        ZoneId london = ZoneId.of("Europe/London");
        long now = at(london, 2026, 10, 31, 12, 0);
        assertEquals("Today", ChatTime.dayLine(at(london, 2026, 10, 31, 9, 0), now, london, EN));
        assertEquals("Yesterday", ChatTime.dayLine(at(london, 2026, 10, 30, 9, 0), now, london, EN));
        assertEquals("3 days ago", ChatTime.dayLine(at(london, 2026, 10, 28, 9, 0), now, london, EN));
        assertEquals("24 October", ChatTime.dayLine(at(london, 2026, 10, 24, 9, 0), now, london, EN));
        assertEquals("31 December 2025", ChatTime.dayLine(at(london, 2025, 12, 31, 9, 0), now, london, EN));
        assertEquals("Wednesday, 28 October 2026 at 14:05", ChatTime.full(at(london, 2026, 10, 28, 14, 5), london, EN));
    }

    @Test
    void countsCalendarDaysNotHoursOnBothSidesOfMidnight() {
        long midnight = at(ISTANBUL, 2026, 10, 31, 0, 0);
        assertEquals("Dün", ChatTime.dayLine(midnight - 1, midnight, ISTANBUL, TR));
        assertEquals("Bugün", ChatTime.dayLine(midnight, midnight, ISTANBUL, TR));
        // Forty minutes back, yet yesterday; almost three days of hours, two calendar days.
        assertEquals("Dün", ChatTime.dayLine(at(ISTANBUL, 2026, 10, 30, 23, 30), at(ISTANBUL, 2026, 10, 31, 0, 10),
            ISTANBUL, TR));
        assertEquals(2, ChatTime.daysAgo(at(ISTANBUL, 2026, 10, 29, 0, 10), at(ISTANBUL, 2026, 10, 31, 23, 50),
            ISTANBUL));
        assertEquals(midnight + 24 * HOUR, ChatTime.nextDay(midnight - 1 + 24 * HOUR, ISTANBUL));
        assertEquals(midnight, ChatTime.nextDay(midnight - 1, ISTANBUL));
    }

    @Test
    void turnsTheYearWithTheCalendar() {
        long now = at(ISTANBUL, 2027, 1, 1, 0, 30);
        assertEquals("Dün", ChatTime.dayLine(at(ISTANBUL, 2026, 12, 31, 23, 50), now, ISTANBUL, TR));
        assertEquals("6 gün önce", ChatTime.dayLine(at(ISTANBUL, 2026, 12, 26, 12, 0), now, ISTANBUL, TR));
        assertEquals("25 Aralık 2026", ChatTime.dayLine(at(ISTANBUL, 2026, 12, 25, 12, 0), now, ISTANBUL, TR));
    }

    @Test
    void readsATimeAheadOfTheClockAsToday() {
        long now = at(ISTANBUL, 2026, 10, 31, 23, 59);
        assertEquals(-1, ChatTime.daysAgo(now + 2 * MINUTE, now, ISTANBUL));
        assertEquals("Bugün", ChatTime.dayLine(now + 2 * MINUTE, now, ISTANBUL, TR));
    }

    @Test
    void keepsSummerTimesLongAndShortDaysWhole() {
        // 25 October 2026 is 25 hours long in Berlin; 29 March 23.
        long now = at(BERLIN, 2026, 10, 26, 0, 30);
        assertEquals("2 gün önce", ChatTime.dayLine(at(BERLIN, 2026, 10, 24, 23, 30), now, BERLIN, TR));
        assertEquals("Dün", ChatTime.dayLine(at(BERLIN, 2026, 10, 25, 0, 10), now, BERLIN, TR));
        assertEquals("Dün", ChatTime.dayLine(at(BERLIN, 2026, 10, 25, 23, 59), now, BERLIN, TR));
        // The clocks went back at 03:00: 02:30 came twice, both on 25 October.
        long first = java.time.Instant.parse("2026-10-25T00:30:00Z").toEpochMilli();
        long second = java.time.Instant.parse("2026-10-25T01:30:00Z").toEpochMilli();
        assertEquals("02:30", ChatTime.clock(first, BERLIN));
        assertEquals("02:30", ChatTime.clock(second, BERLIN));
        assertEquals(ChatTime.day(first, BERLIN), ChatTime.day(second, BERLIN));
        long morning = at(BERLIN, 2026, 3, 29, 0, 30);
        long night = at(BERLIN, 2026, 3, 29, 23, 30);
        assertEquals(22 * HOUR, night - morning);
        assertEquals("Bugün", ChatTime.dayLine(morning, night, BERLIN, TR));
        assertEquals("Dün", ChatTime.dayLine(at(BERLIN, 2026, 3, 28, 23, 59), night, BERLIN, TR));
        // The 25-hour day's next day still starts at its own midnight.
        assertEquals(at(BERLIN, 2026, 10, 26, 0, 0), ChatTime.nextDay(at(BERLIN, 2026, 10, 25, 12, 0), BERLIN));
    }

    @Test
    void putsOneMomentOnEachReadersOwnDayAndClock() {
        long moment = java.time.Instant.parse("2026-10-31T02:30:00Z").toEpochMilli();
        long later = java.time.Instant.parse("2026-10-31T12:00:00Z").toEpochMilli();
        assertEquals("Bugün", ChatTime.dayLine(moment, later, ISTANBUL, TR));
        assertEquals("05:30", ChatTime.clock(moment, ISTANBUL));
        assertEquals("Dün", ChatTime.dayLine(moment, later, NEW_YORK, TR));
        assertEquals("22:30", ChatTime.clock(moment, NEW_YORK));
        // 15:30 on 31 October in Auckland, read at 01:00 on 1 November.
        assertEquals("Dün", ChatTime.dayLine(moment, later, AUCKLAND, TR));
        assertEquals("15:30", ChatTime.clock(moment, AUCKLAND));
    }

    @Test
    void writesTheClockAndTheFullTime() {
        assertEquals("00:05", ChatTime.clock(at(ISTANBUL, 2026, 10, 28, 0, 5), ISTANBUL));
        assertEquals("14:05", ChatTime.clock(at(ISTANBUL, 2026, 10, 28, 14, 5), ISTANBUL));
        assertEquals("28 Ekim 2026 Çarşamba 14:05", ChatTime.full(at(ISTANBUL, 2026, 10, 28, 14, 5), ISTANBUL, TR));
        assertEquals("31 Aralık 2025 Çarşamba 23:59", ChatTime.full(at(ISTANBUL, 2025, 12, 31, 23, 59), ISTANBUL, TR));
    }

    @Test
    void groupsOneSendersMessagesUpToFiveMinutesApartOnOneDay() {
        long t = at(ISTANBUL, 2026, 10, 31, 14, 0);
        assertTrue(ChatTime.together(t, t + 5 * MINUTE, ISTANBUL));
        assertFalse(ChatTime.together(t, t + 5 * MINUTE + 1, ISTANBUL));
        // A message the launcher holds a little out of order still joins.
        assertTrue(ChatTime.together(t, t - MINUTE, ISTANBUL));
        // Two minutes apart across midnight: two days, two groups.
        assertFalse(ChatTime.together(at(ISTANBUL, 2026, 10, 30, 23, 59), at(ISTANBUL, 2026, 10, 31, 0, 1),
            ISTANBUL));
    }
}
