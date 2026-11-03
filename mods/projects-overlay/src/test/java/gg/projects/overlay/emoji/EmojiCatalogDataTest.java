package gg.projects.overlay.emoji;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.ui.EmojiGlyphs;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * OVL-SOC: the launcher's picker data as the build copies it into the jar (processResources), against the faces'
 * pictures: every emoji the picker offers has one, and the search finds what the launcher's finds.
 */
class EmojiCatalogDataTest {
    @Test
    void theJarsDataOffersThePicturedEmojiInBothLanguages() {
        EmojiCatalog tr = EmojiCatalog.load("tr", EmojiGlyphs::colour);
        EmojiCatalog en = EmojiCatalog.load("en", EmojiGlyphs::colour);
        assertTrue(tr.entries().size() > 1200, "the launcher's one-code-point emoji: " + tr.entries().size());
        assertEquals(tr.entries().size(), en.entries().size(), "one set, two languages");
        for (EmojiCatalog.Entry e : tr.entries()) {
            assertTrue(EmojiGlyphs.colour(e.emoji().codePointAt(0)), e.emoji());
            assertEquals(1, e.emoji().codePointCount(0, e.emoji().length()), e.emoji());
        }
        assertEquals(List.of(EmojiCatalog.Group.values()), tr.groups(), "every group has some (flags: 🏁 🚩 🎌 🏴 🏳)");
        assertEquals("😂", EmojiFinder.search(tr, "sevinç").get(0).emoji());
        assertEquals("❤", EmojiFinder.search(tr, "kalp").get(0).emoji(), "the red heart before the anatomical one");
        assertEquals("🔥", EmojiFinder.search(tr, "fire").get(0).emoji(), "a Turkish search finds English");
        assertEquals("🔥", EmojiFinder.search(en, "fire").get(0).emoji());
        assertNotNull(tr.find("👍"));
        assertEquals("Sevinç gözyaşları", EmojiCatalog.displayName(tr.find("😂").name(), true));
    }
}
