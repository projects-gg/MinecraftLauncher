package gg.projects.overlay.ui;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.time.LocalDateTime;
import java.time.ZoneId;
import java.util.List;
import org.junit.jupiter.api.Test;

/** The plan form's days and times, as the launcher's {@code crew-plans.ts} offers them (OVL-SOC2). */
class PlanWhenTest {
    private static final ZoneId IST = ZoneId.of("Europe/Istanbul");

    private static long ms(int day, int hour, int minute) {
        return LocalDateTime.of(2026, 11, day, hour, minute).atZone(IST).toInstant().toEpochMilli();
    }

    private static long sec(int day, int hour, int minute) {
        return ms(day, hour, minute) / 1000;
    }

    @Test
    void dayChipsAreAsWideAsTheirWordsAndFillTheRow() {
        // "Tomorrow" needs more than "Sat": room to spare is shared evenly, the row is filled exactly.
        int[] wide = PlanWhen.chipWidths(new int[] {33, 45, 26, 26, 28, 26, 30}, 240, 3);
        assertEquals(240 - 6 * 3, java.util.Arrays.stream(wide).sum());
        assertTrue(wide[1] > wide[2] && wide[1] - wide[2] >= 19 - 1, "the wide word keeps its extra room");
        // Too little room: shared in proportion, still filling it exactly, none wider than it wanted.
        int[] tight = PlanWhen.chipWidths(new int[] {40, 60, 40}, 100, 2);
        assertEquals(96, java.util.Arrays.stream(tight).sum());
        assertTrue(tight[1] > tight[0] && tight[0] <= 40 && tight[1] <= 60);
        assertEquals(0, PlanWhen.chipWidths(new int[0], 100, 3).length);
    }

    @Test
    void theFormStartsTonightAtNineElseTheNextWholeHourElseTomorrowAtNine() {
        assertEquals(sec(5, 21, 0), PlanWhen.defaultTime(ms(5, 17, 0), IST));
        // 20:40: tonight at 21:00 is too close; 22:00 is the next whole hour half an hour away.
        assertEquals(sec(5, 22, 0), PlanWhen.defaultTime(ms(5, 20, 40), IST));
        // 23:40: no whole hour left today.
        assertEquals(sec(6, 21, 0), PlanWhen.defaultTime(ms(5, 23, 40), IST));
    }

    @Test
    void sevenDaysOfQuarterHoursFromFiveMinutesAhead() {
        long now = ms(5, 23, 50);
        List<PlanWhen.Day> days = PlanWhen.days(now, IST);
        assertEquals(7, days.size());
        // Late in the evening today has no quarter left: tomorrow comes first.
        assertEquals(1, days.get(0).offset());
        List<Long> today = PlanWhen.times(new PlanWhen.Day(sec(5, 0, 0), 0, 4), ms(5, 20, 7), IST);
        assertEquals(sec(5, 20, 15), today.get(0), "at least five minutes ahead, on a quarter");
        assertEquals(sec(5, 23, 45), today.get(today.size() - 1));
        for (long at : today) {
            assertEquals(0, at % PlanWhen.STEP);
        }
    }

    @Test
    void anotherDayKeepsTheWallClockTime() {
        long now = ms(5, 12, 0);
        PlanWhen.Day saturday = new PlanWhen.Day(sec(7, 0, 0), 2, 6);
        assertEquals(sec(7, 21, 30), PlanWhen.on(saturday, sec(5, 21, 30), now, IST));
        // Today, a time gone by moves to the first one left.
        PlanWhen.Day today = new PlanWhen.Day(sec(5, 0, 0), 0, 4);
        assertEquals(sec(5, 12, 15), PlanWhen.on(today, sec(6, 9, 0), now, IST));
        assertTrue(PlanWhen.times(new PlanWhen.Day(sec(20, 0, 0), 15, 5), now, IST).isEmpty(), "past 7 days");
    }
}
