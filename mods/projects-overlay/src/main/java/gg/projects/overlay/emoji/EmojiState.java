package gg.projects.overlay.emoji;

import com.google.gson.JsonObject;
import gg.projects.overlay.bridge.Protocol;

/**
 * The chat's emoji picker as the launcher says (CHAT-EMOJI): whether it is on ("Sohbette emoji seçici", the launcher's switch
 * shown in two places) and the Projects server's emojis. The game starts with the switch the launcher's JVM argument names
 * ({@code -Dprojects.overlay.emoji=off}, absent: on) and the list the jar carries; the launcher's {@code emojis} frame (E3)
 * brings both as they are now. Render thread only.
 */
public final class EmojiState {
    /** The JVM argument's system property: {@code off} while the switch is off. */
    public static final String SWITCH_PROPERTY = "projects.overlay.emoji";

    private boolean on;
    private boolean heard;
    private EmojiList projects = EmojiList.builtin();
    private int version;

    public EmojiState(boolean on) {
        this.on = on;
    }

    /** The switch as the game starts: on unless the launcher said {@code off}. */
    public static boolean onAtStart(String property) {
        return property == null || !property.strip().equalsIgnoreCase("off");
    }

    public boolean on() {
        return on;
    }

    /** The launcher has spoken this game: it takes the player's switch from the overlay's settings (E4). */
    public boolean heard() {
        return heard;
    }

    /** The Projects server's emojis now. */
    public EmojiList projects() {
        return projects;
    }

    /** Grows with every change, so the picker knows when to lay itself out again. */
    public int version() {
        return version;
    }

    /** The launcher's {@code emojis} frame. */
    public void apply(JsonObject frame) {
        on = Protocol.bool(frame, "on", on);
        heard = true;
        projects = EmojiList.read(frame);
        version++;
    }

    /** The player switched the picker in the overlay's settings: it holds at once (the launcher answers the same). */
    public void switchTo(boolean value) {
        if (on != value) {
            on = value;
            version++;
        }
    }

    /** Both switches went off during the game (the launcher said {@code bye{disabled}} to a picker-only game). */
    public void endedByLauncher() {
        switchTo(false);
    }
}
