package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import org.junit.jupiter.api.Test;

/** CHAT2's frames from the panel, and emoji as the game's font can draw them (CHAT2 §3.2). */
class ProtocolChat2Test {
    private static String cp(int... points) {
        StringBuilder b = new StringBuilder();
        for (int p : points) {
            b.appendCodePoint(p);
        }
        return b.toString();
    }

    @Test
    void helloSaysTheModKnowsChat2() {
        JsonObject hello = Protocol.hello("tok", "0.2.0", "26.1.2", "tr_tr");
        assertTrue(hello.getAsJsonArray("features").toString().contains("\"chat2\""));
        assertTrue(hello.getAsJsonArray("features").toString().contains("\"connect\""));
    }

    @Test
    void emojiLoseWhatOnlyColourFontsDraw() {
        String heart = cp(0x2764, 0xFE0F);
        assertEquals(cp(0x2764), Protocol.plain(heart, 10), "VS16 would draw as a dashed box");
        String thumbTone = cp(0x1F44D, 0x1F3FD);
        assertEquals(cp(0x1F44D), Protocol.plain(thumbTone, 10), "a skin tone would draw as a coloured square");
        String keycap = cp('1', 0xFE0F, 0x20E3);
        assertEquals("1", Protocol.plain(keycap, 10), "the digit of a keycap stays");
        String textStyle = cp(0x263A, 0xFE0E);
        assertEquals(cp(0x263A), Protocol.plain(textStyle, 10));
        // A family's joiners were dropped already (format characters): its people stay side by side.
        String family = cp(0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467);
        assertEquals(cp(0x1F468, 0x1F469, 0x1F467), Protocol.plain(family, 10));
        // Flags stay as their two letter boxes; ordinary text is untouched.
        String flag = cp(0x1F1F9, 0x1F1F7);
        assertEquals(flag, Protocol.plain(flag, 10));
        assertEquals("Gel ❤ hemen", Protocol.plain("Gel " + heart + " hemen", 100));
        // What is dropped does not count against the length.
        assertEquals(cp(0x1F44D, 0x1F44D), Protocol.plain(thumbTone + thumbTone, 2));
    }

    @Test
    void sendCarriesAReplyAndAStickerOnlyWhenThere() {
        JsonObject plain = Protocol.send("c1", "d:1", "selam");
        assertNull(plain.get("reply"));
        assertNull(plain.get("sticker"));
        JsonObject reply = Protocol.send("c2", "d:1", "tamam", 1_800_000_000_123L);
        assertEquals(1_800_000_000_123L, Protocol.num(reply, "reply", 0));
        JsonObject sticker = Protocol.sendSticker("c3", "c:t1", "pl1/gg", 0);
        assertEquals("send", Protocol.type(sticker));
        assertEquals("pl1/gg", Protocol.str(sticker, "sticker", ""));
        assertNull(sticker.get("text"));
        assertNull(sticker.get("reply"));
        assertEquals(7, Protocol.num(Protocol.sendSticker("c4", "c:t1", "pl1/hi", 7), "reply", 0));
    }

    @Test
    void reactionsAndWritingHaveTheirFrames() {
        JsonObject react = Protocol.react("c:t1", 42, "👍", true);
        assertEquals("react", Protocol.type(react));
        assertEquals(42, Protocol.num(react, "sid", 0));
        assertEquals("👍", Protocol.str(react, "e", ""));
        assertTrue(Protocol.bool(react, "on", false));
        assertFalse(Protocol.bool(Protocol.react("c:t1", 42, "👍", false), "on", true));
        JsonObject typing = Protocol.typing("d:1");
        assertEquals("typing", Protocol.type(typing));
        assertEquals("d:1", Protocol.str(typing, "conv", ""));
    }

    @Test
    void idsAreWholePositiveNumbers() {
        JsonObject o = Protocol.parse("{\"t\":\"x\",\"a\":12,\"b\":-3,\"c\":\"7\"}");
        assertEquals(12, Protocol.id(o, "a"));
        assertEquals(0, Protocol.id(o, "b"));
        assertEquals(0, Protocol.id(o, "c"));
        assertEquals(0, Protocol.id(o, "missing"));
    }
}
