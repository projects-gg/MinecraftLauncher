package gg.projects.overlay.settings;

import java.util.ArrayList;
import java.util.List;

/**
 * How the overlay looks and sounds on this computer: the player's fine-tuning from the panel's settings
 * page. The launcher decides whether a toast is sent; these decide how (or whether) the game shows it.
 * {@code keyUsed} and {@code fnNeeded} are no preferences: what this computer taught the toasts' key hint (KEY-HINT,
 * {@link gg.projects.overlay.keys.KeyHint}): the overlay key has opened the panel here, and its F-key reaches the game
 * only with Fn ("Açmak için [Fn] + [F7] tuşlarına bas"). {@code showKeyHint}: "Açma ipucunu göster", the toasts' one
 * line that says which key opens the panel. {@code showSpeakers} (VOICE-O): "Konuşanları göster", the HUD's corner of
 * who speaks in the player's voice room. {@code recentChatEmojis} (OVL-SOC): the panel picker's recent emoji, no
 * preference either. Immutable; the {@code with…} methods return a changed copy.
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
    boolean keyUsed,
    List<String> recentStickers,
    List<String> recentEmojis,
    boolean showSpeakers,
    List<String> recentChatEmojis,
    boolean showKeyHint,
    boolean fnNeeded,
    List<String> cardsOff) {

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

    /**
     * OVL-SOC2: the newer card kinds the player may turn off in game ("Oyunda gösterilecek kartlar"), as the launcher
     * names their groups: friend and crew requests, friends coming online, crew plans, missed calls.
     */
    public static final List<String> CARD_GROUPS = List.of("request", "online", "plan", "missed");
    // Before DEFAULTS: its constructor reads it.
    public static final OverlaySettings DEFAULTS = new OverlaySettings(true, true, true, true, true,
        Duration.NORMAL, Corner.BOTTOM_RIGHT, false, true, 70, true, 85, false, List.of(), List.of(), true, List.of(),
        true, false, List.of());
    /** Stickers the picker keeps at the top (the last sent first). */
    public static final int RECENT_STICKERS = 8;
    /** Emojis the chat's emoji picker keeps at the top (the last written first; CHAT-EMOJI). */
    public static final int RECENT_EMOJIS = 16;
    /**
     * Emoji the panel's picker keeps at the top (OVL-SOC: the last written first; the launcher's 24). Apart from the
     * game chat's own: that picker writes the server's codes and symbols, this one the launcher's emoji.
     */
    public static final int RECENT_CHAT_EMOJIS = 24;

    public OverlaySettings {
        duration = duration == null ? Duration.NORMAL : duration;
        corner = corner == null ? Corner.BOTTOM_RIGHT : corner;
        messageVolume = clampVolume(messageVolume);
        callVolume = clampVolume(callVolume);
        recentStickers = recentStickers == null ? List.of()
            : List.copyOf(recentStickers.subList(0, Math.min(RECENT_STICKERS, recentStickers.size())));
        recentEmojis = recentEmojis == null ? List.of()
            : List.copyOf(recentEmojis.subList(0, Math.min(RECENT_EMOJIS, recentEmojis.size())));
        recentChatEmojis = recentChatEmojis == null ? List.of()
            : List.copyOf(recentChatEmojis.subList(0, Math.min(RECENT_CHAT_EMOJIS, recentChatEmojis.size())));
        cardsOff = cardsOff == null ? List.of() : cardsOff.stream().filter(CARD_GROUPS::contains).distinct().sorted()
            .toList();
    }

    public static int clampVolume(int volume) {
        return Math.max(0, Math.min(MAX_VOLUME, volume));
    }

    /** Whether toasts of this kind ({@code message}, {@code call}, {@code invite}, {@code info}, …) show. */
    public boolean showsKind(String kind) {
        return switch (kind) {
            case "call" -> callToasts;
            // A friend's world invite (HOST1) and a crew invite (OVL-SOC2) are invites as well.
            case "invite", "world", "crew-invite" -> inviteToasts;
            case "info" -> infoToasts;
            // OVL-SOC2: the newer cards, by their group.
            case "request", "friend", "crew-ask", "crew-in" -> !cardsOff.contains("request");
            case "online", "projects" -> !cardsOff.contains("online");
            case "plan" -> !cardsOff.contains("plan");
            case "missed" -> !cardsOff.contains("missed");
            default -> messageToasts;
        };
    }

    /** OVL-SOC2: whether cards of {@code group} ({@code request}, {@code online}, {@code plan}, {@code missed}) show. */
    public boolean showsCards(String group) {
        return !cardsOff.contains(group);
    }

    /**
     * OVL-SOC2: every card kind the player turned off, as the launcher names their groups ({@code message},
     * {@code call}, {@code invite} and the newer ones): the launcher keeps its own notifications for them.
     */
    public List<String> offGroups() {
        List<String> off = new ArrayList<>();
        if (!messageToasts) {
            off.add("message");
        }
        if (!callToasts) {
            off.add("call");
        }
        if (!inviteToasts) {
            off.add("invite");
        }
        off.addAll(cardsOff);
        return off;
    }

    /** OVL-SOC2: cards of {@code group} shown ({@code on}) or not. */
    public OverlaySettings withCards(String group, boolean on) {
        List<String> next = new ArrayList<>(cardsOff);
        next.remove(group);
        if (!on) {
            next.add(group);
        }
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers,
            recentEmojis, showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, next);
    }

    /**
     * The display preferences equal the defaults (what the computer taught the key hint, the recent stickers and the
     * recent emojis do not count).
     */
    public boolean isDefault() {
        return keptFrom(DEFAULTS).equals(DEFAULTS);
    }

    /** The defaults, keeping what is no display preference (what the key hint learnt, the recent stickers, emojis). */
    public OverlaySettings reset() {
        return DEFAULTS.withKeyUsed(keyUsed).withFnNeeded(fnNeeded).withRecentStickers(recentStickers)
            .withRecentEmojis(recentEmojis).withRecentChatEmojis(recentChatEmojis);
    }

    /** These preferences with what is no preference taken from {@code other}. */
    public OverlaySettings keptFrom(OverlaySettings other) {
        return withKeyUsed(other.keyUsed).withFnNeeded(other.fnNeeded).withRecentStickers(other.recentStickers)
            .withRecentEmojis(other.recentEmojis).withRecentChatEmojis(other.recentChatEmojis);
    }

    /** {@code emoji} (what the picker wrote: a code, or the text itself) written now: it comes first in its recent row. */
    public OverlaySettings withEmojiUsed(String emoji) {
        List<String> next = new ArrayList<>(RECENT_EMOJIS);
        next.add(emoji);
        for (String e : recentEmojis) {
            if (!e.equals(emoji) && next.size() < RECENT_EMOJIS) {
                next.add(e);
            }
        }
        return withRecentEmojis(next);
    }

    /** {@code emoji} written from the panel's picker now (OVL-SOC): it comes first in its recent rows. */
    public OverlaySettings withChatEmojiUsed(String emoji) {
        List<String> next = new ArrayList<>(RECENT_CHAT_EMOJIS);
        next.add(emoji);
        for (String e : recentChatEmojis) {
            if (!e.equals(emoji) && next.size() < RECENT_CHAT_EMOJIS) {
                next.add(e);
            }
        }
        return withRecentChatEmojis(next);
    }

    /** {@code sticker} sent now: it comes first in the picker's recent row. */
    public OverlaySettings withStickerUsed(String sticker) {
        List<String> next = new ArrayList<>(RECENT_STICKERS);
        next.add(sticker);
        for (String s : recentStickers) {
            if (!s.equals(sticker) && next.size() < RECENT_STICKERS) {
                next.add(s);
            }
        }
        return withRecentStickers(next);
    }

    public OverlaySettings withMessageToasts(boolean v) {
        return new OverlaySettings(v, callToasts, inviteToasts, infoToasts, showMessageText, duration, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withCallToasts(boolean v) {
        return new OverlaySettings(messageToasts, v, inviteToasts, infoToasts, showMessageText, duration, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withInviteToasts(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, v, infoToasts, showMessageText, duration, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withInfoToasts(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, v, showMessageText, duration, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withShowMessageText(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, v, duration, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withDuration(Duration v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, v, corner,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withCorner(Corner v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration, v,
            reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withReduceMotion(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, v, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withMessageSound(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, v, messageVolume, callSound, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withMessageVolume(int v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, v, callSound, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withCallSound(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, v, callVolume, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withCallVolume(int v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, v, keyUsed, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withRecentStickers(List<String> v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, v, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withRecentEmojis(List<String> v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers, v,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    /** "Konuşanları göster" (VOICE-O). */
    public OverlaySettings withShowSpeakers(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers,
            recentEmojis, v, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withRecentChatEmojis(List<String> v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers,
            recentEmojis, showSpeakers, v, showKeyHint, fnNeeded, cardsOff);
    }

    public OverlaySettings withKeyUsed(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, callVolume, v, recentStickers, recentEmojis,
            showSpeakers, recentChatEmojis, showKeyHint, fnNeeded, cardsOff);
    }

    /** "Açma ipucunu göster" (KEY-HINT). */
    public OverlaySettings withShowKeyHint(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers,
            recentEmojis, showSpeakers, recentChatEmojis, v, fnNeeded, cardsOff);
    }

    /** This computer's F-key reaches the game only with Fn (KEY-HINT: learnt, no preference). */
    public OverlaySettings withFnNeeded(boolean v) {
        return new OverlaySettings(messageToasts, callToasts, inviteToasts, infoToasts, showMessageText, duration,
            corner, reduceMotion, messageSound, messageVolume, callSound, callVolume, keyUsed, recentStickers,
            recentEmojis, showSpeakers, recentChatEmojis, showKeyHint, v, cardsOff);
    }
}
