package gg.projects.overlay.host;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.nio.charset.StandardCharsets;
import org.junit.jupiter.api.Test;

/** The gate's header against the shared vectors (testdata/gate/plg1.json) and its own edges. */
class PlgHeaderTest {
    private static final JsonObject VECTORS = Vectors.read("gate/plg1.json");

    private static byte[] key() {
        return PlgHeader.gateKey(VECTORS.get("bridgeToken").getAsString());
    }

    @Test
    void theGateKeyComesFromTheBridgeTokensAsciiBytes() {
        assertArrayEquals(Vectors.hex(VECTORS.get("gateKeyHex").getAsString()), key());
    }

    @Test
    void everyValidHeaderVouchesForWhatItSays() {
        int checked = 0;
        for (JsonElement e : VECTORS.getAsJsonArray("valid")) {
            JsonObject v = e.getAsJsonObject();
            byte[] header = Vectors.hex(v.get("headerHex").getAsString());
            GuestIdentity id = PlgHeader.verify(key(), header);
            assertNotNull(id, v.toString());
            JsonObject f = v.getAsJsonObject("fields");
            assertEquals(f.get("id").getAsString(), id.id());
            assertEquals(f.get("conn").getAsLong(), id.conn());
            assertEquals(f.get("guest").getAsString(), id.guest());
            assertEquals(f.get("name").getAsString(), id.name());
            assertEquals(f.get("premium").getAsBoolean(), id.premium());
            assertEquals(f.get("friend").getAsString(), id.friend());
            // The same bytes come out of our own writer (the development launcher uses it).
            String json = new String(Vectors.hex(v.get("jsonHex").getAsString()), StandardCharsets.UTF_8);
            assertArrayEquals(header, PlgHeader.build(key(), json));
            checked++;
        }
        assertEquals(2, checked);
    }

    @Test
    void everyInvalidHeaderIsRefused() {
        int checked = 0;
        for (JsonElement e : VECTORS.getAsJsonArray("invalid")) {
            JsonObject v = e.getAsJsonObject();
            assertNull(PlgHeader.verify(key(), Vectors.hex(v.get("headerHex").getAsString())),
                v.get("case").getAsString());
            checked++;
        }
        assertEquals(4, checked);
    }

    @Test
    void theLengthIsKnownFromTheFirstSixBytes() {
        byte[] good = Vectors.hex(VECTORS.getAsJsonArray("valid").get(0).getAsJsonObject().get("headerHex")
            .getAsString());
        assertEquals(good.length, PlgHeader.length(good));
        assertEquals(-1, PlgHeader.length(new byte[] {'P', 'L', 'G', '2', 0, 10}));
        assertEquals(-1, PlgHeader.length(new byte[] {'P', 'L', 'G', '1', 0, 0}));
        assertEquals(-1, PlgHeader.length(new byte[] {'P', 'L', 'G', '1', 0x04, 0x01}));
        assertEquals(PlgHeader.MAX_BYTES, PlgHeader.length(new byte[] {'P', 'L', 'G', '1', 0x04, 0x00}));
        assertEquals(-1, PlgHeader.length(new byte[] {'P', 'L', 'G'}));
    }

    @Test
    void aHeaderWithoutEveryFieldInItsFormIsRefusedEvenWithAGoodMac() {
        String[] bad = {
            "{\"v\":2,\"id\":\"p1\",\"conn\":1,\"guest\":\"g1\",\"name\":\"Notch\",\"premium\":false,\"friend\":\"1UbirWHKbvkTXKMZNmDDug\"}",
            "{\"v\":1,\"id\":\"p 1\",\"conn\":1,\"guest\":\"g1\",\"name\":\"Notch\",\"premium\":false,\"friend\":\"1UbirWHKbvkTXKMZNmDDug\"}",
            "{\"v\":1,\"id\":\"p1\",\"conn\":0,\"guest\":\"g1\",\"name\":\"Notch\",\"premium\":false,\"friend\":\"1UbirWHKbvkTXKMZNmDDug\"}",
            "{\"v\":1,\"id\":\"p1\",\"conn\":1.5,\"guest\":\"g1\",\"name\":\"Notch\",\"premium\":false,\"friend\":\"1UbirWHKbvkTXKMZNmDDug\"}",
            "{\"v\":1,\"id\":\"p1\",\"conn\":1,\"guest\":\"g0\",\"name\":\"Notch\",\"premium\":false,\"friend\":\"1UbirWHKbvkTXKMZNmDDug\"}",
            "{\"v\":1,\"id\":\"p1\",\"conn\":1,\"guest\":\"g1\",\"name\":\"\",\"premium\":false,\"friend\":\"1UbirWHKbvkTXKMZNmDDug\"}",
            "{\"v\":1,\"id\":\"p1\",\"conn\":1,\"guest\":\"g1\",\"name\":\"Notch\",\"premium\":\"no\",\"friend\":\"1UbirWHKbvkTXKMZNmDDug\"}",
            "{\"v\":1,\"id\":\"p1\",\"conn\":1,\"guest\":\"g1\",\"name\":\"Notch\",\"premium\":false,\"friend\":\"short\"}",
            "[1,2,3]",
        };
        for (String json : bad) {
            assertNull(PlgHeader.verify(key(), PlgHeader.build(key(), json)), json);
        }
        assertThrows(IllegalArgumentException.class, () -> PlgHeader.build(key(), "x".repeat(1025)));
    }

    @Test
    void anotherLaunchsKeyDoesNotOpenTheGate() {
        byte[] header = PlgHeader.build(key(),
            "{\"v\":1,\"id\":\"p1\",\"conn\":7,\"guest\":\"g7\",\"name\":\"Notch\",\"premium\":true,\"friend\":\"1UbirWHKbvkTXKMZNmDDug\"}");
        assertNotNull(PlgHeader.verify(key(), header));
        assertNull(PlgHeader.verify(PlgHeader.gateKey("another-token-of-the-launcher-0123456789abcd"), header));
        GuestIdentity id = PlgHeader.verify(key(), header);
        assertTrue(id.premium());
        assertFalse(id.name().isEmpty());
    }
}
