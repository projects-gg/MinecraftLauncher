package gg.projects.overlay.ui;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import gg.projects.overlay.state.OverlayState;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * CREW-PLUS: the plan form's templates fill it as the launcher's {@code plan-templates.ts} does (its own tests use the
 * same days and times).
 */
class PlanTemplatesTest {
    private static final ZoneId IST = ZoneId.of("Europe/Istanbul");

    private static long ms(int day, int hour, int minute) {
        return LocalDateTime.of(2026, 11, day, hour, minute).atZone(IST).toInstant().toEpochMilli();
    }

    private static long sec(int day, int hour, int minute) {
        return ms(day, hour, minute) / 1000;
    }

    private static OverlayState.PlanTemplate template(String day, int time) {
        return new OverlayState.PlanTemplate("Şablon", day, time, "");
    }

    /** Where {@code template} puts the form opened at {@code now}: its time (Unix seconds), or -1. */
    private static long fills(OverlayState.PlanTemplate template, long now) {
        PlanTemplates.Fill fill = PlanTemplates.fill(template, PlanWhen.days(now, IST), now, IST);
        return fill == null ? -1 : fill.at();
    }

    @Test
    void theBuiltInThreeAreTheLaunchersThree() {
        List<OverlayState.PlanTemplate> three = PlanTemplates.builtIn(key -> key);
        assertEquals(List.of("today", "fri", "sat"), three.stream().map(OverlayState.PlanTemplate::day).toList());
        assertEquals(List.of(1260, 1260, 900), three.stream().map(OverlayState.PlanTemplate::time).toList());
        assertEquals("plan.template.weekend.note", three.get(2).note());
        // 3 November 2026 was a Tuesday: tonight, Friday the 6th, Saturday the 7th.
        long tuesday = ms(3, 18, 0);
        assertEquals(sec(3, 21, 0), fills(three.get(0), tuesday));
        assertEquals(sec(6, 21, 0), fills(three.get(1), tuesday));
        assertEquals(sec(7, 15, 0), fills(three.get(2), tuesday));
    }

    @Test
    void aTimeGoneByTakesTheClosestTimeLeftThatDay() {
        assertEquals(sec(3, 22, 15), fills(template("today", 1260), ms(3, 22, 10)));
        assertEquals(sec(6, 22, 15), fills(template("fri", 1260), ms(6, 22, 10)));
        // Its own day, still ahead: today, not next week.
        assertEquals(sec(7, 15, 0), fills(template("sat", 900), ms(7, 9, 0)));
    }

    @Test
    void todayLateAtNightMeansTheFormsFirstDay() {
        assertEquals(sec(4, 21, 0), fills(template("today", 1260), ms(3, 23, 56)));
        assertEquals(sec(4, 20, 30), fills(template("tomorrow", 1230), ms(3, 12, 0)));
        PlanTemplates.Fill fill = PlanTemplates.fill(template("tomorrow", 1230), PlanWhen.days(ms(3, 12, 0), IST),
            ms(3, 12, 0), IST);
        assertEquals(1, fill.day(), "the form's second day");
        assertNull(PlanTemplates.fill(template("today", 1260), List.of(), ms(3, 12, 0), IST));
    }

    @Test
    void onlyTemplatesThatKeepTheRulesComeOverTheBridge() {
        assertEquals(new OverlayState.PlanTemplate("Ada turu", "sat", 900, "Sesliye gelin"),
            OverlayState.PlanTemplate.of("  Ada turu ", "sat", 900, " Sesliye gelin "));
        assertNull(OverlayState.PlanTemplate.of("Bozuk", "someday", 900, ""));
        assertNull(OverlayState.PlanTemplate.of("Yamuk", "fri", 1270, ""));
        assertNull(OverlayState.PlanTemplate.of("Geç", "fri", 24 * 60, ""));
        assertNull(OverlayState.PlanTemplate.of("   ", "fri", 900, ""));
    }
}
