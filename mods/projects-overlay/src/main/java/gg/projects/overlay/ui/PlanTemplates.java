package gg.projects.overlay.ui;

import gg.projects.overlay.state.OverlayState;
import java.time.Instant;
import java.time.ZoneId;
import java.util.ArrayList;
import java.util.List;

/**
 * Crew plan templates (CREW-PLUS; the launcher's {@code features/social/plan-templates.ts}): a day relative to the day
 * they are used ({@code today}, {@code tomorrow}, the next of a weekday), a time of day and a note, never a date. The
 * three built-in ones are the launcher's three in the overlay's own words ("Bu Akşam": today 21:00, "Cuma Akşamı":
 * Friday 21:00, "Hafta Sonu": Saturday 15:00); the player's own come from the launcher ({@code lists.templates}).
 * Pressing one puts its day, time and note in the plan form, as the launcher's form does: its time on the first day it
 * names that still offers it, else that day's closest time; "today" late at night means the form's first day.
 */
public final class PlanTemplates {
    /** The built-in three: their key in the overlay's words, their day and their time (minutes after midnight). */
    private static final String[][] BUILT_IN = {
        {"tonight", "today", "1260"},
        {"friday", "fri", "1260"},
        {"weekend", "sat", "900"},
    };
    /** Weekday names in ISO order (Monday is 1, {@link PlanWhen.Day#weekday()}). */
    private static final List<String> WEEKDAYS = List.of("mon", "tue", "wed", "thu", "fri", "sat", "sun");

    /** Where a template puts the form: its day (an index of the form's days) and time (Unix seconds). */
    public record Fill(int day, long at) {
    }

    private PlanTemplates() {
    }

    /** The built-in three, in the overlay's language now. */
    public static List<OverlayState.PlanTemplate> builtIn() {
        return builtIn(key -> Texts.str(key));
    }

    /** The built-in three with their names and notes from {@code words} (a dictionary key in, the words out). */
    static List<OverlayState.PlanTemplate> builtIn(java.util.function.UnaryOperator<String> words) {
        List<OverlayState.PlanTemplate> out = new ArrayList<>(BUILT_IN.length);
        for (String[] one : BUILT_IN) {
            out.add(new OverlayState.PlanTemplate(words.apply("plan.template." + one[0]), one[1],
                Integer.parseInt(one[2]), words.apply("plan.template." + one[0] + ".note")));
        }
        return out;
    }

    /** The form's days a template's day names. */
    private static List<Integer> daysOf(String day, List<PlanWhen.Day> days) {
        List<Integer> out = new ArrayList<>();
        for (int i = 0; i < days.size(); i++) {
            PlanWhen.Day d = days.get(i);
            boolean fits = switch (day) {
                case "today" -> d.offset() == 0;
                case "tomorrow" -> d.offset() == 1;
                default -> WEEKDAYS.indexOf(day) + 1 == d.weekday();
            };
            if (fits) {
                out.add(i);
            }
        }
        return out;
    }

    /** Where {@code template} puts the form opened at {@code nowMs}, or null when no day fits. */
    public static Fill fill(OverlayState.PlanTemplate template, List<PlanWhen.Day> days, long nowMs) {
        return fill(template, days, nowMs, ZoneId.systemDefault());
    }

    static Fill fill(OverlayState.PlanTemplate template, List<PlanWhen.Day> days, long nowMs, ZoneId zone) {
        int hour = template.time() / 60;
        int minute = template.time() % 60;
        List<Integer> named = daysOf(template.day(), days);
        for (int i : named) {
            long wanted = at(days.get(i), hour, minute, zone);
            if (PlanWhen.times(days.get(i), nowMs, zone).contains(wanted)) {
                return new Fill(i, wanted);
            }
        }
        int first = !named.isEmpty() ? named.get(0) : "today".equals(template.day()) && !days.isEmpty() ? 0 : -1;
        if (first < 0) {
            return null;
        }
        long at = PlanWhen.on(days.get(first), at(days.get(first), hour, minute, zone), nowMs, zone);
        return at < 0 ? null : new Fill(first, at);
    }

    /** {@code day} at {@code hour}:{@code minute} on this computer's calendar (Unix seconds). */
    private static long at(PlanWhen.Day day, int hour, int minute, ZoneId zone) {
        return Instant.ofEpochSecond(day.start()).atZone(zone).toLocalDate().atTime(hour, minute).atZone(zone)
            .toEpochSecond();
    }
}
