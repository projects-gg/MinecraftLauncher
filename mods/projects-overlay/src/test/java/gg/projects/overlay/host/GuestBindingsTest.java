package gg.projects.overlay.host;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.List;
import javax.crypto.Mac;
import javax.crypto.spec.SecretKeySpec;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/** The world's name binding file against the shared vectors (testdata/binding) and its own rules. */
class GuestBindingsTest {
    private static final JsonObject DIGESTS = Vectors.read("binding/digest.json");
    private static final JsonObject RULES = Vectors.read("binding/rules.json");
    private static final String EXAMPLE = Vectors.text("binding/projects-guests.example.json");
    private static final String KEY_ID = DIGESTS.get("keyId").getAsString();

    /** What the launcher writes into the header for a friend (PROTOCOL.md §9); the mod never computes it. */
    static String digest(String friendUserId) throws Exception {
        Mac mac = Mac.getInstance("HmacSHA256");
        mac.init(new SecretKeySpec(Vectors.hex(DIGESTS.get("bindingKeyHex").getAsString()), "HmacSHA256"));
        mac.update("projects guest binding v1".getBytes(StandardCharsets.US_ASCII));
        mac.update((byte) 0);
        mac.update(friendUserId.getBytes(StandardCharsets.UTF_8));
        return Base64.getUrlEncoder().withoutPadding().encodeToString(Arrays.copyOf(mac.doFinal(), 16));
    }

    @Test
    void theTestsDigestsAreTheVectorsOwn() throws Exception {
        byte[] key = Vectors.hex(DIGESTS.get("bindingKeyHex").getAsString());
        byte[] sha = MessageDigest.getInstance("SHA-256").digest(key);
        assertEquals(KEY_ID, Base64.getUrlEncoder().withoutPadding().encodeToString(Arrays.copyOf(sha, 6)));
        for (JsonElement e : DIGESTS.getAsJsonArray("digests")) {
            JsonObject d = e.getAsJsonObject();
            assertEquals(d.get("digest").getAsString(), digest(d.get("friend").getAsString()));
        }
    }

    /** The admission verdict by name as the session decides it (PROTOCOL.md §6.8 steps 3 and 4). */
    private static String verdict(GuestBindings bindings, String hostName, String name, String friend) {
        if (!Names.valid(name)) {
            return "invalid";
        }
        if (Names.same(name, hostName)) {
            return "host";
        }
        return bindings.check(name, friend) == GuestBindings.Verdict.TAKEN ? "taken" : "ok";
    }

    @Test
    void everyAdmissionCaseOfTheVectors() throws Exception {
        String hostName = RULES.get("hostName").getAsString();
        int checked = 0;
        for (JsonElement e : RULES.getAsJsonArray("cases")) {
            JsonObject c = e.getAsJsonObject();
            String file = c.has("fileKey") ? EXAMPLE.replace(KEY_ID, c.get("fileKey").getAsString()) : EXAMPLE;
            GuestBindings bindings = GuestBindings.parse(file, KEY_ID);
            String friend = digest(c.get("friend").getAsString());
            String name = c.get("name").getAsString();
            assertEquals(c.get("expect").getAsString(), verdict(bindings, hostName, name, friend), c.toString());
            if (c.has("then") && c.get("then").getAsString().startsWith("bound")) {
                assertTrue(bindings.bind(name, friend, 1_792_600_000L));
                assertEquals("taken", verdict(bindings, hostName, name, digest("123456789012345678")));
                assertEquals("ok", verdict(bindings, hostName, name.toLowerCase(), friend));
            }
            checked++;
        }
        assertEquals(12, checked);
    }

    @Test
    void theExampleFileReadsBackAsItWasWritten() {
        GuestBindings bindings = GuestBindings.parse(EXAMPLE, KEY_ID);
        assertEquals(2, bindings.size());
        GuestBindings again = GuestBindings.parse(bindings.toJson(), KEY_ID);
        assertEquals(bindings.entries(), again.entries());
        assertTrue(bindings.toJson().endsWith("\n"));
    }

    @Test
    void anotherKeysFileCountsAsEmptyAndIsRewrittenUnderOurs() throws Exception {
        GuestBindings bindings = GuestBindings.parse(EXAMPLE.replace(KEY_ID, "zzzzzzzz"), KEY_ID);
        assertEquals(0, bindings.size());
        assertTrue(bindings.bind("Notch", digest("345678901234567890"), 1_792_600_000L));
        assertTrue(bindings.toJson().contains("\"key\": \"" + KEY_ID + "\""));
    }

