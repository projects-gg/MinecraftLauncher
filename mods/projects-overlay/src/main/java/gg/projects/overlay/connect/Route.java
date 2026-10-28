package gg.projects.overlay.connect;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;

/**
 * The Projects sub-server the player picked beside the launcher's "Doğrudan sunucuya bağlan" (AUTOJOIN). The launcher
 * starts the game with {@code -Dprojects.overlay.route} (percent-encoded {@code {"v":1,"name":…,"command":…}}) only for
 * OYNA to the instance's own Projects server, the command taken from Studio's signed list. While it waits the mod
 * listens on {@code projects:route}: registering that channel is what tells the lobby this player will switch. When the
 * lobby says {@code ready} there (its login, the two-factor check and the agreement all done) the mod sends the command
 * once, as if the player typed it, and the lobby takes it its usual way (its queue, its closed servers).
 *
 * <p>Never twice and never anywhere else: the route ends the moment the command goes, when the game's first place is not
 * Projects, when the proxy moves the game before {@code ready} (the player went somewhere already) and when that
 * connection ends first. A later {@code ready} (back in the lobby) finds no route. Pure: the game comes in through
 * {@link Game}, so the rules have plain tests.
 */
public final class Route {
    /** The JVM property the launcher starts the game with (read once, then cleared). */
    public static final String PROPERTY = "projects.overlay.route";
    /** The launcher's longest command, its {@code /} included (pl-content's rule). */
    public static final int MAX_COMMAND_CHARS = 64;
    /** The launcher's longest sub-server name. */
    public static final int MAX_NAME_CHARS = 24;
    /** The travel toast never stays longer than this after the command went (a full server queues the player). */
    public static final long TRAVEL_MS = 8_000;
    private static final int MAX_RAW_CHARS = 1024;

    /** What the flow needs from the game. */
    public interface Game {
        /** The game is on one of Projects' names (the address the player connected to). */
        boolean onProjects();

        /** Sends {@code command} (without its {@code /}) the way the player typing it would. */
        void sendCommand(String command);

        /** "SkyBlock sunucusuna geçiliyor…" while the panel runs; nothing else otherwise. */
        void showTravel(String name);

        void endTravel();
    }

    private enum Stage {
        /** The game has joined nothing yet. */
        START,
        /** On Projects' first server (the lobby), waiting for its {@code ready}. */
        WAITING,
        /** The command went; the travel toast is up until the proxy moves the game. */
        SENT,
        /** Nothing more: done or dropped. */
        GONE
    }

    private final String name;
    private final String command;
    private Stage stage = Stage.START;
    private long sentAt;

    Route(String name, String command) {
        this.name = name;
        this.command = command;
    }

    /** The launcher's pick, read once and cleared from the system properties; null when there is none. */
    public static Route fromSystemProperty() {
        String raw = System.getProperty(PROPERTY);
        System.clearProperty(PROPERTY);
        return of(raw);
    }

    /** The pick in the launcher's encoding, or null for nothing, a bad encoding or a pick that breaks a rule. */
    public static Route of(String raw) {
        String json = decode(raw);
        if (json == null) {
            return null;
        }
        JsonObject o;
        try {
            JsonElement root = JsonParser.parseString(json);
            if (!root.isJsonObject()) {
                return null;
            }
            o = root.getAsJsonObject();
        } catch (JsonParseException | IllegalStateException e) {
            return null;
        }
        String name = str(o, "name");
        String command = str(o, "command");
        if (!(o.get("v") instanceof JsonPrimitive v) || !v.isNumber() || v.getAsInt() != 1 || !validName(name)
            || !validCommand(command)) {
            return null;
        }
        return new Route(name, command);
    }

    /** The name the player reads ("SkyBlock"). */
    public String name() {
        return name;
    }

    /** The command as the launcher passed it ({@code /aurora}). */
    public String command() {
        return command;
    }

    /** Still waiting for the lobby: the channel stays registered meanwhile. */
    public boolean waiting() {
        return stage == Stage.START || stage == Stage.WAITING;
    }

    /** Nothing left to do: the channel and the route can go. */
    public boolean finished() {
        return stage == Stage.GONE;
    }

    /** The game joined a server: the first one, or the proxy moved it to another. */
    public void joined(Game game) {
        switch (stage) {
            case START -> stage = game.onProjects() ? Stage.WAITING : Stage.GONE;
            // Moved before the lobby's ready: the player went somewhere already.
            case WAITING -> stage = Stage.GONE;
            // The switch happened.
            case SENT -> finish(game);
            case GONE -> {
            }
        }
    }

    /** The connection ended. */
    public void disconnected(Game game) {
        if (stage == Stage.SENT) {
            finish(game);
        } else if (stage == Stage.WAITING) {
            stage = Stage.GONE;
        }
    }

    /** The lobby's {@code ready} on {@code projects:route}: the command goes, once. */
    public void ready(Game game, long now) {
        if (stage != Stage.WAITING || !game.onProjects()) {
            return;
        }
        stage = Stage.SENT;
        sentAt = now;
        game.sendCommand(command.substring(1));
        game.showTravel(name);
    }

    /** Once per client tick: the travel toast runs out. */
    public void tick(Game game, long now) {
        if (stage == Stage.SENT && now - sentAt >= TRAVEL_MS) {
            finish(game);
        }
    }

    private void finish(Game game) {
        stage = Stage.GONE;
        game.endTravel();
    }

    /**
     * A command a player could type (the launcher's rule): {@code /}, a command name (not {@code /} or a space), words
     * separated by single spaces, at most {@link #MAX_COMMAND_CHARS} characters, no control, {@code §} or private use
     * character.
     */
    static boolean validCommand(String command) {
        if (command == null || !command.startsWith("/") || command.codePointCount(0, command.length()) > MAX_COMMAND_CHARS) {
            return false;
        }
        String rest = command.substring(1);
        if (rest.isEmpty() || rest.startsWith("/") || rest.startsWith(" ") || rest.endsWith(" ") || rest.contains("  ")) {
            return false;
        }
        return rest.codePoints()
            .noneMatch(cp -> bad(cp) || ((Character.isWhitespace(cp) || Character.isSpaceChar(cp)) && cp != ' '));
    }

    static boolean validName(String name) {
        return name != null && !name.isEmpty() && name.strip().equals(name)
            && name.codePointCount(0, name.length()) <= MAX_NAME_CHARS && name.codePoints().noneMatch(Route::bad);
    }

    private static boolean bad(int cp) {
        return Character.isISOControl(cp) || cp == 0xA7 || Character.getType(cp) == Character.PRIVATE_USE;
    }

    /** Percent-decoding of the launcher's encoding (UTF-8 bytes, {@code %XX} for all but letters, digits, {@code -._~}). */
    static String decode(String raw) {
        if (raw == null || raw.isEmpty() || raw.length() > MAX_RAW_CHARS * 3) {
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
        try {
            return StandardCharsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes.toByteArray())).toString();
        } catch (CharacterCodingException e) {
            return null;
        }
    }

    private static String str(JsonObject o, String key) {
        JsonElement e = o.get(key);
        return e instanceof JsonPrimitive p && p.isString() ? p.getAsString() : null;
    }
}
