package gg.projects.overlay.settings;

import com.google.gson.GsonBuilder;
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
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.atomic.AtomicReference;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Keeps {@link OverlaySettings} in {@code projects-overlay.json} at the instance root, next to
 * {@code options.txt}: that folder is shared by every Minecraft version of the instance, while
 * {@code config/} is per version. A missing, unreadable or half-written file means defaults (field by
 * field: one bad value does not lose the others). Writes go to a temporary file that replaces the old
 * one in one step, on a background thread, so the render thread never waits on the disk and a crash
 * mid-write leaves the previous file intact.
 */
public final class SettingsStore {
    public static final String FILE_NAME = "projects-overlay.json";
    static final int FORMAT = 1;
    private static final long WRITE_DELAY_MS = 300;

    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");

    private final Path file;
    private final AtomicReference<OverlaySettings> pending = new AtomicReference<>();
    private final ExecutorService writer;
    private volatile OverlaySettings current;

    public SettingsStore(Path directory) {
        this.file = directory.resolve(FILE_NAME);
        this.current = read(file);
        this.writer = Executors.newSingleThreadExecutor(r -> {
            Thread t = new Thread(r, "Projects Overlay settings");
            t.setDaemon(true);
            return t;
        });
    }

    public Path file() {
        return file;
    }

    public OverlaySettings get() {
        return current;
    }

    /** Applies new settings at once and saves them soon (the latest of quick changes wins). */
    public void set(OverlaySettings next) {
        if (next == null || next.equals(current)) {
            return;
        }
        current = next;
        if (pending.getAndSet(next) == null) {
            try {
                writer.execute(this::flushSoon);
            } catch (RejectedExecutionException e) {
                flushPending();
            }
        }
    }

    /** A slider drag sends many values: wait a moment so only the last one is written. */
    private void flushSoon() {
        try {
            Thread.sleep(WRITE_DELAY_MS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        flushPending();
    }

    /** Writes anything still pending on the calling thread (game shutdown). */
    public void close() {
        writer.shutdown();
        flushPending();
    }

    private void flushPending() {
        OverlaySettings next = pending.getAndSet(null);
        if (next != null) {
            try {
                write(file, next);
            } catch (IOException e) {
                LOG.warn("Could not save the overlay settings: {}", e.toString());
            }
        }
    }

    /** Reads settings, falling back to the default of every field that is missing or wrong. */
    public static OverlaySettings read(Path file) {
        String text;
        try {
            text = Files.readString(file, StandardCharsets.UTF_8);
        } catch (NoSuchFileException e) {
            return OverlaySettings.DEFAULTS;
        } catch (IOException e) {
            LOG.warn("Could not read the overlay settings, using defaults: {}", e.toString());
            return OverlaySettings.DEFAULTS;
        }
        return parse(text);
    }

    static OverlaySettings parse(String text) {
        JsonObject root;
        try {
            JsonElement element = JsonParser.parseString(text);
            if (!element.isJsonObject()) {
                return OverlaySettings.DEFAULTS;
            }
            root = element.getAsJsonObject();
        } catch (JsonParseException | IllegalStateException e) {
            LOG.warn("The overlay settings file is damaged, using defaults");
            return OverlaySettings.DEFAULTS;
        }
        OverlaySettings d = OverlaySettings.DEFAULTS;
        JsonObject toasts = object(root, "toasts");
        JsonObject sounds = object(root, "sounds");
        return new OverlaySettings(
            bool(toasts, "message", d.messageToasts()),
            bool(toasts, "call", d.callToasts()),
            bool(toasts, "invite", d.inviteToasts()),
            bool(toasts, "info", d.infoToasts()),
            bool(root, "showMessageText", d.showMessageText()),
            OverlaySettings.Duration.byId(string(root, "duration"), d.duration()),
            OverlaySettings.Corner.byId(string(root, "corner"), d.corner()),
            bool(root, "reduceMotion", d.reduceMotion()),
            bool(sounds, "message", d.messageSound()),
            number(sounds, "messageVolume", d.messageVolume()),
            bool(sounds, "call", d.callSound()),
            number(sounds, "callVolume", d.callVolume()),
            bool(root, "hintSeen", d.hintSeen()));
    }

    static String format(OverlaySettings s) {
        JsonObject root = new JsonObject();
        root.addProperty("version", FORMAT);
        JsonObject toasts = new JsonObject();
        toasts.addProperty("message", s.messageToasts());
        toasts.addProperty("call", s.callToasts());
        toasts.addProperty("invite", s.inviteToasts());
        toasts.addProperty("info", s.infoToasts());
        root.add("toasts", toasts);
        root.addProperty("showMessageText", s.showMessageText());
        root.addProperty("duration", s.duration().id);
        root.addProperty("corner", s.corner().id);
        root.addProperty("reduceMotion", s.reduceMotion());
        JsonObject sounds = new JsonObject();
        sounds.addProperty("message", s.messageSound());
        sounds.addProperty("messageVolume", s.messageVolume());
        sounds.addProperty("call", s.callSound());
        sounds.addProperty("callVolume", s.callVolume());
        root.add("sounds", sounds);
        root.addProperty("hintSeen", s.hintSeen());
        return new GsonBuilder().setPrettyPrinting().create().toJson(root) + "\n";
    }

    static void write(Path file, OverlaySettings settings) throws IOException {
        Path dir = file.toAbsolutePath().getParent();
        if (dir != null) {
            Files.createDirectories(dir);
        }
        Path tmp = file.resolveSibling(file.getFileName() + ".tmp");
        Files.writeString(tmp, format(settings), StandardCharsets.UTF_8);
        try {
            Files.move(tmp, file, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        } catch (AtomicMoveNotSupportedException e) {
            Files.move(tmp, file, StandardCopyOption.REPLACE_EXISTING);
        }
    }

    private static JsonObject object(JsonObject o, String key) {
        JsonElement e = o.get(key);
        return e != null && e.isJsonObject() ? e.getAsJsonObject() : new JsonObject();
    }

    private static boolean bool(JsonObject o, String key, boolean fallback) {
        JsonElement e = o.get(key);
        return e instanceof JsonPrimitive p && p.isBoolean() ? p.getAsBoolean() : fallback;
    }

    private static int number(JsonObject o, String key, int fallback) {
        JsonElement e = o.get(key);
        if (e instanceof JsonPrimitive p && p.isNumber()) {
            double v = p.getAsDouble();
            if (!Double.isNaN(v) && !Double.isInfinite(v)) {
                return (int) Math.round(Math.max(-1e6, Math.min(1e6, v)));
            }
        }
        return fallback;
    }

    private static String string(JsonObject o, String key) {
        JsonElement e = o.get(key);
        return e instanceof JsonPrimitive p && p.isString() ? p.getAsString() : "";
    }
}
