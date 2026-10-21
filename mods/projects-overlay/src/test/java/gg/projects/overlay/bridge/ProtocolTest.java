package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;

import com.google.gson.JsonObject;
import org.junit.jupiter.api.Test;

class ProtocolTest {
    @Test
    void parsesOnlyObjectsWithAType() {
        assertNotNull(Protocol.parse("{\"t\":\"welcome\",\"proto\":1}"));
        assertNull(Protocol.parse("{\"proto\":1}"));
        assertNull(Protocol.parse("[1,2]"));
        assertNull(Protocol.parse("POST / HTTP/1.1"));
        assertNull(Protocol.parse("{\"t\":"));
        assertNull(Protocol.parse(""));
        assertNull(Protocol.parse(null));
    }

    @Test
    void readsFieldsDefensively() {
        JsonObject o = Protocol.parse("{\"t\":\"x\",\"n\":\"12\",\"b\":1,\"s\":5}");
        assertEquals(-1, Protocol.num(o, "n", -1));
        assertEquals(false, Protocol.bool(o, "b", false));
        assertEquals("d", Protocol.str(o, "s", "d"));
        assertEquals("d", Protocol.str(o, "missing", "d"));
    }

    @Test
    void helloCarriesVersionAndToken() {
        JsonObject hello = Protocol.hello("tok", "0.1.0", "26.1.2", "tr_tr");
        assertEquals("hello", Protocol.type(hello));
        assertEquals(Protocol.VERSION, Protocol.num(hello, "proto", 0));
        assertEquals("tok", Protocol.str(hello, "token", ""));
    }

    @Test
    void plainDropsFormattingAndControls() {
        String section = new String(Character.toChars(0xA7));
        String rlo = new String(Character.toChars(0x202E));
        String zwsp = new String(Character.toChars(0x200B));
        assertEquals("cKırmızı", Protocol.plain(section + "cKırmızı", 100));
        assertEquals("abc", Protocol.plain("a" + rlo + "b" + zwsp + "c", 100));
        assertEquals("iki satır", Protocol.plain("iki\n\nsatır\t", 100));
        assertEquals("Şükrü", Protocol.plain("  Şükrü  ", 100));
        assertEquals("", Protocol.plain(null, 10));
    }

    @Test
    void plainCapsByCodePoints() {
        assertEquals("ğüş", Protocol.plain("ğüşıöç", 3));
        String smile = new String(Character.toChars(0x1F600));
        assertEquals(smile + smile, Protocol.plain(smile + smile + smile, 2));
    }
}
