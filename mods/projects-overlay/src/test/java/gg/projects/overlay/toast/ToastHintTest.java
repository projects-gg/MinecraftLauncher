package gg.projects.overlay.toast;

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
 * the Fn line names both keys, and every hint line stays inside the card with the keys' names on their caps.
 */
class ToastHintTest {
    /** The hints that name the overlay key. */
    private static final List<String> KEY_HINTS = List.of("projects_overlay.toast.hint.open",
        "projects_overlay.toast.hint.call", "projects_overlay.toast.hint.join", "projects_overlay.toast.hint.manage");
    private static final String FN_HINT = "projects_overlay.toast.hint.fn";
    private static final String UNBOUND = "projects_overlay.toast.hint.unbound";
    /** The game rounds each glyph's advance to its pixel grid: the words keep this much room for it. */
    private static final int ROUNDING_SLACK = 8;

    @Test
    void aKeyNameGetsTheRoomTheRestOfTheLineLeaves() {
        assertEquals(41, ToastCard.keyRoom(ToastCard.TEXT_W, ToastCard.TEXT_W - 41));
        assertEquals(ToastCard.MAX_KEY_NAME, ToastCard.keyRoom(ToastCard.TEXT_W, 30), "short words: the old cap");
        assertEquals(ToastCard.MIN_KEY_NAME, ToastCard.keyRoom(ToastCard.TEXT_W, 200), "never cut to nothing");
    }

    @Test
    void aLineIsItsPartsWithASpaceBetweenThoseThatAreThere() {
        int s = ToastCard.HINT_SPACE;
        // "Açmak için [F7] tuşuna bas": words, a cap, words.
        assertEquals(37 + s + 14 + s + 36, ToastCard.lineWidth(new int[] {37, 36}, new int[] {14}));
        // "Açılmazsa [Fn] + [F7] tuşlarına bas": the "+" between the caps is a word of its own.
        assertEquals(29 + s + 14 + s + 5 + s + 14 + s + 48,
            ToastCard.lineWidth(new int[] {29, 5, 48}, new int[] {14, 14}));
        // A sentence that starts with its key, and words alone.
        assertEquals(14 + s + 40, ToastCard.lineWidth(new int[] {0, 40}, new int[] {14}));
        assertEquals(83, ToastCard.lineWidth(new int[] {83}, new int[] {}));
    }

    @Test
    void theKeysGoWhereTheSentenceHasThem() {
        assertEquals(List.of("Açmak için", "tuşuna bas"), ToastCard.split("Açmak için \u0000 tuşuna bas"));
        assertEquals(List.of("Press", "to open"), ToastCard.split("Press \u0000 to open"));
        assertEquals(List.of("Açılmazsa", "+", "tuşlarına bas"),
            ToastCard.split("Açılmazsa \u0000 + \u0000 tuşlarına bas"));
        assertEquals(List.of("If nothing happens, press", "+", ""),
            ToastCard.split("If nothing happens, press \u0000 + \u0000"));
        assertEquals(List.of("Açmak için Kontroller'den bir tuş ata"),
            ToastCard.split("Açmak için Kontroller'den bir tuş ata"));
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
        // Both keys named, the way laptop makers write it, and the condition said ("Olmazsa" also reads "if not
        // allowed").
        assertEquals("Açılmazsa %s + %s tuşlarına bas", words.template(Language.TURKISH, FN_HINT));
        assertEquals("If nothing happens, press %s + %s", words.template(Language.ENGLISH, FN_HINT));
        assertEquals("Açmak için Kontroller'den bir tuş ata", words.template(Language.TURKISH, UNBOUND));
        assertEquals("To open, set a key in Controls", words.template(Language.ENGLISH, UNBOUND));
        assertEquals("Fn", ToastCard.FN);
    }

    @Test
    void everyHintLineFitsTheCard() throws Exception {
        Font font = figtree();
        Assumptions.assumeTrue(font != null, "no font rasteriser in this JVM");
        Dictionary words = Dictionary.shared();
        for (Language language : Language.values()) {
            for (String key : KEY_HINTS) {
                String template = words.template(language, key);
                fits(font, language, template, "F7");
                // A long name (a rebound key) still shows whole beside the words.
                assertTrue(width(font, "Sağ Shift") <= room(font, template, 0, "Sağ Shift"), template);
            }
            // The Fn line shows for F1 to F12 only: F12 is its widest key.
            fits(font, language, words.template(language, FN_HINT), ToastCard.FN, "F12");
            fits(font, language, words.template(language, UNBOUND));
        }
    }

    /** The line with {@code keys} on its caps fits the card with room for the game's rounding. */
    private static void fits(Font font, Language language, String template, String... keys) {
        int line = line(font, template, keys);
        assertTrue(line <= ToastCard.TEXT_W - ROUNDING_SLACK, language + ": " + template + " is " + line + " wide");
    }

    /** The room key {@code index} gets in the line, as {@code ToastCard.Hint.of} gives it. */
    private static int room(Font font, String template, int index, String... keys) {
        int nameWidth = width(font, keys[index]);
        return ToastCard.keyRoom(ToastCard.TEXT_W, line(font, template, keys) - nameWidth);
    }

    private static int line(Font font, String template, String... keys) {
        List<String> parts = ToastCard.split(template.replace("%s", "\u0000"));
        int[] words = new int[parts.size()];
        for (int i = 0; i < words.length; i++) {
            words[i] = width(font, parts.get(i));
        }
        int[] caps = new int[keys.length];
        for (int i = 0; i < caps.length; i++) {
            caps[i] = width(font, keys[i]) + 2 * ToastCard.KEY_PAD;
        }
        return ToastCard.lineWidth(words, caps);
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
