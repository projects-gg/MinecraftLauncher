package gg.projects.overlay.toast;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.Test;

/** A pinned toast (a voice chat ring, VOICE-O): its own time, never in the conveyor's way, never pushed out. */
class ToastStackPinTest {
    private static final float H = 40f;

    private final ToastStack<String> stack = new ToastStack<>();
    private long now = 1_000L;

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
    void aRingStaysWhileTheOthersComeAndGoAroundIt() {
        stack.pin("ring", "R", H, 30_000, now);
        run(100);
        stack.push("a", "A", H, 2_000, now);
        run(2_600);
        // The message's time ran out above the ring: it went without waiting for the ring below it.
        assertEquals(List.of("ring"), visibleKeys());
        for (String key : List.of("b", "c", "d", "e")) {
            stack.push(key, key.toUpperCase(), H, 20_000, now);
        }
        run(300);
        assertTrue(visibleKeys().contains("ring"), "never pushed out");
        assertEquals(List.of("e", "d", "ring"), visibleKeys(), "the oldest of the others made room");
    }

    @Test
    void aRingEndsOnItsOwnTimeOrWhenDismissedEvenWhileTheGameIsAway() {
        stack.pin("ring", "R", H, 1_000, now);
        long until = now + 1_000;
        while (now < until + 300) {
            now += 8;
            // The game window is not in front: the others' time stands still, the ring's does not.
            stack.update(now, true, false);
        }
        assertTrue(visibleKeys().isEmpty());
        stack.pin("ring2", "R", H, 30_000, now);
        run(100);
        stack.dismiss("ring2", now);
        assertTrue(visibleKeys().isEmpty());
        run(400);
        assertTrue(stack.isEmpty(), "faded and gone");
    }
}
