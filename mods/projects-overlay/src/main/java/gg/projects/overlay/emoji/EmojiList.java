package gg.projects.overlay.emoji;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;

/**
 * The server's chat emojis (docs/content-format.md, "Chat emojis"): Studio's list from the launcher's {@code emojis} frame
 * (row E3 of docs/tasks/ovl2-protocol.md), else the list the jar carries (the launcher's built-in one, the same file).
 * They apply only while the game is on one of the {@code hosts} (the item art's rule); elsewhere the picker offers its
 * own unicode set ({@link UnicodeSet}). Entries that break the format's rules are left out, as the launcher does.
 */
public final class EmojiList {
    /** The file the jar carries (copied from crates/pl-content/data/chat-emojis.json at build time). */
    public static final String BUILTIN_RESOURCE = "/assets/projects_overlay/emojis/projects.json";
    public static final int MAX_EMOJIS = 256;
    private static final int MAX_HOSTS = 16;
    private static final int MAX_CODE_CHARS = 32;
    private static final int MAX_ALIASES = 4;
    private static final int MAX_RANK_CHARS = 12;
    private static final int MAX_SERVERS = 6;
    private static final int MAX_SERVER_CHARS = 24;
    private static final int MAX_MEANING_CHARS = 60;

    private static volatile EmojiList builtin;

    private final List<String> hosts;
    private final List<Emoji> emojis;

    public EmojiList(List<String> hosts, List<Emoji> emojis) {
        this.hosts = List.copyOf(hosts);
        this.emojis = List.copyOf(emojis);
    }

    public List<String> hosts() {
        return hosts;
    }

    public List<Emoji> emojis() {
        return emojis;
    }

    /** The list the jar carries: the Projects server's emojis as its scripts had them on 26 Oct 2026. */
    public static EmojiList builtin() {
        EmojiList list = builtin;
        if (list == null) {
            list = load(EmojiList.class.getResourceAsStream(BUILTIN_RESOURCE));
            builtin = list;
        }
        return list;
    }

    /** A list read from a stream of the content format's JSON; an empty list when it cannot be read. */
    static EmojiList load(InputStream in) {
        if (in == null) {
            return new EmojiList(List.of(), List.of());
        }
        try (Reader reader = new InputStreamReader(in, StandardCharsets.UTF_8)) {
            JsonElement root = JsonParser.parseReader(reader);
            return root.isJsonObject() ? read(root.getAsJsonObject()) : new EmojiList(List.of(), List.of());
        } catch (IOException | RuntimeException e) {
            return new EmojiList(List.of(), List.of());
        }
    }

    /** The content document or the launcher's {@code emojis} frame (both carry {@code hosts} and {@code emojis}). */
    public static EmojiList read(JsonObject o) {
        List<String> hosts = new ArrayList<>();
        Set<String> seenHosts = new HashSet<>();
        for (String host : strings(o.get("hosts"))) {
            if (hosts.size() < MAX_HOSTS && validHost(host) && seenHosts.add(host.toLowerCase(Locale.ROOT))) {
                hosts.add(host);
            }
        }
        List<Emoji> emojis = new ArrayList<>();
        Set<String> codes = new HashSet<>();
        JsonElement list = o.get("emojis");
        if (list instanceof JsonArray array) {
            for (JsonElement element : array) {
                if (emojis.size() >= MAX_EMOJIS) {
                    break;
                }
                Emoji emoji = element instanceof JsonObject entry ? entry(entry) : null;
                if (emoji == null) {
                    continue;
                }
                List<String> mine = new ArrayList<>();
                mine.add(emoji.code().toLowerCase(Locale.ROOT));
                for (String alias : emoji.aliases()) {
                    mine.add(alias.toLowerCase(Locale.ROOT));
                }
                if (mine.stream().anyMatch(codes::contains) || new HashSet<>(mine).size() != mine.size()) {
                    continue;
                }
                codes.addAll(mine);
                emojis.add(emoji);
            }
        }
        return new EmojiList(hosts, emojis);
    }

    /** One entry, or null when it breaks a rule. */
    static Emoji entry(JsonObject o) {
        String code = string(o, "code");
        if (!validCode(code)) {
            return null;
        }
        List<String> aliases = strings(o.get("aliases"));
        if (aliases.size() > MAX_ALIASES || !aliases.stream().allMatch(EmojiList::validCode)) {
            return null;
        }
        EmojiKind kind = EmojiKind.read(string(o, "kind"));
        String look = string(o, "look");
        if (look != null && Look.spans(look) == null) {
            return null;
        }
        if (look == null && kind != EmojiKind.SHORTCUT) {
            return null;
        }
        String glyphText = string(o, "glyph");
        int glyph = -1;
        if (glyphText != null) {
            if (kind != EmojiKind.PACK || glyphText.codePointCount(0, glyphText.length()) != 1
                || !Look.isPrivateUse(glyphText.codePointAt(0))) {
                return null;
            }
            glyph = glyphText.codePointAt(0);
        } else if (kind == EmojiKind.PACK) {
            return null;
        }
        String rank = string(o, "rank");
        if (rank != null && !plain(rank, MAX_RANK_CHARS)) {
            return null;
        }
        List<String> servers = strings(o.get("servers"));
        Set<String> seenServers = new HashSet<>();
        if (servers.size() > MAX_SERVERS) {
            return null;
        }
        for (String server : servers) {
            if (!plain(server, MAX_SERVER_CHARS) || !seenServers.add(server.toLowerCase(Locale.ROOT))) {
                return null;
            }
        }
        String tr = null;
        String en = null;
        if (o.get("meaning") instanceof JsonObject meaning) {
            tr = string(meaning, "tr");
            en = string(meaning, "en");
            if (tr == null || en == null || !plain(tr, MAX_MEANING_CHARS) || !plain(en, MAX_MEANING_CHARS)) {
                return null;
            }
        } else if (kind == EmojiKind.SHORTCUT) {
            return null;
        }
        return new Emoji(code, aliases, look, glyph, kind, rank, servers, tr, en);
    }

