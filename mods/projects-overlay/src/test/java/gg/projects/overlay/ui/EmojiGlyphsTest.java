package gg.projects.overlay.ui;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.ByteArrayInputStream;
import java.nio.charset.StandardCharsets;
import java.util.BitSet;
import org.junit.jupiter.api.Test;

class EmojiGlyphsTest {
    @Test
    void theReactionsTestersSawAreColourPictures() {
        // The owner's screenshot (26 Oct): 👍 and 😅 on reaction chips; the launcher's quick row has ❤ too.
        assertTrue(EmojiGlyphs.colour(0x1F44D));
        assertTrue(EmojiGlyphs.colour(0x1F605));
        assertTrue(EmojiGlyphs.colour(0x2764));
        assertTrue(EmojiGlyphs.colour(0x1F525));
        assertFalse(EmojiGlyphs.colour('a'));
        assertFalse(EmojiGlyphs.colour('Ş'));
        assertFalse(EmojiGlyphs.colour(0xE106), "a server's pack picture is the game's, not ours");
        assertTrue(EmojiGlyphs.any("ok 👍"));
        assertFalse(EmojiGlyphs.any("Görünme süresi"));
        assertFalse(EmojiGlyphs.any(null));
    }

    @Test
    void theRangesFileReadsLeniently() {
        BitSet set = EmojiGlyphs.load(new ByteArrayInputStream("1f600-1f602\n2764\n\nnot a range\nzz-1\n"
            .getBytes(StandardCharsets.UTF_8)));
        assertTrue(set.get(0x1F600) && set.get(0x1F601) && set.get(0x1F602) && set.get(0x2764));
        assertFalse(set.get(0x1F603));
        assertTrue(EmojiGlyphs.load(null).isEmpty());
    }
}
