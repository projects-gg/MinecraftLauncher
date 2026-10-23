package gg.projects.overlay.settings;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class SettingsTest {
    @TempDir
    Path dir;

    @Test
    void defaultsShowEverythingWithTextAndSound() {
        OverlaySettings d = OverlaySettings.DEFAULTS;
        assertTrue(d.showsKind("message") && d.showsKind("call") && d.showsKind("invite") && d.showsKind("info"));
        assertTrue(d.showMessageText());
        assertTrue(d.messageSound() && d.callSound());
        assertEquals(OverlaySettings.Corner.BOTTOM_RIGHT, d.corner());
        assertEquals(OverlaySettings.Duration.NORMAL, d.duration());
        assertFalse(d.reduceMotion());
        assertTrue(d.isDefault());
    }

    @Test
    void volumesStayInRange() {
        OverlaySettings s = OverlaySettings.DEFAULTS.withMessageVolume(140).withCallVolume(-3);
        assertEquals(100, s.messageVolume());
        assertEquals(0, s.callVolume());
    }

    @Test
    void resetKeepsTheFirstTimeHint() {
        OverlaySettings s = OverlaySettings.DEFAULTS.withHintSeen(true).withMessageToasts(false)
            .withCorner(OverlaySettings.Corner.TOP_RIGHT);
        assertFalse(s.isDefault());
        OverlaySettings reset = s.reset();
        assertTrue(reset.isDefault());
        assertTrue(reset.hintSeen());
        // The hint alone is not a display preference.
        assertTrue(OverlaySettings.DEFAULTS.withHintSeen(true).isDefault());
    }

    @Test
    void recentStickersPutTheLastSentFirstAndKeepEight() {
        OverlaySettings s = OverlaySettings.DEFAULTS;
        for (String id : new String[] {"a", "b", "c", "d", "e", "f", "g", "h", "i"}) {
            s = s.withStickerUsed("pl1/" + id);
        }
        assertEquals(OverlaySettings.RECENT_STICKERS, s.recentStickers().size());
        assertEquals("pl1/i", s.recentStickers().get(0));
        s = s.withStickerUsed("pl1/e");
        assertEquals(List.of("pl1/e", "pl1/i", "pl1/h", "pl1/g", "pl1/f", "pl1/d", "pl1/c", "pl1/b"),
            s.recentStickers());
        // They are not a display preference: a reset keeps them, and they do not make settings non-default.
        assertTrue(s.isDefault());
        assertEquals(s.recentStickers(), s.withCorner(OverlaySettings.Corner.TOP_RIGHT).reset().recentStickers());
    }

    @Test
    void onlyStickerReferencesAreReadBack() {
        OverlaySettings s = SettingsStore.parse("{\"recentStickers\":[\"pl1/gg\",\"../../x\",5,\"pl1/gg\","
            + "\"pl1/hi\"]}");
        assertEquals(List.of("pl1/gg", "pl1/hi"), s.recentStickers());
        assertEquals(List.of(), SettingsStore.parse("{\"recentStickers\":\"pl1/gg\"}").recentStickers());
    }

    @Test
    void unknownKindsCountAsMessages() {
        OverlaySettings s = OverlaySettings.DEFAULTS.withMessageToasts(false);
        assertFalse(s.showsKind("something-new"));
        assertTrue(s.showsKind("call"));
    }

    @Test
    void writesAndReadsEveryField() throws IOException {
        OverlaySettings s = new OverlaySettings(false, true, false, true, false, OverlaySettings.Duration.LONG,
            OverlaySettings.Corner.TOP_RIGHT, true, false, 35, true, 90, true, List.of("pl1/gg", "pl1/hi"));
        Path file = dir.resolve(SettingsStore.FILE_NAME);
        SettingsStore.write(file, s);
        assertEquals(s, SettingsStore.read(file));
        assertFalse(Files.exists(dir.resolve(SettingsStore.FILE_NAME + ".tmp")));
    }

    @Test
    void aMissingFileMeansDefaults() {
        assertEquals(OverlaySettings.DEFAULTS, SettingsStore.read(dir.resolve("nothing.json")));
    }

    @Test
    void aDamagedFileMeansDefaults() throws IOException {
        Path file = dir.resolve(SettingsStore.FILE_NAME);
        Files.writeString(file, "{\"toasts\": {\"message\": fal", StandardCharsets.UTF_8);
        assertEquals(OverlaySettings.DEFAULTS, SettingsStore.read(file));
        Files.writeString(file, "[1, 2]", StandardCharsets.UTF_8);
        assertEquals(OverlaySettings.DEFAULTS, SettingsStore.read(file));
    }

    @Test
    void oneBadValueKeepsTheOthers() {
        OverlaySettings s = SettingsStore.parse("{\"version\":1,\"toasts\":{\"message\":\"yes\",\"call\":false},"
            + "\"corner\":\"left\",\"duration\":\"short\",\"sounds\":{\"messageVolume\":\"loud\",\"callVolume\":250},"
            + "\"future\":{\"x\":1}}");
        assertTrue(s.messageToasts());
        assertFalse(s.callToasts());
        assertEquals(OverlaySettings.Corner.BOTTOM_RIGHT, s.corner());
        assertEquals(OverlaySettings.Duration.SHORT, s.duration());
        assertEquals(OverlaySettings.DEFAULTS.messageVolume(), s.messageVolume());
        assertEquals(100, s.callVolume());
    }

    @Test
    void theStoreSavesInTheBackgroundAndOnClose() throws IOException {
        SettingsStore store = new SettingsStore(dir);
        assertEquals(OverlaySettings.DEFAULTS, store.get());
        store.set(store.get().withShowMessageText(false));
        store.set(store.get().withMessageVolume(20));
        assertEquals(20, store.get().messageVolume());
        store.close();
        OverlaySettings saved = SettingsStore.read(dir.resolve(SettingsStore.FILE_NAME));
        assertFalse(saved.showMessageText());
        assertEquals(20, saved.messageVolume());
        String json = Files.readString(dir.resolve(SettingsStore.FILE_NAME), StandardCharsets.UTF_8);
        assertTrue(json.contains("\"version\": 1"));
    }
}
