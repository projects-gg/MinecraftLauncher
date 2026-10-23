package gg.projects.overlay.stickers;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import org.junit.jupiter.api.Test;

/** The set the build copies from packages/stickers into the jar, as the mod reads it. */
class StickerSetTest {
    private static String manifest() throws IOException {
        try (InputStream in = StickerSetTest.class.getResourceAsStream(StickerSet.MANIFEST)) {
            assertNotNull(in, "the build copies the manifest into the jar");
            return new String(in.readAllBytes(), StandardCharsets.UTF_8);
        }
    }

    @Test
    void theSetComesWithEveryStickerItsNamesAndCells() throws IOException {
        StickerSet set = StickerSet.parse(manifest());
        assertEquals("pl1", set.set());
        assertEquals(32, set.all().size());
        assertEquals(List.of("reactions", "game", "server", "greetings"),
            set.themes().stream().map(StickerSet.Theme::id).toList());
        StickerSet.Sticker gg = set.find("pl1/gg");
        assertNotNull(gg);
        assertEquals("GG", gg.name(true));
        StickerSet.Sticker lol = set.find("pl1/lol");
        assertEquals("Kahkaha", lol.name(true));
        assertEquals("LOL", lol.name(false));
        assertEquals(0, lol.column());
        assertEquals(0, lol.row());
        assertNull(set.find("pl9/new"));
        assertNull(set.find(null));
        // Theme by theme, each in its order: the picker's rows are the atlas's rows.
        for (int i = 0; i < set.all().size(); i++) {
            StickerSet.Sticker s = set.all().get(i);
            assertEquals(i / 8, s.row(), s.ref());
            assertEquals(i % 8, s.column(), s.ref());
        }
        Set<String> cells = new HashSet<>();
        for (StickerSet.Sticker s : set.all()) {
            assertTrue(cells.add(s.column() + ":" + s.row()), "one cell each");
            assertFalse(s.motion().isEmpty(), s.ref());
            assertTrue(StickerMotion.duration(s.motion()) <= 1_100, s.motion());
        }
        assertEquals(160, set.tile());
        assertEquals(8, set.columns());
        assertEquals(4, set.rows());
    }

    @Test
    void theAtlasComesWithTheJar() throws IOException {
        try (InputStream in = StickerSetTest.class.getResourceAsStream("/assets/projects_overlay/stickers/pl1.png")) {
            assertNotNull(in);
            byte[] head = in.readNBytes(8);
            assertEquals((byte) 0x89, head[0]);
            assertEquals('P', head[1]);
        }
    }

    @Test
    void aBrokenManifestIsRefused() {
        assertThrows(RuntimeException.class, () -> StickerSet.parse("{}"));
        assertThrows(RuntimeException.class, () -> StickerSet.parse("[]"));
    }
}
