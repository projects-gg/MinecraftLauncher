package gg.projects.overlay.voice;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Icon;
import org.junit.jupiter.api.Test;

/** The signs beside someone in a room (VOICE-KEYS), as the launcher's rules.ts draws them. */
class VoiceMarksTest {
    @Test
    void nothingWhileTheMicrophoneAndTheSoundAreOn() {
        assertNull(VoiceMarks.own(false, false));
        assertNull(VoiceMarks.face(false, false, false));
    }

    @Test
    void deafenedShowsTheHeadphonesAloneEvenWhenMutedToo() {
        assertEquals(Icon.DEAF, VoiceMarks.own(false, true));
        assertEquals(Icon.DEAF, VoiceMarks.own(true, true));
        assertEquals(Icon.MIC_OFF, VoiceMarks.own(true, false));
        assertEquals(new VoiceMarks.Mark(Icon.DEAF, Theme.ASH_300), VoiceMarks.face(true, true, false));
        assertEquals(new VoiceMarks.Mark(Icon.MIC_OFF, Theme.ASH_300), VoiceMarks.face(true, false, false));
    }

    @Test
    void aModeratorsMuteStaysFlareOverEverything() {
        assertEquals(new VoiceMarks.Mark(Icon.MIC_OFF, Theme.FLARE_300), VoiceMarks.face(false, true, true));
        assertEquals(new VoiceMarks.Mark(Icon.MIC_OFF, Theme.FLARE_300), VoiceMarks.face(false, false, true));
    }

    @Test
    void theDeafSignIsTheHeadphonesSlashed() {
        assertEquals("Headphones", Icon.DEAF.phosphorName);
        assertEquals(true, Icon.DEAF.slashed);
        assertEquals(false, Icon.HEADPHONES.slashed);
    }
}
