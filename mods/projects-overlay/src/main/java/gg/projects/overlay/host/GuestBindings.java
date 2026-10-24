package gg.projects.overlay.host;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.NoSuchFileException;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Consumer;
import java.util.regex.Pattern;

/**
 * Which game name belongs to which friend in one world (PROTOCOL.md §9): {@code <world>/projects-guests.json}. A name
 * is bound to a friend's digest at their first login; another friend can never take it, so nobody gets someone
 * else's inventory or place. The digests are made with this computer's binding key (the launcher's), whose id the
 * file names: a file from another key (a world copied from another computer, launcher data reset) counts as empty.
 * Pure apart from reading and writing the file; not thread-safe (the session guards it).
 */
public final class GuestBindings {
    public static final String FILE = "projects-guests.json";
    /** At most this many names; the one used longest ago goes first. */
    public static final int MAX_ENTRIES = 512;
    private static final Pattern KEY_ID = Pattern.compile("[A-Za-z0-9_-]{8}");
    private static final Pattern DIGEST = Pattern.compile("[A-Za-z0-9_-]{22}");

    /** A name and its friend's digest; {@code last} the last login with it (Unix seconds). */
    public record Entry(String name, String friend, long last) {
    }

    /** A name's state for one friend. */
    public enum Verdict { FREE, OURS, TAKEN }

    private final String keyId;
    /** By folded name, in file order. */
    private final Map<String, Entry> entries = new LinkedHashMap<>();
    /** Names whose {@code last} moved in this session (it moves at most once a session). */
    private final Set<String> touched = new HashSet<>();

    private GuestBindings(String keyId) {
        this.keyId = keyId;
    }

    /** No bindings yet, for key {@code keyId}. */
    public static GuestBindings empty(String keyId) {
        return new GuestBindings(keyId);
    }

    /**
     * Reads the world's file for binding key {@code keyId}. Missing: empty. Written with another key: empty (rewritten
     * on the next binding). Malformed: renamed to {@code projects-guests.json.bad} (replacing an older one) with one
     * line to {@code log}, and empty: only the host's own hand breaks the file, and every guest is the host's friend,
     * so availability wins over keeping old bindings.
     */
    public static GuestBindings load(Path file, String keyId, Consumer<String> log) {
        String json;
        try {
            json = Files.readString(file, StandardCharsets.UTF_8);
        } catch (NoSuchFileException e) {
            return empty(keyId);
        } catch (IOException e) {
            log.accept("could not read the guest name file; starting empty");
            return empty(keyId);
        }
        try {
            return parse(json, keyId);
        } catch (IllegalArgumentException e) {
            try {
                Files.move(file, file.resolveSibling(FILE + ".bad"), StandardCopyOption.REPLACE_EXISTING);
                log.accept("the guest name file was malformed; kept it as " + FILE + ".bad and started empty");
            } catch (IOException moveFailed) {
                log.accept("the guest name file was malformed and could not be moved aside; starting empty");
            }
            return empty(keyId);
        }
    }

    /**
     * Reads a file's text for binding key {@code keyId}: another key's file is empty.
     *
     * @throws IllegalArgumentException when the text breaks the file's rules
     */
    public static GuestBindings parse(String json, String keyId) {
        JsonObject root;
        try {
            JsonElement e = JsonParser.parseString(json);
            if (!e.isJsonObject()) {
                throw new IllegalArgumentException("not an object");
            }
            root = e.getAsJsonObject();
        } catch (JsonParseException | IllegalStateException e) {
            throw new IllegalArgumentException("not JSON", e);
        }
        if (!(root.get("v") instanceof JsonPrimitive v) || !v.isNumber() || v.getAsDouble() != 1d) {
            throw new IllegalArgumentException("unknown version");
        }
        String fileKey = root.get("key") instanceof JsonPrimitive k && k.isString() ? k.getAsString() : null;
        if (fileKey == null || !KEY_ID.matcher(fileKey).matches()
            || !(root.get("names") instanceof JsonArray names)) {
            throw new IllegalArgumentException("no key or names");
        }
        GuestBindings out = new GuestBindings(keyId);
        for (JsonElement e : names) {
            if (!(e instanceof JsonObject entry)) {
                throw new IllegalArgumentException("an entry is not an object");
            }
            String name = entry.get("name") instanceof JsonPrimitive n && n.isString() ? n.getAsString() : null;
            String friend = entry.get("friend") instanceof JsonPrimitive f && f.isString() ? f.getAsString() : null;
            if (!Names.valid(name) || friend == null || !DIGEST.matcher(friend).matches()
                || !(entry.get("last") instanceof JsonPrimitive last) || !last.isNumber()
                || out.entries.containsKey(Names.key(name))) {
                throw new IllegalArgumentException("an entry breaks the rules");
            }
            out.entries.put(Names.key(name), new Entry(name, friend, last.getAsLong()));
        }
        if (out.entries.size() > MAX_ENTRIES) {
            throw new IllegalArgumentException("too many entries");
        }
        if (!fileKey.equals(keyId)) {
            // Made with another computer's (or a reset launcher's) key: its digests mean nothing here.
            out.entries.clear();
        }
        return out;
    }

    /** What {@code name} is for the friend with digest {@code friend}. */
    public Verdict check(String name, String friend) {
        Entry e = entries.get(Names.key(name));
        if (e == null) {
            return Verdict.FREE;
        }
        return e.friend().equals(friend) ? Verdict.OURS : Verdict.TAKEN;
    }

    /**
     * Binds {@code name} to {@code friend} after a login (or moves its {@code last}, once a session). Returns true when
     * something changed and the file should be written. A name bound to someone else stays theirs.
     */
    public boolean bind(String name, String friend, long nowSeconds) {
        String key = Names.key(name);
        Entry e = entries.get(key);
        if (e != null && !e.friend().equals(friend)) {
            return false;
        }
        if (e != null && touched.contains(key)) {
            return false;
        }
        touched.add(key);
        entries.put(key, new Entry(e == null ? name : e.name(), friend, nowSeconds));
        while (entries.size() > MAX_ENTRIES) {
            String oldest = entries.values().stream().min(Comparator.comparingLong(Entry::last))
                .map(x -> Names.key(x.name())).orElseThrow();
            entries.remove(oldest);
        }
        return true;
    }

    public int size() {
        return entries.size();
    }

    public List<Entry> entries() {
        return List.copyOf(entries.values());
    }

    /** The file's text: two-space indentation, LF. */
    public String toJson() {
        JsonObject root = new JsonObject();
        root.addProperty("v", 1);
        root.addProperty("key", keyId);
        JsonArray names = new JsonArray();
        for (Entry e : new ArrayList<>(entries.values())) {
            JsonObject o = new JsonObject();
            o.addProperty("name", e.name());
            o.addProperty("friend", e.friend());
            o.addProperty("last", e.last());
            names.add(o);
        }
        root.add("names", names);
        return new com.google.gson.GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(root)
            + "\n";
    }

    /** Writes the file: a temporary file first, then it replaces the old one. */
    public void save(Path file) throws IOException {
        Path temp = file.resolveSibling(FILE + ".tmp");
        Files.writeString(temp, toJson(), StandardCharsets.UTF_8);
        try {
            Files.move(temp, file, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        } catch (AtomicMoveNotSupportedException e) {
            Files.move(temp, file, StandardCopyOption.REPLACE_EXISTING);
        }
    }
}
