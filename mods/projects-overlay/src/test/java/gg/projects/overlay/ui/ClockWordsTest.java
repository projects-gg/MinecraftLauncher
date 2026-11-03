package gg.projects.overlay.ui;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.time.LocalDateTime;
import java.time.ZoneId;
import java.time.ZoneOffset;
import org.junit.jupiter.api.Test;

/** OVL-SOC 2: until when a status holds, in the launcher's words ({@code untilText}, {@code timeDative}). */
class ClockWordsTest {
    private static final ZoneId ZONE = ZoneOffset.ofHours(3);

    private static long at(int day, int hour, int minute) {
        return LocalDateTime.of(2026, 11, day, hour, minute).atZone(ZONE).toInstant().toEpochMilli();
    }

    @Test
    void theEndingFollowsHowTheTimeIsRead() {
        assertEquals("a", ClockWords.dative(14, 30), "otuza");
        assertEquals("e", ClockWords.dative(14, 0), "on dörde");
        assertEquals("ya", ClockWords.dative(16, 0), "on altıya");
        assertEquals("ye", ClockWords.dative(12, 0), "on ikiye");
        assertEquals("a", ClockWords.dative(10, 0), "ona");
        assertEquals("ye", ClockWords.dative(9, 20), "yirmiye");
        assertEquals("ye", ClockWords.dative(9, 50), "elliye");
        assertEquals("a", ClockWords.dative(0, 0), "sıfıra");
        assertEquals("e", ClockWords.dative(8, 0), "sekize");
    }

    @Test
    void todayTomorrowAndLaterAreSaidAsTheLauncherSaysThem() {
        long now = at(3, 16, 20);
        assertEquals("17:30'a kadar", ClockWords.until(at(3, 17, 30), now, ZONE, true));
        assertEquals("until 17:30", ClockWords.until(at(3, 17, 30), now, ZONE, false));
        assertEquals("yarın 08:00'e kadar", ClockWords.until(at(4, 8, 0), now, ZONE, true));
        assertEquals("until 08:00 tomorrow", ClockWords.until(at(4, 8, 0), now, ZONE, false));
        assertEquals("5 Kasım 00:20'ye kadar", ClockWords.until(at(5, 0, 20), now, ZONE, true));
        assertEquals("until 5 November, 00:20", ClockWords.until(at(5, 0, 20), now, ZONE, false));
    }
}
