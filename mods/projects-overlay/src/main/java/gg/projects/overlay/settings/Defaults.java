package gg.projects.overlay.settings;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import java.util.function.UnaryOperator;

/**
 * What the settings page's "Varsayılana dön" puts back, and what its "Geri al" restores: the overlay's display
 * settings ({@link OverlaySettings#reset()}) and every key the page offers (the overlay key, kept by the game in
 * {@code options.txt}). The button is on while any of them differs from its default (OVL-FIX: a changed key alone
 * left it off). The language is not among them: it is the player's own pick, kept by the launcher for every game.
 */
public final class Defaults {
    /** A key the settings page offers ("Kısayol"), as the reset sees it. */
    public interface Key {
        /** The key as {@code options.txt} keeps it ({@code key.keyboard.f7}). */
        String saved();

        boolean isDefault();

        /** Binds the key {@code saved} names (null: the default) and keeps it, as the game's Controls do. */
        void bind(String saved);
    }

    /** What a reset took away: the settings before it and each key's saved form (null where it was the default). */
    public record Undo(OverlaySettings settings, List<String> keys) {
    }

    private Defaults() {
    }

    /** Everything the reset covers is at its default: there is nothing to put back. */
    public static boolean all(OverlaySettings settings, List<? extends Key> keys) {
        if (!settings.isDefault()) {
            return false;
        }
        for (Key key : keys) {
            if (!key.isDefault()) {
                return false;
            }
        }
        return true;
    }

    /**
     * Puts back the defaults: the display settings through {@code apply} (the first-time hint and the recent stickers
     * stay), each changed key to its default. Returns what "Geri al" needs, or null when everything was at its default.
     */
    public static Undo reset(OverlaySettings current, Consumer<UnaryOperator<OverlaySettings>> apply,
        List<? extends Key> keys) {
        if (all(current, keys)) {
            return null;
        }
        List<String> saved = new ArrayList<>(keys.size());
        for (Key key : keys) {
            saved.add(key.isDefault() ? null : key.saved());
        }
        if (!current.isDefault()) {
            apply.accept(OverlaySettings::reset);
        }
        for (Key key : keys) {
            if (!key.isDefault()) {
                key.bind(null);
            }
        }
        return new Undo(current, saved);
    }

    /**
     * "Geri al": the settings and keys as they were before the reset. What is not a display preference (the first-time
     * hint, the recent stickers) stays as it is now.
     */
    public static void undo(Undo undo, Consumer<UnaryOperator<OverlaySettings>> apply, List<? extends Key> keys) {
        OverlaySettings back = undo.settings();
        apply.accept(now -> back.withHintSeen(now.hintSeen()).withRecentStickers(now.recentStickers()));
        for (int i = 0; i < keys.size() && i < undo.keys().size(); i++) {
            String saved = undo.keys().get(i);
            if (saved != null) {
                keys.get(i).bind(saved);
            }
        }
    }
}
