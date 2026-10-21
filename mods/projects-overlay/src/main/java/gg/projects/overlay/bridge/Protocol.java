package gg.projects.overlay.bridge;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;

/**
 * Wire format shared with Projects Launcher: one UTF-8 JSON object per line, the {@code t} field names the
 * message. Unknown messages and unknown fields are ignored on both sides, so additions never need a new
 * version; only breaking changes bump {@link #VERSION}.
 */
public final class Protocol {
    /** Protocol version this build speaks. */
    public static final int VERSION = 1;
    /**
     * Longest launcher frame the mod accepts, in bytes without the newline (a snapshot with many friends or a
     * conversation history). Longer frames end the connection.
     */
    public static final int MAX_INCOMING_FRAME_BYTES = 256 * 1024;
    /** Longest frame the mod sends; its frames are small player actions. The launcher enforces the same. */
    public static final int MAX_OUTGOING_FRAME_BYTES = 16 * 1024;
    /** Longest chat message a player can send; the social service enforces the same rule. */
    public static final int MAX_MESSAGE_CHARS = 300;
    /** Longest name or title rendered from launcher data. */
    public static final int MAX_NAME_CHARS = 48;
    /** Minecraft's legacy formatting sign; launcher text must never carry formatting into the game. */
    private static final int SECTION_SIGN = 0xA7;

    private Protocol() {
    }

    public static JsonObject hello(String token, String modVersion, String minecraftVersion, String language) {
        JsonObject o = message("hello");
        o.addProperty("proto", VERSION);
        o.addProperty("token", token);
        o.addProperty("mod", modVersion);
        o.addProperty("mc", minecraftVersion);
        o.addProperty("lang", language);
        return o;
    }

    public static JsonObject panel(boolean open) {
        JsonObject o = message("panel");
        o.addProperty("open", open);
        return o;
    }

    public static JsonObject openConversation(String conversation) {
        JsonObject o = message("open-conv");
        o.addProperty("conv", conversation);
        return o;
    }

    public static JsonObject send(String clientId, String conversation, String text) {
        JsonObject o = message("send");
        o.addProperty("cid", clientId);
        o.addProperty("conv", conversation);
        o.addProperty("text", text);
        return o;
    }

    public static JsonObject answer(String call, String answer) {
        JsonObject o = message("answer");
        o.addProperty("call", call);
        o.addProperty("answer", answer);
        return o;
    }

    public static JsonObject message(String type) {
        JsonObject o = new JsonObject();
        o.addProperty("t", type);
        return o;
    }

    /** Parses one frame. Returns null for anything that is not a JSON object with a string {@code t}. */
    public static JsonObject parse(String line) {
        if (line == null || line.isEmpty()) {
            return null;
        }
        try {
            JsonElement element = JsonParser.parseString(line);
            if (!element.isJsonObject()) {
                return null;
            }
            JsonObject o = element.getAsJsonObject();
            return type(o).isEmpty() ? null : o;
        } catch (JsonParseException | IllegalStateException e) {
            return null;
        }
    }

    public static String type(JsonObject o) {
        return str(o, "t", "");
    }

    public static String str(JsonObject o, String key, String fallback) {
        JsonElement e = o.get(key);
        if (e instanceof JsonPrimitive p && p.isString()) {
            return p.getAsString();
        }
        return fallback;
    }

    public static long num(JsonObject o, String key, long fallback) {
        JsonElement e = o.get(key);
        if (e instanceof JsonPrimitive p && p.isNumber()) {
            try {
                return p.getAsLong();
            } catch (NumberFormatException ex) {
                return fallback;
            }
        }
        return fallback;
    }

    public static boolean bool(JsonObject o, String key, boolean fallback) {
        JsonElement e = o.get(key);
        if (e instanceof JsonPrimitive p && p.isBoolean()) {
            return p.getAsBoolean();
        }
        return fallback;
    }

    /**
     * Turns launcher text into plain text for the game's font: drops the legacy formatting sign, control and
     * format characters (bidi overrides, zero-width marks) and line breaks, and caps the length in code points.
     */
    public static String plain(String s, int maxCodePoints) {
        if (s == null || s.isEmpty() || maxCodePoints <= 0) {
            return "";
        }
        StringBuilder b = new StringBuilder(Math.min(s.length(), maxCodePoints * 2));
        int kept = 0;
        boolean pendingSpace = false;
        for (int i = 0; i < s.length() && kept < maxCodePoints; ) {
            int cp = s.codePointAt(i);
            i += Character.charCount(cp);
            if (cp == '\n' || cp == '\r' || cp == '\t') {
                pendingSpace = b.length() > 0;
                continue;
            }
            int kind = Character.getType(cp);
            if (cp == SECTION_SIGN || kind == Character.CONTROL || kind == Character.FORMAT
                || kind == Character.SURROGATE || kind == Character.UNASSIGNED || kind == Character.PRIVATE_USE) {
                continue;
            }
            if (pendingSpace) {
                b.append(' ');
                kept++;
                pendingSpace = false;
                if (kept >= maxCodePoints) {
                    break;
                }
            }
            b.appendCodePoint(cp);
            kept++;
        }
        return b.toString().strip();
    }
}