    @Test
    void aBrokenFileIsKeptAsideAndTheWorldStartsEmpty(@TempDir Path dir) throws Exception {
        Path file = dir.resolve(GuestBindings.FILE);
        List<String> log = new ArrayList<>();
        for (String broken : new String[] {"not json", "{\"v\":2,\"key\":\"sW9hzU3c\",\"names\":[]}",
            "{\"v\":1,\"key\":\"sW9hzU3c\",\"names\":[{\"name\":\"no\",\"friend\":\"1UbirWHKbvkTXKMZNmDDug\",\"last\":1}]}",
            "{\"v\":1,\"key\":\"sW9hzU3c\",\"names\":[{\"name\":\"Notch\",\"friend\":\"x\",\"last\":1}]}",
            "{\"v\":1,\"key\":\"sW9hzU3c\",\"names\":[{\"name\":\"Notch\",\"friend\":\"1UbirWHKbvkTXKMZNmDDug\",\"last\":1},"
                + "{\"name\":\"notch\",\"friend\":\"n7ppm8zILloa1X-qpWnqVA\",\"last\":2}]}"}) {
            Files.writeString(file, broken);
            GuestBindings bindings = GuestBindings.load(file, KEY_ID, log::add);
            assertEquals(0, bindings.size(), broken);
            assertFalse(Files.exists(file), broken);
            assertEquals(broken, Files.readString(dir.resolve(GuestBindings.FILE + ".bad")));
        }
        assertEquals(5, log.size());
        // No file: empty, nothing said.
        assertEquals(0, GuestBindings.load(dir.resolve("missing.json"), KEY_ID, log::add).size());
        assertEquals(5, log.size());
    }

    @Test
    void savingReplacesTheFileInOneStep(@TempDir Path dir) throws Exception {
        Path file = dir.resolve(GuestBindings.FILE);
        Files.writeString(file, EXAMPLE);
        GuestBindings bindings = GuestBindings.load(file, KEY_ID, s -> { });
        assertTrue(bindings.bind("Dinnerbone", digest("345678901234567890"), 1_792_700_000L));
        bindings.save(file);
        assertFalse(Files.exists(dir.resolve(GuestBindings.FILE + ".tmp")));
        GuestBindings back = GuestBindings.load(file, KEY_ID, s -> { });
        assertEquals(3, back.size());
        assertEquals(GuestBindings.Verdict.OURS, back.check("dinnerbone", digest("345678901234567890")));
    }

    @Test
    void lastMovesOnceASessionAndTheOldestNameGoesPastTheLimit() throws Exception {
        GuestBindings bindings = GuestBindings.parse(EXAMPLE, KEY_ID);
        String notch = digest("123456789012345678");
        assertTrue(bindings.bind("Notch", notch, 1_800_000_000L));
        assertFalse(bindings.bind("Notch", notch, 1_800_000_100L));
        assertFalse(bindings.bind("Notch", digest("345678901234567890"), 1_800_000_200L));
        GuestBindings full = GuestBindings.empty(KEY_ID);
        for (int i = 0; i < GuestBindings.MAX_ENTRIES; i++) {
            full.bind("player" + i, notch, 1_000L + i);
        }
        assertEquals(GuestBindings.MAX_ENTRIES, full.size());
        full.bind("newcomer", notch, 9_999L);
        assertEquals(GuestBindings.MAX_ENTRIES, full.size());
        assertEquals(GuestBindings.Verdict.FREE, full.check("player0", digest("345678901234567890")));
        assertEquals(GuestBindings.Verdict.OURS, full.check("newcomer", notch));
    }

    @Test
    void tooManyEntriesIsABrokenFile() {
        StringBuilder b = new StringBuilder("{\"v\":1,\"key\":\"sW9hzU3c\",\"names\":[");
        for (int i = 0; i <= GuestBindings.MAX_ENTRIES; i++) {
            b.append(i == 0 ? "" : ",").append("{\"name\":\"p").append(String.format("%04d", i))
                .append("\",\"friend\":\"1UbirWHKbvkTXKMZNmDDug\",\"last\":1}");
        }
        b.append("]}");
        assertThrows(IllegalArgumentException.class, () -> GuestBindings.parse(b.toString(), KEY_ID));
    }
}
