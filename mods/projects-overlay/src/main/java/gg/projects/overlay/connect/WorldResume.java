package gg.projects.overlay.connect;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;

/**
 * The launcher's "Devam et" into a singleplayer world (OVL-FIX). The launcher names the world's folder in
 * {@code -Dprojects.overlay.world} (percent-encoded UTF-8) instead of passing the game's quick play, and the overlay
 * opens it the way the game's quick play does, but only once the title screen is up and the game has drawn a frame
 * after it.
 *
 * <p>Why not quick play: a mod that finishes the game's loading early (Remove Reloading Screen) makes quick play open the
 * world inside the game's start, before its first frame; mods that set up on the first frame (XaeroLib, in Xaero's
 * Minimap, World Map and Better PvP) then crash as the world's server starts. Here the title screen has been set up and
 * a frame has gone by (the game's own order: the title screen comes under the fading loading screen, and the world
 * opens right after, so nothing shows that quick play would not have shown).
 *
 * <p>The frame count comes from {@code MinecraftFramesMixin} ({@link #frameStarted}); should that not apply, the title
 * screen's own first drawn frame (Fabric's screen event) stands in, a moment later. The world opens once, on the title
 * screen and on its way nowhere. The player going to another world or a server first, or the title screen not being up
 * within {@link #WAIT_MS} of its first set-up (a mod's question in between that the player stays on), drops it. A world
 * that is gone gets a short note once the title screen shows itself, and the title screen stays. The game side sits
 * behind {@link Game}, so the flow has plain tests.
 */
public final class WorldResume {
    /** The JVM property the launcher starts the game with. */
    public static final String PROPERTY = "projects.overlay.world";
    /** After the title screen's first set-up, how long the world may still wait for the title screen to be current. */
    public static final long WAIT_MS = 120_000;
    /** A world folder name longer than this is not one the launcher sends. */
    private static final int MAX_CHARS = 255;

    /** Frames the game began, counted on the render thread. */
    private static long frames;

    /** What the flow needs from the game. */
    public interface Game {
        /** The title screen is the current screen. */
        boolean onTitleScreen();

        /** A world or server is open, loading or being joined: the player went somewhere already. */
        boolean elsewhere();

        boolean exists(String world);

        /** Opens the world as the game's own quick play does (its confirmations included). */
        void open(String world);

        /** "<world> adlı dünya bulunamadı." */
        void missing(String world);
    }

    private String world;
    /** The frame count and the time when a title screen was first set up; -1: not yet. */
    private long titleFrames = -1;
    private long titleAt = -1;
    /** The title screen drew a frame of its own (the stand-in when the frame count is not kept). */
    private boolean titleDrawn;

    private WorldResume(String world) {
        this.world = world;
    }

    /** {@code Minecraft.renderFrame} began (the mixin; render thread). */
    public static void frameStarted() {
        frames++;
    }

    /** Frames the game began so far. */
    public static long frames() {
        return frames;
    }

    /** The launcher's word, read once and cleared from the system properties; null when there is none. */
    public static WorldResume fromSystemProperty() {
        String raw = System.getProperty(PROPERTY);
        System.clearProperty(PROPERTY);
        return of(raw);
    }

    /** A resume for the encoded folder name {@code raw}, or null when it is missing or not a usable name. */
    public static WorldResume of(String raw) {
        String world = decode(raw);
        return world == null ? null : new WorldResume(world);
    }

    /**
     * Percent-decoding of the launcher's encoding (UTF-8 bytes, {@code %XX} for everything but letters, digits and
     * {@code -._~}): null for nothing, a bad escape, bytes that are not UTF-8, a name with path separators or control
     * characters, "." and "..", or one longer than {@link #MAX_CHARS}.
     */
    static String decode(String raw) {
        if (raw == null || raw.isEmpty() || raw.length() > MAX_CHARS * 9) {
            return null;
        }
        ByteArrayOutputStream bytes = new ByteArrayOutputStream(raw.length());
        for (int i = 0; i < raw.length(); i++) {
            char c = raw.charAt(i);
            if (c == '%') {
                if (i + 2 >= raw.length()) {
                    return null;
                }
                int hi = Character.digit(raw.charAt(i + 1), 16);
                int lo = Character.digit(raw.charAt(i + 2), 16);
                if (hi < 0 || lo < 0) {
                    return null;
                }
                bytes.write(hi * 16 + lo);
                i += 2;
            } else if (c < 0x80) {
                bytes.write(c);
            } else {
                return null;
            }
        }
        byte[] data = bytes.toByteArray();
        String text = new String(data, StandardCharsets.UTF_8);
        if (!java.util.Arrays.equals(text.getBytes(StandardCharsets.UTF_8), data)) {
            return null;
        }
        if (text.isBlank() || text.length() > MAX_CHARS || text.equals(".") || text.equals("..")) {
            return null;
        }
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            if (c < 0x20 || c == 0x7f || c == '/' || c == '\\') {
                return null;
            }
        }
        return text;
    }

    /** The world still waiting to open, or null once it opened or was dropped. */
    public String pending() {
        return world;
    }

    /** A title screen was set up ({@code frames} frames into the game, at {@code now}); the first one counts. */
    public void titleScreen(long frames, long now) {
        if (world != null && titleAt < 0) {
            titleFrames = frames;
            titleAt = now;
        }
    }

    /** A title screen drew a frame of its own. */
    public void titleFrame() {
        if (world != null && titleAt >= 0) {
            titleDrawn = true;
        }
    }

    /** Each client tick, {@code frames} frames into the game. */
    public void tick(Game game, long frames, long now) {
        if (world == null) {
            return;
        }
        if (game.elsewhere()) {
            world = null;
            return;
        }
        if (titleAt < 0) {
            return;
        }
        if (!game.onTitleScreen()) {
            if (now - titleAt > WAIT_MS) {
                world = null;
            }
            return;
        }
        // A frame went by after the title screen came: every mod has had its first frame.
        if (frames <= titleFrames && !titleDrawn) {
            return;
        }
        String name = world;
        if (!game.exists(name)) {
            // Gone: said once the title screen shows itself (under the fading loading screen the note would go unseen).
            if (titleDrawn) {
                world = null;
                game.missing(name);
            }
            return;
        }
        world = null;
        game.open(name);
    }
}
