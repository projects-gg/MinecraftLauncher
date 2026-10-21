package gg.projects.overlay.toast;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.Test;

class ToastStackTest {
    private static final float H = 50f;

    private final ToastStack<String> stack = new ToastStack<>();
    private long now = 1_000L;

    /** Runs frames at 120 Hz for {@code ms}. */
    private void run(long ms) {
        long end = now + ms;
        while (now < end) {
            now += 8;
            stack.update(now, false, false);
        }
    }

    private List<String> visibleKeys() {
        return stack.entries().stream().filter(e -> !e.leaving()).map(e -> e.key).toList();
    }

    @Test
    void newestIsOnTopAndTheStackEndsAtItsBottomAnchor() {
        stack.push("a", "A", H, 5_000, now);
        run(50);
        stack.push("b", "B", H, 5_000, now);
        run(600);
        assertEquals(List.of("b", "a"), visibleKeys());
        ToastStack.Entry<String> a = stack.entries().get(1);
        ToastStack.Entry<String> b = stack.entries().get(0);
        assertEquals(-H, a.y(), 0.05f);
        assertEquals(-H - ToastStack.GAP - H, b.y(), 0.05f);
    }

    @Test
    void anchoredAtTheTopTheNewestStartsAtZero() {
        stack.setAnchoredAtTop(true);
        stack.push("a", "A", H, 5_000, now);
        stack.push("b", "B", 30f, 5_000, now);
        run(600);
        assertEquals(0f, stack.entries().get(0).y(), 0.05f);
        assertEquals(30f + ToastStack.GAP, stack.entries().get(1).y(), 0.05f);
    }

    @Test
    void aNewToastDropsInFromAbove() {
        stack.push("a", "A", H, 5_000, now);
        stack.update(now, false, false);
        assertEquals(-H - ToastStack.ENTER_OFFSET, stack.entries().get(0).y(), 0.05f);
        assertEquals(0f, stack.entries().get(0).alpha(now), 0.01f);
        run(400);
        assertEquals(-H, stack.entries().get(0).y(), 0.05f);
        assertEquals(1f, stack.entries().get(0).alpha(now), 0.01f);
    }

    @Test
    void theFourthPushesTheOldestOutAtOnce() {
        for (String k : new String[] {"a", "b", "c"}) {
            stack.push(k, k, H, 5_000, now);
            run(20);
        }
        stack.push("d", "d", H, 5_000, now);
        assertEquals(List.of("d", "c", "b"), visibleKeys());
        ToastStack.Entry<String> oldest = stack.entries().get(3);
        assertTrue(oldest.leaving());
        run(400);
        assertEquals(4 - 1, stack.entries().size());
    }

    @Test
    void toastsLeaveFromTheBottomOneAfterAnother() {
        stack.push("a", "A", H, 5_000, now);
        run(1_000);
        stack.push("b", "B", H, 1_000, now);
        // b's time is up long before a's: it waits until a, below it, has gone.
        run(2_000);
        assertEquals(List.of("b", "a"), visibleKeys());
        // a goes at 6000; b, overdue, follows one stagger later (6200), not at once.
        run(2_000);
        assertEquals(List.of("b"), visibleKeys());
        run(ToastStack.STAGGER_MS - 50);
        assertEquals(List.of("b"), visibleKeys());
        run(100);
        assertTrue(visibleKeys().isEmpty());
        run(400);
        assertTrue(stack.isEmpty());
    }

    @Test
    void aLeavingToastSlidesDownWhileTheOthersGlideIntoItsPlace() {
        stack.push("a", "A", H, 1_000, now);
        run(100);
        stack.push("b", "B", H, 10_000, now);
        run(1_000);
        ToastStack.Entry<String> a = stack.entries().get(1);
        float before = a.y();
        run(60);
        assertTrue(a.leaving());
        assertTrue(a.y() > before, "the oldest slides down");
        ToastStack.Entry<String> b = stack.entries().get(0);
        run(500);
        assertEquals(-H, b.y(), 0.05f);
    }

    @Test
    void anUpdateInPlaceCountsAndStartsTheTimeAgain() {
        assertEquals(ToastStack.Push.ADDED, stack.push("a", "1", H, 1_000, now));
        run(800);
        assertEquals(ToastStack.Push.UPDATED, stack.push("a", "2", 60f, 1_000, now));
        ToastStack.Entry<String> a = stack.entries().get(0);
        assertEquals(2, a.count());
        assertEquals("2", a.content());
        assertEquals(0f, a.refresh(now), 0.01f);
        run(800);
        assertFalse(a.leaving());
        assertEquals(-60f, a.y(), 0.05f);
        run(400);
        assertTrue(a.leaving());
    }

    @Test
    void reducedMotionMovesNothingButStillFades() {
        stack.push("a", "A", H, 1_000, now);
        stack.update(now, false, true);
        assertEquals(-H, stack.entries().get(0).y(), 0.001f);
        stack.push("b", "B", H, 1_000, now);
        stack.update(now, false, true);
        assertEquals(-2 * H - ToastStack.GAP, stack.entries().get(0).y(), 0.001f);
        long t = now + 1_100;
        stack.update(t, false, true);
        ToastStack.Entry<String> a = stack.entries().get(1);
        assertTrue(a.leaving());
        assertEquals(-H, a.y(), 0.001f);
        assertTrue(a.alpha(t + 50) < 1f);
    }

    @Test
    void timeStandsStillWhileTheWindowIsAway() {
        stack.push("a", "A", H, 1_000, now);
        long end = now + 5_000;
        while (now < end) {
            now += 16;
            stack.update(now, true, false);
        }
        assertEquals(List.of("a"), visibleKeys());
        run(1_100);
        assertTrue(visibleKeys().isEmpty());
    }

    @Test
    void clearingFadesEverythingWhereItIs() {
        stack.push("a", "A", H, 5_000, now);
        run(500);
        float y = stack.entries().get(0).y();
        stack.clear(now);
        run(50);
        assertEquals(y, stack.entries().get(0).y(), 0.05f);
        run(ToastStack.CLEAR_FADE_MS);
        assertTrue(stack.isEmpty());
    }

    @Test
    void anIdleStackReportsNothingMoving() {
        assertFalse(stack.update(now, false, false));
        stack.push("a", "A", H, 5_000, now);
        assertTrue(stack.update(now + 8, false, false));
        run(600);
        assertFalse(stack.update(now + 8, false, false));
    }
}
