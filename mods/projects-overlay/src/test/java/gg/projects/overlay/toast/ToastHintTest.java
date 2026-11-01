package gg.projects.overlay.toast;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.lang.Dictionary;
import gg.projects.overlay.lang.Language;
import java.awt.Font;
import java.awt.font.FontRenderContext;
import java.io.InputStream;
import java.util.List;
import org.junit.jupiter.api.Assumptions;
import org.junit.jupiter.api.Test;

/**
 * F7-OPEN: a toast's key hint says to press a key ("F7 ile arayüzü aç" was read as a button, and players clicked it),
 * and every hint line stays inside the card with the key's name on its cap.
 */
class ToastHintTest {
    /** The hints that name the overlay key, and the Fn line under them. */
    private static final List<String> KEY_HINTS = List.of("projects_overlay.toast.hint.open",
        "projects_overlay.toast.hint.call", "projects_overlay.toast.hint.join", "projects_overlay.toast.hint.manage");
    private static final String FN_HINT = "projects_overlay.toast.hint.fn";
    /** The game rounds each glyph's advance to its pixel grid: the words keep this much room for it. */
    private static final int ROUNDING_SLACK = 8;

    @Test
    void aKeyNameGetsTheRoomTheWordsLeave() {
        int both = ToastCard.TEXT_W - 2 * ToastCard.KEY_PAD - (50 + ToastCard.HINT_SPACE) - (40 + ToastCard.HINT_SPACE);
        assertEquals(both, ToastCard.keyRoom(ToastCard.TEXT_W, 50, 40));
        assertEquals(ToastCard.MAX_KEY_NAME, ToastCard.keyRoom(ToastCard.TEXT_W, 0, 30), "short words: the old cap");
        assertEquals(ToastCard.MIN_KEY_NAME, ToastCard.keyRoom(ToastCard.TEXT_W, 120, 40), "never cut to nothing");
        assertTrue(ToastCard.keyRoom(ToastCard.TEXT_W, 0, 0) <= ToastCard.MAX_KEY_NAME);
    }

    @Test
    void theKeyGoesWhereTheSentenceHasIt() {
        assertArrayEquals(new String[] {"Açmak için", "tuşuna bas"}, ToastCard.around("Açmak için \u0000 tuşuna bas"));
        assertArrayEquals(new String[] {"Press", "to open"}, ToastCard.around("Press \u0000 to open"));
        assertArrayEquals(new String[] {"Olmazsa", "ile birlikte bas"},
            ToastCard.around("Olmazsa \u0000 ile birlikte bas"));
        assertArrayEquals(new String[] {"Kontroller'den bir tuş ata", ""},
            ToastCard.around("Kontroller'den bir tuş ata"));
    }

    @Test
    void twoHintLinesStandApart() {
        assertEquals(0, ToastCard.hintsHeight(0));
        assertEquals(ToastCard.HINT_H, ToastCard.hintsHeight(1));
        assertEquals(2 * ToastCard.HINT_H + ToastCard.HINT_LINE_GAP, ToastCard.hintsHeight(2));
    }

    @Test
    void everyKeyHintSaysToPressTheKey() {
        Dictionary words = Dictionary.shared();
        for (String key : KEY_HINTS) {
            String tr = words.template(Language.TURKISH, key);
            String en = words.template(Language.ENGLISH, key);
            assertTrue(tr.endsWith(" için %s tuşuna bas"), key + ": " + tr);
            assertTrue(en.startsWith("Press %s to "), key + ": " + en);
        }
        assertEquals("Olmazsa %s ile birlikte bas", words.template(Language.TURKISH, FN_HINT));
        assertEquals("If nothing happens, hold %s too", words.template(Language.ENGLISH, FN_HINT));
        assertEquals(ToastCard.FN, "Fn");
    }

    @Test
    void everyHintLineFitsTheCard() throws Exception {
        Font font = figtree();
        Assumptions.assumeTrue(font != null, "no font rasteriser in this JVM");
        Dictionary words = Dictionary.shared();
        for (Language language : Language.values()) {
            for (String key : KEY_HINTS) {
                fits(font, language, words.template(language, key), "F7", "Sağ Shift");
            }
            fits(font, language, words.template(language, FN_HINT), ToastCard.FN, ToastCard.FN);
            String unbound = words.template(language, "projects_overlay.toast.hint.unbound");
            assertTrue(width(font, unbound) <= ToastCard.TEXT_W - ROUNDING_SLACK, unbound);
        }
    }

    /** The line with {@code usual} on the cap fits; {@code longName} still shows whole in the room left. */
    private static void fits(Font font, Language language, String template, String usual, String longName) {
        String[] around = ToastCard.around(template.replace("%s", "\u0000"));
        int before = width(font, around[0]);
        int after = width(font, around[1]);
        int line = before + ToastCard.HINT_SPACE + width(font, usual) + 2 * ToastCard.KEY_PAD
            + (after > 0 ? ToastCard.HINT_SPACE + after : 0);
        assertTrue(line <= ToastCard.TEXT_W - ROUNDING_SLACK, language + ": " + template + " is " + line + " wide");
        assertTrue(width(font, longName) <= ToastCard.keyRoom(ToastCard.TEXT_W, before, after),
            language + ": " + template + " cuts " + longName);
    }

    private static int width(Font font, String text) {
        if (text.isEmpty()) {
            return 0;
        }
        FontRenderContext frc = new FontRenderContext(null, true, true);
        return (int) Math.ceil(font.getStringBounds(text, frc).getWidth());
    }

    /** The toasts' small face (Figtree Medium at 7.5 GUI pixels, as font/small_*.json bakes it), or null. */
    private static Font figtree() {
        try (InputStream in = ToastHintTest.class.getResourceAsStream(
            "/assets/projects_overlay/font/figtree-medium.ttf")) {
            return in == null ? null : Font.createFont(Font.TRUETYPE_FONT, in).deriveFont(7.5f);
        } catch (Exception | Error e) {
            return null;
        }
    }
}
