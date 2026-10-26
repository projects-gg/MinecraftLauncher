package gg.projects.overlay.emoji;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.Test;

class LookTest {
    @Test
    void coloursAndStylesSplitTheLookAsTheGameDoes() {
        List<Look.Span> heart = Look.spans("&c❤");
        assertEquals(List.of(new Look.Span("❤", 0xFF5555, false, false, false, false)), heart);
        List<Look.Span> wow = Look.spans("&6&lWOW");
        assertEquals(List.of(new Look.Span("WOW", 0xFFAA00, true, false, false, false)), wow);
        // A colour ends the styles before it; &r goes back to the line's own colour.
        List<Look.Span> mixed = Look.spans("&lA&eB&rC");
        assertEquals(3, mixed.size());
        assertEquals(new Look.Span("A", -1, true, false, false, false), mixed.get(0));
        assertEquals(new Look.Span("B", 0xFFFF55, false, false, false, false), mixed.get(1));
        assertEquals(new Look.Span("C", -1, false, false, false, false), mixed.get(2));
        // Codes are read without case.
        assertEquals(0x55FFFF, Look.spans("&B(╥﹏╥)").get(0).colour());
        assertEquals("¯\\_(ツ)_/¯", Look.plain("¯\\_(ツ)_/¯"));
        assertEquals("(・・?)", Look.plain("&e(・・?)"));
    }

    @Test
    void looksThatBreakTheRulesAreRefused() {
        assertNull(Look.spans("&"));
        assertNull(Look.spans("&x❤"));
        assertNull(Look.spans("&kmagic"), "no obfuscated text");
        assertNull(Look.spans("&c"), "a colour alone shows nothing");
        assertNull(Look.spans("&c "));
        assertNull(Look.spans("§c❤"));
        assertNull(Look.spans("a\nb"));
        assertNull(Look.spans(new String(Character.toChars(0xE106))));
        assertNull(Look.spans("x".repeat(49)));
        assertEquals(1, Look.spans("&c" + "x".repeat(48)).size());
        assertEquals("", Look.plain("&"));
        assertNull(Look.spans(null));
    }

    @Test
    void privateUseIsTheServerFontsArea() {
        assertTrue(Look.isPrivateUse(0xE106));
        assertTrue(Look.isPrivateUse(0xF8FF));
        assertTrue(Look.isPrivateUse(0x100000));
        assertFalse(Look.isPrivateUse(0x2764));
    }
}
