package gg.projects.overlay.ui;

import java.time.Instant;
import java.time.LocalDate;
import java.time.ZoneId;
import java.time.ZonedDateTime;
import java.time.temporal.ChronoUnit;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * A crew plan's time as the launcher says it (OVL-SOC2; its {@code planWhen}, {@code planDays}, {@code planTimes} and
 * {@code defaultPlanTime} in {@code features/social/crew-plans.ts}): "Bugün 21:00", "Yarın 20:30", "Cumartesi 21:00",
 * inside a sentence "bugün 21:00". The form offers today (while a quarter of an hour of it is left) and the days after,
 * seven in all, each day's whole quarters of an hour from 5 minutes to 7 days ahead; it starts tonight at 21:00 while
 * that is half an hour away or more, else the next whole hour (before midnight), else tomorrow at 21:00.
 */
public final class PlanWhen {
    /** A plan's step (seconds): a whole quarter of an hour. */
    public static final int STEP = 900;
    /** Days the form offers. */
    public static final int DAYS = 7;
    /** A plan starts at least this long ahead (seconds), and at most this short of 7 days. */
    public static final int MIN_LEAD = 300;

    private PlanWhen() {
    }

    /** {@code atMs} as seen at {@code nowMs} on this computer's clock and calendar. */
    public static String of(long atMs, long nowMs, boolean inSentence) {
        return of(atMs, nowMs, inSentence, ZoneId.systemDefault());
    }

    static String of(long atMs, long nowMs, boolean inSentence, ZoneId zone) {
        ZonedDateTime at = Instant.ofEpochMilli(atMs).atZone(zone);
        LocalDate today = Instant.ofEpochMilli(nowMs).atZone(zone).toLocalDate();
        String time = clock(at);
        long days = ChronoUnit.DAYS.between(today, at.toLocalDate());
        if (days == 0) {
            return Texts.str(inSentence ? "plan.today_in" : "plan.today", time);
        }
        if (days == 1) {
            return Texts.str(inSentence ? "plan.tomorrow_in" : "plan.tomorrow", time);
        }
        return Texts.str("plan.on_day", Texts.str("plan.day." + at.getDayOfWeek().getValue()), time);
    }

    /** "21:00". */
    public static String clock(ZonedDateTime at) {
        return String.format(Locale.ROOT, "%02d:%02d", at.getHour(), at.getMinute());
    }

    /** "21:00" for a Unix second on this computer's clock. */
    public static String clock(long atSeconds) {
        return clock(Instant.ofEpochSecond(atSeconds).atZone(ZoneId.systemDefault()));
    }

    /** A day the form offers: its local midnight (Unix seconds) and how many days after today it is. */
    public record Day(long start, int offset, int weekday) {
    }

    /** The days the form offers at {@code nowMs}: seven, today only while one of its times is left. */
    public static List<Day> days(long nowMs) {
        return days(nowMs, ZoneId.systemDefault());
    }

    static List<Day> days(long nowMs, ZoneId zone) {
        List<Day> out = new ArrayList<>(DAYS);
        LocalDate today = Instant.ofEpochMilli(nowMs).atZone(zone).toLocalDate();
        for (int offset = 0; offset <= DAYS && out.size() < DAYS; offset++) {
            LocalDate date = today.plusDays(offset);
            Day day = new Day(date.atStartOfDay(zone).toEpochSecond(), offset, date.getDayOfWeek().getValue());
            if (!times(day, nowMs, zone).isEmpty()) {
                out.add(day);
            }
        }
        return out;
    }

    /** A day's whole quarters of an hour the form offers (Unix seconds). */
    public static List<Long> times(Day day, long nowMs) {
        return times(day, nowMs, ZoneId.systemDefault());
    }

    static List<Long> times(Day day, long nowMs, ZoneId zone) {
        long now = nowMs / 1000;
        long end = Instant.ofEpochSecond(day.start()).atZone(zone).toLocalDate().plusDays(1).atStartOfDay(zone)
            .toEpochSecond();
        List<Long> out = new ArrayList<>();
        for (long at = (day.start() + STEP - 1) / STEP * STEP; at < end; at += STEP) {
            if (at >= now + MIN_LEAD && at <= now + DAYS * 86_400L - MIN_LEAD) {
                out.add(at);
            }
        }
        return out;
    }

    /** "Bugün", "Yarın", then the weekday's short name ("Cmt"). */
    public static String dayChip(Day day) {
        if (day.offset() == 0) {
            return Texts.str("plan.form.today");
        }
        if (day.offset() == 1) {
            return Texts.str("plan.form.tomorrow");
        }
        return Texts.str("plan.day.short." + day.weekday());
    }

    /**
     * The day chips' widths: each its {@code wanted} width and the room left shared out evenly, or, when they do not
     * all fit, the room shared in proportion to them (the words then get smaller); with the gaps they fill
     * {@code room} exactly.
     */
    public static int[] chipWidths(int[] wanted, int room, int gap) {
        int n = wanted.length;
        int[] out = new int[n];
        if (n == 0) {
            return out;
        }
        int free = room - (n - 1) * gap;
        long sum = 0;
        for (int w : wanted) {
            sum += w;
        }
        int used = 0;
        for (int i = 0; i < n; i++) {
            out[i] = sum <= free ? wanted[i] + (int) ((free - sum) / n) : (int) (wanted[i] * (long) free / sum);
            used += out[i];
        }
        // What rounding left over goes to the first chips, a pixel each.
        for (int i = 0; used < free; i = (i + 1) % n) {
            out[i]++;
            used++;
        }
        return out;
    }

    /** Where the form starts (Unix seconds): tonight 21:00, else the next whole hour, else tomorrow 21:00. */
    public static long defaultTime(long nowMs) {
        return defaultTime(nowMs, ZoneId.systemDefault());
    }

    static long defaultTime(long nowMs, ZoneId zone) {
        ZonedDateTime now = Instant.ofEpochMilli(nowMs).atZone(zone);
        LocalDate today = now.toLocalDate();
        long halfHour = 30 * 60_000L;
        long tonight = today.atTime(21, 0).atZone(zone).toInstant().toEpochMilli();
        if (tonight - nowMs >= halfHour) {
            return tonight / 1000;
        }
        long hour = now.truncatedTo(ChronoUnit.HOURS).plusHours(1).toInstant().toEpochMilli();
        if (hour - nowMs < halfHour) {
            hour += 3_600_000L;
        }
        long midnight = today.plusDays(1).atStartOfDay(zone).toInstant().toEpochMilli();
        return (hour < midnight ? hour : today.plusDays(1).atTime(21, 0).atZone(zone).toInstant().toEpochMilli())
            / 1000;
    }

    /**
     * The offered time closest to {@code wanted}'s wall clock on {@code day}: the same hour and minute, else the day's
     * first time after it, else its last (today, the past is not offered); -1 when the day offers none.
     */
    public static long on(Day day, long wantedSeconds, long nowMs) {
        return on(day, wantedSeconds, nowMs, ZoneId.systemDefault());
    }

    static long on(Day day, long wantedSeconds, long nowMs, ZoneId zone) {
        List<Long> times = times(day, nowMs, zone);
        if (times.isEmpty()) {
            return -1;
        }
        ZonedDateTime wanted = Instant.ofEpochSecond(wantedSeconds).atZone(zone);
        long target = Instant.ofEpochSecond(day.start()).atZone(zone).toLocalDate()
            .atTime(wanted.getHour(), wanted.getMinute()).atZone(zone).toEpochSecond();
        for (long at : times) {
            if (at >= target) {
                return at;
            }
        }
        return times.get(times.size() - 1);
    }
}