    /** Whether the list belongs to the server at {@code host} ("play.projects.gg", with or without a port). */
    public boolean appliesTo(String host) {
        return hostMatches(host, hosts);
    }

    /**
     * Whether a server address is one of {@code hosts} or a subdomain of one: the port and a trailing dot are ignored,
     * case does not matter, an IPv4 entry matches only itself (the launcher's {@code item_art_host_matches}).
     */
    public static boolean hostMatches(String host, List<String> hosts) {
        if (host == null) {
            return false;
        }
        String name = hostName(host);
        if (name.isEmpty()) {
            return false;
        }
        for (String entry : hosts) {
            String e = entry.strip();
            while (e.endsWith(".")) {
                e = e.substring(0, e.length() - 1);
            }
            e = e.toLowerCase(Locale.ROOT);
            if (e.isEmpty()) {
                continue;
            }
            if (ipv4(e)) {
                if (name.equals(e)) {
                    return true;
                }
                continue;
            }
            if (name.equals(e) || name.endsWith("." + e)) {
                return true;
            }
        }
        return false;
    }

    private static String hostName(String host) {
        String h = host.strip();
        int colon = h.lastIndexOf(':');
        if (colon > 0 && h.indexOf(':') == colon && h.substring(colon + 1).chars().allMatch(Character::isDigit)) {
            h = h.substring(0, colon);
        }
        while (h.endsWith(".")) {
            h = h.substring(0, h.length() - 1);
        }
        return h.toLowerCase(Locale.ROOT);
    }

    private static boolean ipv4(String text) {
        String[] parts = text.split("\\.", -1);
        if (parts.length != 4) {
            return false;
        }
        for (String part : parts) {
            if (part.isEmpty() || part.length() > 3 || !part.chars().allMatch(Character::isDigit)
                || Integer.parseInt(part) > 255) {
                return false;
            }
        }
        return true;
    }

    /** A host name (labels of letters, digits and hyphens, at least two) or an IPv4 address; no port or scheme. */
    static boolean validHost(String host) {
        if (ipv4(host)) {
            return true;
        }
        if (host.isEmpty() || host.length() > 253) {
            return false;
        }
        String[] labels = host.split("\\.", -1);
        if (labels.length < 2) {
            return false;
        }
        for (String label : labels) {
            if (label.isEmpty() || label.length() > 63 || label.startsWith("-") || label.endsWith("-")) {
                return false;
            }
            for (int i = 0; i < label.length(); i++) {
                char c = label.charAt(i);
                if (!(c < 128 && (Character.isLetterOrDigit(c) || c == '-'))) {
                    return false;
                }
            }
        }
        return labels[labels.length - 1].chars().anyMatch(Character::isLetter);
    }

    /** A code or alias: 1–32 characters, no spaces, control characters, {@code §} or private use characters. */
    static boolean validCode(String code) {
        if (code == null) {
            return false;
        }
        int count = code.codePointCount(0, code.length());
        if (count < 1 || count > MAX_CODE_CHARS) {
            return false;
        }
        return code.codePoints().noneMatch(cp -> Character.isWhitespace(cp) || Character.isSpaceChar(cp)
            || Character.isISOControl(cp) || cp == 0xA7 || Look.isPrivateUse(cp));
    }

    private static boolean plain(String text, int max) {
        int count = text.codePointCount(0, text.length());
        return count >= 1 && count <= max && text.strip().equals(text)
            && text.codePoints().noneMatch(cp -> Character.isISOControl(cp) || cp == 0xA7 || Look.isPrivateUse(cp));
    }

    private static String string(JsonObject o, String key) {
        JsonElement e = o.get(key);
        return e instanceof JsonPrimitive p && p.isString() ? p.getAsString() : null;
    }

    private static List<String> strings(JsonElement e) {
        List<String> out = new ArrayList<>();
        if (e instanceof JsonArray array) {
            for (JsonElement item : array) {
                if (item instanceof JsonPrimitive p && p.isString()) {
                    out.add(p.getAsString());
                } else {
                    return List.of("\u0000");
                }
            }
        }
        return out;
    }
}
