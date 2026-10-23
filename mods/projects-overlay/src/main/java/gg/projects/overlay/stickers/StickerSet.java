package gg.projects.overlay.stickers;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The Projects stickers the mod brings (CHAT2 §8, {@code packages/stickers}): the set's manifest, copied into
 * the jar by the build beside its atlas ({@code assets/projects_overlay/stickers/}). A message carries only a
 * reference ({@code pl1/gg}); the names, the atlas cell and the arrival motion come from here. Pure apart from
 * {@link #load()}, so it is tested on its own.
 */
public final class StickerSet {
    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");
    /** Where the build puts the set's manifest and atlas. */
    public static final String MANIFEST = "/assets/projects_overlay/stickers/pl1.json";

    /** One sticker: its reference, theme, place in the atlas, names and motion pattern. */
    public record Sticker(String ref, String id, String theme, int order, int column, int row, String nameTr,
        String nameEn, String motion) {
        public String name(boolean turkish) {
            return turkish ? nameTr : nameEn;
        }
    }

    /** A theme of the set, in its order. */
    public record Theme(String id, String nameTr, String nameEn) {
        public String name(boolean turkish) {
            return turkish ? nameTr : nameEn;
        }
    }

    private static final StickerSet EMPTY = new StickerSet("", List.of(), List.of(), 0, 0, 0);
    private static volatile StickerSet loaded;

    private final String set;
    private final List<Theme> themes;
    private final List<Sticker> stickers;
    private final Map<String, Sticker> byRef = new HashMap<>();
    private final int tile;
    private final int columns;
    private final int rows;

    private StickerSet(String set, List<Theme> themes, List<Sticker> stickers, int tile, int columns, int rows) {
        this.set = set;
        this.themes = themes;
        this.stickers = stickers;
        this.tile = tile;
        this.columns = columns;
        this.rows = rows;
        for (Sticker s : stickers) {
            byRef.put(s.ref(), s);
        }
    }

    /** The set the mod brings, read once from its jar; an empty set when it cannot be read. */
    public static StickerSet get() {
        StickerSet s = loaded;
        if (s == null) {
            s = load();
            loaded = s;
        }
        return s;
    }

    private static StickerSet load() {
        try (InputStream in = StickerSet.class.getResourceAsStream(MANIFEST)) {
            if (in == null) {
                LOG.warn("The overlay's sticker set is missing; stickers show as a quiet box");
                return EMPTY;
            }
            return parse(new String(in.readAllBytes(), StandardCharsets.UTF_8));
        } catch (IOException | RuntimeException e) {
            LOG.warn("The overlay's sticker set could not be read: {}", e.toString());
            return EMPTY;
        }
    }

    /** Reads a manifest ({@code packages/stickers/pl1/manifest.json}); throws when it is not one. */
    public static StickerSet parse(String json) {
        JsonObject root = JsonParser.parseString(json).getAsJsonObject();
        String set = root.get("set").getAsString();
        JsonObject overlay = root.getAsJsonObject("formats").getAsJsonObject("overlay");
        int tile = overlay.get("tile").getAsInt();
        int columns = overlay.get("columns").getAsInt();
        int rows = overlay.get("rows").getAsInt();
        List<Theme> themes = new ArrayList<>();
        for (JsonElement e : root.getAsJsonArray("themes")) {
            JsonObject t = e.getAsJsonObject();
            themes.add(new Theme(t.get("id").getAsString(), t.get("tr").getAsString(), t.get("en").getAsString()));
        }
        List<Sticker> stickers = new ArrayList<>();
        JsonArray list = root.getAsJsonArray("stickers");
        for (JsonElement e : list) {
            JsonObject s = e.getAsJsonObject();
            JsonObject name = s.getAsJsonObject("name");
            JsonObject atlas = s.getAsJsonObject("atlas");
            int column = atlas.get("column").getAsInt();
            int row = atlas.get("row").getAsInt();
            if (column < 0 || column >= columns || row < 0 || row >= rows) {
                throw new IllegalArgumentException("sticker outside the atlas: " + s.get("ref"));
            }
            JsonElement motion = s.get("motion");
            stickers.add(new Sticker(s.get("ref").getAsString(), s.get("id").getAsString(),
                s.get("theme").getAsString(), s.get("order").getAsInt(), column, row, name.get("tr").getAsString(),
                name.get("en").getAsString(), motion == null || motion.isJsonNull() ? "" : motion.getAsString()));
        }
        List<String> order = themes.stream().map(Theme::id).toList();
        stickers.sort(Comparator.comparingInt((Sticker s) -> {
            int at = order.indexOf(s.theme());
            return at < 0 ? Integer.MAX_VALUE : at;
        }).thenComparingInt(Sticker::order));
        return new StickerSet(set, List.copyOf(themes), List.copyOf(stickers), tile, columns, rows);
    }

    public String set() {
        return set;
    }

    public boolean isEmpty() {
        return stickers.isEmpty();
    }

    /** Every sticker, theme by theme, each theme in its order (the picker's grid). */
    public List<Sticker> all() {
        return stickers;
    }

    public List<Theme> themes() {
        return themes;
    }

    /** A sticker of this set by its reference, or null (another set, or one this build does not have). */
    public Sticker find(String ref) {
        return ref == null ? null : byRef.get(ref);
    }

    /** The atlas's tile size in pixels, and its columns and rows. */
    public int tile() {
        return tile;
    }

    public int columns() {
        return columns;
    }

    public int rows() {
        return rows;
    }
}
