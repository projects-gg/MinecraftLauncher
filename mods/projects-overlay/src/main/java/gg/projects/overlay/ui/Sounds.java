package gg.projects.overlay.ui;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.settings.OverlaySettings;
import net.minecraft.client.Minecraft;
import net.minecraft.client.resources.sounds.SimpleSoundInstance;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundSource;
import net.minecraft.util.RandomSource;

/**
 * The overlay's two cues (tools/build-sounds.py): a soft note for messages, two notes for calls and
 * invites. They play in the game's interface sound category, so its slider in Music &amp; Sounds applies on
 * top of the overlay's own volume. A burst of messages rings once.
 */
public final class Sounds {
    public enum Cue { MESSAGE, CALL }

    private static final Identifier MESSAGE = Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID, "ui.message");
    private static final Identifier CALL = Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID, "ui.call");
    private static final long QUIET_MS = 1_200;
    private static final RandomSource RANDOM = RandomSource.create();

    private static long lastPlayed = Long.MIN_VALUE / 2;

    private Sounds() {
    }

    /** Plays {@code cue} if the player wants it, at most once per {@link #QUIET_MS}. */
    public static void play(Cue cue, OverlaySettings settings, long now) {
        boolean on = cue == Cue.CALL ? settings.callSound() : settings.messageSound();
        int volume = cue == Cue.CALL ? settings.callVolume() : settings.messageVolume();
        if (!on || volume <= 0 || now - lastPlayed < QUIET_MS) {
            return;
        }
        lastPlayed = now;
        preview(cue, volume);
    }

    /** Plays {@code cue} at {@code volume} (0 to 100) now: the settings page's preview. */
    public static void preview(Cue cue, int volume) {
        if (volume <= 0) {
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        float gain = volume / (float) OverlaySettings.MAX_VOLUME;
        mc.getSoundManager().play(new SimpleSoundInstance(cue == Cue.CALL ? CALL : MESSAGE, SoundSource.UI, gain,
            1.0f, RANDOM, false, 0, SoundInstance.Attenuation.NONE, 0.0, 0.0, 0.0, true));
    }
}
