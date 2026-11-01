package gg.projects.overlay.ui;

import static org.junit.jupiter.api.Assertions.assertEquals;

import gg.projects.overlay.lang.Language;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.draw.MarkSprites;
import java.util.List;
import org.junit.jupiter.api.Test;

/** A person's badges beside their name (BADGES-UI): the room they take and how the narrator names them. */
class MarksTest {
    private static final OverlayState.MarkDef ADMIN = new OverlayState.MarkDef("admin", "Projects yöneticisi",
        "Projects admin", "red", "check", "seal");
    private static final OverlayState.MarkDef PREMIUM = new OverlayState.MarkDef("premium", "Premium Club üyesi",
        "Premium Club member", "orange", "star", "seal");

    @Test
    void onlyTheFirstBadgeTakesRoomBesideTheName() {
        assertEquals(0f, Marks.room(List.of()));
        assertEquals(0f, Marks.room(null));
        assertEquals(Marks.GAP + MarkSprites.SIZE, Marks.room(List.of(ADMIN)));
        assertEquals(Marks.GAP + MarkSprites.SIZE, Marks.room(List.of(ADMIN, PREMIUM)), "one badge, however many");
    }

    @Test
    void theNarratorSaysEveryBadgeAfterTheNameInTheOverlaysLanguage() {
        assertEquals("Kuzey_77, Projects yöneticisi, Premium Club üyesi",
            Marks.spoken("Kuzey_77", List.of(ADMIN, PREMIUM), Language.TURKISH));
        assertEquals("Kuzey_77, Projects admin", Marks.spoken("Kuzey_77", List.of(ADMIN), Language.ENGLISH));
        assertEquals("Kuzey_77", Marks.spoken("Kuzey_77", List.of(), Language.TURKISH));
    }

    @Test
    void thePointerIsOnTheBadgeAndItsShadow() {
        assertEquals(true, Marks.over(10, 20, 10f, 20f));
        assertEquals(true, Marks.over(10 + MarkSprites.SIZE, 20 + MarkSprites.SIZE, 10f, 20f));
        assertEquals(false, Marks.over(10 + MarkSprites.FOOT, 20, 10f, 20f));
        assertEquals(false, Marks.over(9.9, 20, 10f, 20f));
    }
}
