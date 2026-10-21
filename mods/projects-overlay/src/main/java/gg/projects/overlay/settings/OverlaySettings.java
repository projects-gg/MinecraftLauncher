package gg.projects.overlay.settings;

/**
 * How the overlay looks and sounds on this computer: the player's fine-tuning from the panel's settings
 * page. The launcher decides whether a toast is sent; these decide how (or whether) the game shows it.
 * Immutable; the {@code with…} methods return a changed copy.
 */
public record OverlaySettings(
    boolean messageToasts,
    boolean callToasts,
    boolean inviteToasts,
    boolean infoToasts,
    boolean showMessageText,
    Duration duration,
    Corner corner,
    boolean reduceMotion,
    boolean messageSound,
    int messageVolume,
    boolean callSound,
    int callVolume,
    boolean hintSeen) {

    public static final int MAX_VOLUME = 100;

    /** How long a toast stays: a multiplier on the kind's own time (and the game's "Notification Time"). */
    public enum Duration {
        SHORT("short", 0.65f),
        NORMAL("normal", 1f),
        LONG("long", 1.6f);

        public final String id;
        public final float factor;

        Duration(String id, float factor) {
            this.id = id;
            this.factor = factor;
        }

        static Duration byId(String id, Duration fallback) {
            for (Duration d : values()) {
                if (d.id.equals(id)) {
                    return d;
                }
            }
            return fallback;
        }
    }

    /** Where toasts stack. */
    public enum Corner {
        BOTTOM_RIGHT("bottom-right"),
        TOP_RIGHT("top-right");

        public final String id;

        Corner(String id) {
            this.id = id;
        }

        static Corner byId(String id, Corner fallback) {
            for (Corner c : values()) {
                if (c.id.equals(id)) {
                    return c;
                }
            }
            return fallback;
        }
    }

    public static final OverlaySettings DEFAULTS = new OverlaySettings(true, true, true, true, true,
        Duration.NORMAL, Corner.BOTTOM_RIGHT, false, true, 70, true, 85, false);

    public OverlaySettings {
        duration = duration == null ? Duration.NORMAL : duration;
        corner = corner == null ? Corner.BOTTOM_RIGHT : corner;
        messageVolume = clampVolume(messageVolume);
        callVolume = clampVolume(callVolume);
    }

    public static int clampVolume(int volume) {
        return Math.max(0, Math.min(MAX_VOLUME, volume));
    }

    /** Whether toasts of this kind ({@code message}, {@code call}, {@code invite}, {@code info}) show. */
    public boolean showsKind(String kind) {
        return switch (kind) {
            case "call" -> callToasts;
            case "invite" -> inviteToasts;
            case "info" -> infoToasts;
            default -> messageToasts;
        };
    }

    /** The display preferences equal the defaults (the first-time hint does not count). */
    public boolean isDefault() {
        return withHintSeen(DEFAULTS.hintSeen).equals(DEFAULTS);
    }

    /** The defaults, keeping what is not a display preference (the first-time hint). */
    public OverlaySettings reset() {
        return DEFAULTS.withHintSeen(hintSeen);
    }

    public OverlaySettings withMessageToasts(boolean v) {
        return new OverlaySettings(v, callToasts, inviteToasts, infoToasts, showMessageText, duration, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, hintSeen);
    }

    public OverlaySettings withCallToasts(boolean v) {
        return new OverlaySettings(messageToasts, v, inviteToasts, infoToasts, showMessageText, duration, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, hintSeen);
    }

    public OverlaySettings withInviteToasts(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, v, infoToasts, showMessageText, duration, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, hintSeen);
    }

    public OverlaySettings withInfoToasts(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, v, showMessageText, duration, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, hintSeen);
    }

    public OverlaySettings withShowMessageText(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, v, duration, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, hintSeen);
    }

    public OverlaySettings withDuration(Duration v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, v, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, hintSeen);
    }

    public OverlaySettings withCorner(Corner v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration, v,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, hintSeen);
    }

    public OverlaySettings withReduceMotion(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, v, messageSound, messageVolume, callSound, callVolume, hintSeen);
    }

    public OverlaySettings withMessageSound(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, v, messageVolume, callSound, callVolume, hintSeen);
    }

    public OverlaySettings withMessageVolume(int v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, v, callSound, callVolume, hintSeen);
    }

    public OverlaySettings withCallSound(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, v, callVolume, hintSeen);
    }

    public OverlaySettings withCallVolume(int v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, v, hintSeen);
    }

    public OverlaySettings withHintSeen(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, callVolume, v);
    }
}
