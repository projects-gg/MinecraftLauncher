package gg.projects.overlay.connect;

import com.google.gson.JsonElement;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;

/**
 * The {@code projects:route} channel's messages (AUTOJOIN, {@link Route}): a string the way Java's
 * {@code DataOutput.writeUTF} writes it (two bytes of length, then modified UTF-8; the lobby's Skript writes it with
 * Guava's {@code ByteStreams}, as it does on {@code BungeeCord}) holding JSON. The lobby sends one,
 * {@code {"t":"ready","v":1}}; the game answers with the command itself. Pure: no Minecraft classes.
 */
public final class RouteMessages {
    /** Longest message taken, in bytes after the length. */
    public static final int MAX_BYTES = 1024;

    private RouteMessages() {
    }

    /** The string in a message's bytes, or "" when they are not one. */
    public static String text(byte[] all) {
        if (all == null || all.length < 2 || all.length > MAX_BYTES + 2) {
            return "";
        }
        int length = ((all[0] & 0xFF) << 8) | (all[1] & 0xFF);
        if (length != all.length - 2) {
            return "";
        }
        try {
            return new DataInputStream(new ByteArrayInputStream(all)).readUTF();
        } catch (IOException e) {
            return "";
        }
    }

    /** A string as the lobby writes it. */
    public static byte[] bytes(String text) {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        try {
            new DataOutputStream(out).writeUTF(text);
        } catch (IOException e) {
            return new byte[] {0, 0};
        }
        return out.toByteArray();
    }

    /** Whether a message is the lobby's {@code ready}; anything else this build does not know is ignored. */
    public static boolean ready(String json) {
        if (json == null || json.isEmpty() || json.length() > MAX_BYTES) {
            return false;
        }
        try {
            JsonElement root = JsonParser.parseString(json);
            return root.isJsonObject() && root.getAsJsonObject().get("t") instanceof JsonPrimitive t && t.isString()
                && "ready".equals(t.getAsString());
        } catch (JsonParseException | IllegalStateException e) {
            return false;
        }
    }
}
