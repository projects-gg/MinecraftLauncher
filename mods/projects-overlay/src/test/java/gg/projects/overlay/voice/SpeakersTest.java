package gg.projects.overlay.voice;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.Set;
import org.junit.jupiter.api.Test;

/** The HUD corner's speakers: in, held, out, and nothing left to draw. */
class SpeakersTest {
    private final Speakers speakers = new Speakers();

    @Test
    void aSpeakerFadesInStaysASecondAfterAndFadesOut() {
        assertTrue(speakers.idle(0), "nobody: the corner draws nothing");
        speakers.update(Set.of("kaan"), 1_000);
        assertEquals(List.of(new Speakers.Shown("kaan", 0f)), speakers.shown(1_000));
        assertEquals(0.5f, speakers.shown(1_050).get(0).alpha(), 1e-4);
        assertEquals(1f, speakers.shown(1_200).get(0).alpha(), 1e-4);
        speakers.update(Set.of(), 2_000);
        assertEquals(1f, speakers.shown(2_900).get(0).alpha(), 1e-4, "held a second after");
        assertEquals(0.5f, speakers.shown(3_150).get(0).alpha(), 1e-4);
        assertTrue(speakers.idle(3_300), "faded out and forgotten");
        assertTrue(speakers.shown(3_300).isEmpty());
    }

    @Test
    void aBreathBetweenWordsNeverBlinks() {
        speakers.update(Set.of("kaan"), 0);
        speakers.update(Set.of(), 500);
        speakers.update(Set.of("kaan"), 900);
        assertEquals(1f, speakers.shown(900).get(0).alpha(), 1e-4, "still in");
        // Back while fading: from where the fade had them.
        speakers.update(Set.of(), 1_000);
        speakers.update(Set.of("kaan"), 2_150);
        assertEquals(0.5f, speakers.shown(2_150).get(0).alpha(), 1e-4);
        assertEquals(1f, speakers.shown(2_200).get(0).alpha(), 1e-4);
    }

    @Test
    void rowsKeepTheOrderTheyStartedInAndCountTheRest() {
        speakers.update(Set.of("a"), 0);
        speakers.update(Set.of("a", "b"), 10);
        speakers.update(Set.of("a", "b", "c", "d", "e"), 20);
        List<Speakers.Shown> rows = speakers.shown(500);
        assertEquals(List.of("a", "b"), rows.subList(0, 2).stream().map(Speakers.Shown::id).toList());
        assertEquals(Speakers.MAX_ROWS, rows.size());
        assertEquals(1, speakers.beyond(500));
        speakers.clear();
        assertTrue(speakers.idle(500));
        assertFalse(speakers.beyond(500) > 0);
    }
}
