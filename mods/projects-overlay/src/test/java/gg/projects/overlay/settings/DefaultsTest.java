package gg.projects.overlay.settings;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;
import java.util.function.UnaryOperator;
import org.junit.jupiter.api.Test;

/** "Varsayılana dön" and "Geri al" over the display settings and the overlay key (OVL-FIX). */
class DefaultsTest {
    /** A key as options.txt keeps it, with a log of what was bound. */
    private static final class FakeKey implements Defaults.Key {
        static final String DEFAULT = "key.keyboard.f7";
        String saved = DEFAULT;
        final List<String> bound = new ArrayList<>();

        @Override
        public String saved() {
            return saved;
        }

        @Override
        public boolean isDefault() {
            return DEFAULT.equals(saved);
        }

        @Override
        public void bind(String to) {
            saved = to == null ? DEFAULT : to;
            bound.add(saved);
        }
    }

    private final AtomicReference<OverlaySettings> settings = new AtomicReference<>(OverlaySettings.DEFAULTS);
    private final Consumer<UnaryOperator<OverlaySettings>> apply = op -> settings.set(op.apply(settings.get()));

    @Test
    void aChangedKeyAloneHasSomethingToPutBack() {
        FakeKey key = new FakeKey();
        assertTrue(Defaults.all(settings.get(), List.of(key)));
        key.saved = "key.keyboard.f8";
        // The tester's case: only "Arayüzü aç ve kapat" changed, and "Varsayılana dön" stayed off.
        assertFalse(Defaults.all(settings.get(), List.of(key)));
    }

    @Test
    void theResetPutsTheKeyBackAndTheUndoBringsItAgain() {
        FakeKey key = new FakeKey();
        key.saved = "key.keyboard.g";
        Defaults.Undo undo = Defaults.reset(settings.get(), apply, List.of(key));
        assertEquals(FakeKey.DEFAULT, key.saved);
        assertTrue(Defaults.all(settings.get(), List.of(key)));
        // The settings were at their defaults: they were not written at all.
        assertEquals(OverlaySettings.DEFAULTS, settings.get());
        Defaults.undo(undo, apply, List.of(key));
        assertEquals("key.keyboard.g", key.saved);
        assertEquals(List.of(FakeKey.DEFAULT, "key.keyboard.g"), key.bound);
    }

    @Test
    void anUnchangedKeyIsNotTouchedByTheResetOrItsUndo() {
        FakeKey key = new FakeKey();
        settings.set(OverlaySettings.DEFAULTS.withCorner(OverlaySettings.Corner.TOP_RIGHT).withReduceMotion(true));
        Defaults.Undo undo = Defaults.reset(settings.get(), apply, List.of(key));
        assertTrue(settings.get().isDefault());
        Defaults.undo(undo, apply, List.of(key));
        assertEquals(OverlaySettings.Corner.TOP_RIGHT, settings.get().corner());
        assertTrue(settings.get().reduceMotion());
        assertTrue(key.bound.isEmpty());
    }

    @Test
    void nothingToPutBackMeansNoUndo() {
        FakeKey key = new FakeKey();
        assertNull(Defaults.reset(settings.get(), apply, List.of(key)));
        assertNull(Defaults.reset(settings.get(), apply, List.of()));
        assertTrue(key.bound.isEmpty());
    }

    @Test
    void theUndoKeepsWhatIsNotADisplayPreference() {
        settings.set(OverlaySettings.DEFAULTS.withMessageSound(false));
        Defaults.Undo undo = Defaults.reset(settings.get(), apply, List.of());
        // Meanwhile the first-time hint was seen and a sticker went out: both stay after "Geri al".
        settings.set(settings.get().withHintSeen(true).withStickerUsed("pl1/gg"));
        Defaults.undo(undo, apply, List.of());
        assertFalse(settings.get().messageSound());
        assertTrue(settings.get().hintSeen());
        assertEquals(List.of("pl1/gg"), settings.get().recentStickers());
    }
}
