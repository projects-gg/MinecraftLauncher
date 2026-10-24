package gg.projects.overlay.bridge;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.util.List;

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
    /**
     * What this build does beyond the first protocol, listed in {@code hello}; the launcher sends only these:
     * {@code connect} (the launcher's "Bağlan"), {@code chat2} (chat extras: replies, reactions, stickers,
     * "yazıyor…", "Görüldü", links; docs/tasks/ovl2-protocol.md, CHAT2-O), {@code tags} (crew tags and their
     * cards, the crews' "ekibe katıldı" lines; TEAM6-O), {@code link} (codeless Discord linking: the launcher's
     * permit to a Projects server that asks, and its question; SKIN1-O).
     */
    public static final List<String> FEATURES = List.of("connect", "chat2", "tags", "link");
    /** Minecraft's legacy formatting sign; launcher text must never carry formatting into the game. */
    private static final int SECTION_SIGN = 0xA7;
    /** The keycap mark (1️⃣): the digit stays, the mark would draw as a dashed box. */
    private static final int KEYCAP = 0x20E3;
    /** Variation selectors 15 and 16 (text or emoji style), skin tone modifiers: the game's font has no colour. */
    private static final int VS15 = 0xFE0E;
    private static final int VS16 = 0xFE0F;
    private static final int TONE_FIRST = 0x1F3FB;
    private static final int TONE_LAST = 0x1F3FF;

    private Protocol() {
    }

    public static JsonObject hello(String token, String modVersion, String minecraftVersion, String language) {
        JsonObject o = message("hello");
        o.addProperty("proto", VERSION);
        o.addProperty("token", token);
        o.addProperty("mod", modVersion);
        o.addProperty("mc", minecraftVersion);
        o.addProperty("lang", language);
        JsonArray features = new JsonArray();
        for (String feature : FEATURES) {
            features.add(feature);
        }
        features.add(PEOPLE);
        o.add("features", features);
        return o;
    }

    /**
     * The answer to the launcher's {@code connect}: {@code ok} once the game's connect screen is up, else
     * why not ({@code busy}, {@code invalid}); {@code already} when the game is on that server.
     */
    public static JsonObject connectResult(String id, boolean ok, String reason, boolean already) {
        JsonObject o = message("connect-result");
        o.addProperty("id", id);
        o.addProperty("ok", ok);
        if (reason != null) {
            o.addProperty("reason", reason);
        }
        if (already) {
            o.addProperty("already", true);
        }
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
        return send(clientId, conversation, text, 0);
    }

    /** A message, as a reply to the message with service id {@code replySid} when that is not 0. */
    public static JsonObject send(String clientId, String conversation, String text, long replySid) {
        JsonObject o = message("send");
        o.addProperty("cid", clientId);
        o.addProperty("conv", conversation);
        o.addProperty("text", text);
        if (replySid > 0) {
            o.addProperty("reply", replySid);
        }
        return o;
    }

    /** A Projects sticker ({@code pl1/gg}), maybe as a reply. */
    public static JsonObject sendSticker(String clientId, String conversation, String sticker, long replySid) {
        JsonObject o = message("send");
        o.addProperty("cid", clientId);
        o.addProperty("conv", conversation);
        o.addProperty("sticker", sticker);
        if (replySid > 0) {
            o.addProperty("reply", replySid);
        }
        return o;
    }

    /** Gives ({@code on}) or takes back the player's reaction {@code emoji} on message {@code sid}. */
    public static JsonObject react(String conversation, long sid, String emoji, boolean on) {
        JsonObject o = message("react");
        o.addProperty("conv", conversation);
        o.addProperty("sid", sid);
        o.addProperty("e", emoji);
        o.addProperty("on", on);
        return o;
    }

    /** The player writes in the panel ("yazıyor…"). */
    public static JsonObject typing(String conversation) {
        JsonObject o = message("typing");
        o.addProperty("conv", conversation);
        return o;
    }

    public static JsonObject answer(String call, String answer) {
        JsonObject o = message("answer");
        o.addProperty("call", call);
        o.addProperty("answer", answer);
        return o;
    }

    /** The player joins a friend (after the in-game confirmation); the launcher marks their invites answered. */
    public static JsonObject join(String friend) {
        JsonObject o = message("join");
        o.addProperty("friend", friend);
        return o;
    }

    // ---- The friend menu (OVL-UX, rows P1–P11 of docs/tasks/ovl2-protocol.md) ----------------------------

    /**
     * The friend menu's feature in {@code hello} (OVL-UX): the mod hears {@code friend-result}. Sent beside
     * {@link #FEATURES}.
     */
    public static final String PEOPLE = "people";
    /** Report reasons the friend menu offers for a player (the launcher's player reasons). */
    public static final List<String> REPORT_REASONS = List.of("name", "profile", "harassment", "spam", "scam",
        "other");
    /** Longest note a report takes (the launcher's {@code NOTE_MAX}). */
    public static final int MAX_REPORT_NOTE_CHARS = 200;

    /** "Birlikte oyna" to a friend (P3); the answer is {@code friend-result}. */
    public static JsonObject invite(String friend) {
        return friendFrame("invite", friend);
    }

    /** "Discord'dan mesaj at" (P4): the launcher opens the friend's Discord profile. */
    public static JsonObject discord(String friend) {
        return friendFrame("discord", friend);
    }

    /** "Arkadaşlıktan çıkar" (P5), after the mod's question. */
    public static JsonObject unfriend(String friend) {
        return friendFrame("unfriend", friend);
    }

    /** "Engelle" (P6), after the mod's question. */
    public static JsonObject block(String friend) {
        return friendFrame("block", friend);
    }

    /** "Rapor et" (P7): one of {@link #REPORT_REASONS}, a note for moderators (may be empty), maybe a block. */
    public static JsonObject report(String friend, String reason, String note, boolean block) {
        JsonObject o = friendFrame("report", friend);
        o.addProperty("reason", reason);
        String clean = plain(note, MAX_REPORT_NOTE_CHARS);
        if (!clean.isEmpty()) {
            o.addProperty("note", clean);
        }
        if (block) {
            o.addProperty("block", true);
        }
        return o;
    }

    private static JsonObject friendFrame(String type, String friend) {
        JsonObject o = message(type);
        o.addProperty("friend", friend);
        return o;
    }

    /**
     * A Projects server asked whether the launcher vouches for the player (SKIN1-O): its permit for the account
     * the game plays as, please. The answer is {@code link-offer} or {@code link-clear}.
     */
    public static JsonObject linkWant(String name) {
        JsonObject o = message("link-want");
        o.addProperty("name", name);
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

    /** A whole number field, 0 when absent or not a number. */
    public static long id(JsonObject o, String key) {
        return Math.max(0L, num(o, key, 0L));
    }

    /**
     * Turns launcher text into plain text for the game's font: drops the legacy formatting sign, control and
     * format characters (bidi overrides, zero-width marks) and line breaks, and caps the length in code points.
     * Emoji keep their single-colour glyph from the game's font, without what only colour fonts draw: the style
     * selectors (VS15, VS16; "❤️" reads "❤"), skin tone modifiers ("👍🏽" reads "👍") and the keycap mark ("1️⃣"
     * reads "1"). Each of those would otherwise show as a dashed box or a coloured square (CHAT2 §3.2).
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
                || kind == Character.SURROGATE || kind == Character.UNASSIGNED || kind == Character.PRIVATE_USE
                || cp == VS15 || cp == VS16 || cp == KEYCAP || (cp >= TONE_FIRST && cp <= TONE_LAST)) {
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
