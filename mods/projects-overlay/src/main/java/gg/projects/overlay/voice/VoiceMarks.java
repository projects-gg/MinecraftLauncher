package gg.projects.overlay.voice;

import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Icon;

/**
 * What is off for someone in a voice room, as the overlay signs it beside them (VOICE-KEYS; the launcher's
 * {@code features/voice/rules.ts} {@code voiceMark} and {@code faceMark}): nothing while their microphone and sound are
 * on; deafened alone when deafened (the microphone is off with it, as Discord shows it), else the microphone; in ash,
 * a moderator's mute in flare (it holds whatever they do). Pure.
 */
public final class VoiceMarks {
    /** A sign and its ink. */
    public record Mark(Icon icon, int ink) {
    }

    private VoiceMarks() {
    }

    /** Their own choice's sign: deafened alone, else the microphone; null while both are on. */
    public static Icon own(boolean mute, boolean deaf) {
        return deaf ? Icon.DEAF : mute ? Icon.MIC_OFF : null;
    }

    /** The one sign a small face carries (the panel's strip): a moderator's mute first, then theirs. */
    public static Mark face(boolean mute, boolean deaf, boolean mutedByModerator) {
        if (mutedByModerator) {
            return new Mark(Icon.MIC_OFF, Theme.FLARE_300);
        }
        Icon own = own(mute, deaf);
        return own == null ? null : new Mark(own, Theme.ASH_300);
    }
}
