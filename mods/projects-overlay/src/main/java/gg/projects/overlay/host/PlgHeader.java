package gg.projects.overlay.host;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.security.MessageDigest;
import java.util.regex.Pattern;
import javax.crypto.Mac;
import javax.crypto.spec.SecretKeySpec;

/**
 * The identity header the host launcher writes first on every local connection to the shared world (PROTOCOL.md §7):
 * {@code "PLG1" || u16 length || JSON || HMAC-SHA256(gate key, all bytes before)}. The gate key is HMAC-SHA256 of the
 * phrase {@code projects host gate v1} keyed with the bridge token's ASCII bytes: both sides hold the token already, no
 * new secret travels. Pure, so the rules have plain tests against the shared vectors ({@code testdata/gate}).
 */
public final class PlgHeader {
    /** The magic, then a big-endian u16 with the JSON's length. */
    public static final int PREFIX = 6;
    public static final int MAC_BYTES = 32;
    /** Longest JSON a header may carry. */
    public static final int MAX_JSON = 1024;
    /** The whole header can never be longer than this. */
    public static final int MAX_BYTES = PREFIX + MAX_JSON + MAC_BYTES;
    private static final byte[] MAGIC = {'P', 'L', 'G', '1'};
    private static final byte[] GATE_PHRASE = "projects host gate v1".getBytes(StandardCharsets.US_ASCII);
    private static final Pattern PUBLISH_ID = Pattern.compile("[A-Za-z0-9_-]{1,32}");
    private static final Pattern GUEST_ID = Pattern.compile("g[1-9][0-9]{0,2}");
    private static final Pattern DIGEST = Pattern.compile("[A-Za-z0-9_-]{22}");

    private PlgHeader() {
    }

    /** The gate key for a bridge token (PROTOCOL.md §7.1). */
    public static byte[] gateKey(String bridgeToken) {
        return hmac(bridgeToken.getBytes(StandardCharsets.US_ASCII), GATE_PHRASE, GATE_PHRASE.length);
    }

    /**
     * How long the whole header is, once its first {@link #PREFIX} bytes are known; -1 when they cannot start a header
     * (another magic, an empty or too long JSON). The gate closes such a connection without a word.
     */
    public static int length(byte[] prefix) {
        if (prefix == null || prefix.length < PREFIX) {
            return -1;
        }
        for (int i = 0; i < MAGIC.length; i++) {
            if (prefix[i] != MAGIC[i]) {
                return -1;
            }
        }
        int json = ((prefix[4] & 0xFF) << 8) | (prefix[5] & 0xFF);
        return json < 2 || json > MAX_JSON ? -1 : PREFIX + json + MAC_BYTES;
    }

    /**
     * Checks a whole header: its length, its MAC in constant time, then the JSON's fields. Returns who it vouches for,
     * or null for anything else. Whether the publish id is the current one and the connection number unused is the
     * session's to decide ({@link HostSession#admitHeader}).
     */
    public static GuestIdentity verify(byte[] gateKey, byte[] header) {
        int total = length(header);
        if (total < 0 || header.length != total) {
            return null;
        }
        int signed = total - MAC_BYTES;
        byte[] expected = hmac(gateKey, header, signed);
        byte[] given = new byte[MAC_BYTES];
        System.arraycopy(header, signed, given, 0, MAC_BYTES);
        if (!MessageDigest.isEqual(expected, given)) {
            return null;
        }
        return fields(new String(header, PREFIX, signed - PREFIX, StandardCharsets.UTF_8));
    }

    /** A header as the launcher writes it (the tests and the development launcher make one with this). */
    public static byte[] build(byte[] gateKey, String json) {
        byte[] body = json.getBytes(StandardCharsets.UTF_8);
        if (body.length < 2 || body.length > MAX_JSON) {
            throw new IllegalArgumentException("a header's JSON is 2 to 1024 bytes");
        }
        byte[] out = new byte[PREFIX + body.length + MAC_BYTES];
        System.arraycopy(MAGIC, 0, out, 0, MAGIC.length);
        out[4] = (byte) (body.length >>> 8);
        out[5] = (byte) body.length;
        System.arraycopy(body, 0, out, PREFIX, body.length);
        byte[] mac = hmac(gateKey, out, PREFIX + body.length);
        System.arraycopy(mac, 0, out, PREFIX + body.length, MAC_BYTES);
        return out;
    }

    /** The JSON's fields, strictly typed (v 1; the ids in their forms); null when one is missing or wrong. */
    static GuestIdentity fields(String json) {
        JsonObject o;
        try {
            JsonElement e = JsonParser.parseString(json);
            if (!e.isJsonObject()) {
                return null;
            }
            o = e.getAsJsonObject();
        } catch (JsonParseException | IllegalStateException e) {
            return null;
        }
        Long v = integer(o, "v");
        Long conn = integer(o, "conn");
        String id = string(o, "id");
        String guest = string(o, "guest");
        String name = string(o, "name");
        String friend = string(o, "friend");
        JsonElement premium = o.get("premium");
        if (v == null || v != 1 || conn == null || conn <= 0 || id == null || !PUBLISH_ID.matcher(id).matches()
            || guest == null || !GUEST_ID.matcher(guest).matches() || name == null || name.isEmpty()
            || name.length() > 64 || friend == null || !DIGEST.matcher(friend).matches()
            || !(premium instanceof JsonPrimitive p && p.isBoolean())) {
            return null;
        }
        return new GuestIdentity(id, conn, guest, name, premium.getAsBoolean(), friend);
    }

    private static String string(JsonObject o, String key) {
        return o.get(key) instanceof JsonPrimitive p && p.isString() ? p.getAsString() : null;
    }

    private static Long integer(JsonObject o, String key) {
        if (!(o.get(key) instanceof JsonPrimitive p) || !p.isNumber()) {
            return null;
        }
        try {
            double d = p.getAsDouble();
            long l = p.getAsLong();
            return d == l ? l : null;
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private static byte[] hmac(byte[] key, byte[] data, int length) {
        try {
            Mac mac = Mac.getInstance("HmacSHA256");
            mac.init(new SecretKeySpec(key, "HmacSHA256"));
            mac.update(data, 0, length);
            return mac.doFinal();
        } catch (GeneralSecurityException e) {
            // Every Java has HmacSHA256.
            throw new IllegalStateException(e);
        }
    }
}
