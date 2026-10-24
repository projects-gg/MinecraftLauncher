package gg.projects.overlay.link;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import gg.projects.overlay.bridge.Protocol;
import java.nio.charset.StandardCharsets;
import java.util.regex.Pattern;

/**
 * The {@code projects:link} plugin channel between a Projects server (ProBotLinker) and this mod: one JSON object
 * per message, at most 1 KB (docs/tasks/skin1.md §5.7, skin1-o.md). Pure: no Minecraft classes.
 * <ul>
 * <li>server → game: {@code {"t":"ask","v":1}} (does the launcher vouch for this player?), then, to a game that
 * said it draws the question, {@code {"t":"question","token":…,"displayName":…,"username":…}};</li>
 * <li>game → server: {@code {"t":"intent","nonce":…,"ui":1}}, {@code {"t":"pending"}}, {@code {"t":"none"}}, and
 * the player's answer {@code {"t":"answer","token":…,"answer":"yes"|"no"}}.</li>
 * </ul>
 * The permit ({@code nonce}) and the question's token never go to a log.
 */
public final class LinkMessages {
    /** Longest message either side takes, in UTF-8 bytes (ProBotLinker's limit). */
    public static final int MAX_BYTES = 1024;
    /** Longest Discord name shown, in code points (ProBotLinker cuts them the same way). */
    public static final int MAX_NAME = 32;
    /** The launcher's permit: {@code i} and the base64url of 24 bytes. */
    static final Pattern NONCE = Pattern.compile("^i[A-Za-z0-9_-]{32}$");
    /** ProBotLinker's own token for one question (base64url). */
    static final Pattern TOKEN = Pattern.compile("^[A-Za-z0-9_-]{8,32}$");

    /** What a server said; {@link Other} for anything else. */
    public sealed interface FromServer permits Ask, Question, Other {
    }

    /** "Does the launcher vouch for this player?" */
    public record Ask() implements FromServer {
    }

    /**
     * "Link to this Discord account?", after ProBot checked the permit. The token answers it once.
     *
     * @param displayName the Discord display name, plain (the user name when it has none)
     * @param username    the unique Discord user name, plain
     */
    public record Question(String token, String displayName, String username) implements FromServer {
        /** The token never goes to a log. */
        @Override
        public String toString() {
            return "Question[@" + username + "]";
        }
    }

    public record Other() implements FromServer {
    }

    private static final Other OTHER = new Other();

    private LinkMessages() {
    }

    /** Reads a server's message; {@link Other} for anything this build does not take. */
    public static FromServer read(String json) {
        if (json == null || json.isEmpty() || json.getBytes(StandardCharsets.UTF_8).length > MAX_BYTES) {
            return OTHER;
        }
        JsonObject o;
        try {
            JsonElement root = JsonParser.parseString(json);
            if (!root.isJsonObject()) {
                return OTHER;
            }
            o = root.getAsJsonObject();
        } catch (JsonParseException | IllegalStateException e) {
            return OTHER;
        }
        switch (str(o, "t")) {
            case "ask":
                return new Ask();
            case "question": {
                String token = str(o, "token");
                String username = Protocol.plain(str(o, "username"), MAX_NAME);
                String displayName = Protocol.plain(str(o, "displayName"), MAX_NAME);
                if (!TOKEN.matcher(token).matches() || username.isEmpty()) {
                    return OTHER;
                }
                return new Question(token, displayName.isEmpty() ? username : displayName, username);
            }
            default:
                return OTHER;
        }
    }

    /** The launcher's permit, for a server that asked; {@code ui}: this game draws the question itself. */
    public static String intent(String nonce) {
        JsonObject o = message("intent");
        o.addProperty("nonce", nonce);
        o.addProperty("ui", 1);
        return o.toString();
    }

    /** The launcher is asked for the permit; the answer follows within the server's window. */
    public static String pending() {
        return message("pending").toString();
    }

    /** No permit: the server goes on without the question ({@code /discord eşle} shows its code). */
    public static String none() {
        return message("none").toString();
    }

    /** The player's explicit answer to the question ("Evet, eşle" or "Hayır"). */
    public static String answer(String token, boolean yes) {
        JsonObject o = message("answer");
        o.addProperty("token", token);
        o.addProperty("answer", yes ? "yes" : "no");
        return o.toString();
    }

    /** A permit as the launcher makes them. */
    public static boolean nonce(String nonce) {
        return nonce != null && NONCE.matcher(nonce).matches();
    }

    private static JsonObject message(String type) {
        JsonObject o = new JsonObject();
        o.addProperty("t", type);
        return o;
    }

    private static String str(JsonObject o, String key) {
        JsonElement e = o.get(key);
        return e instanceof JsonPrimitive p && p.isString() ? p.getAsString() : "";
    }
}
