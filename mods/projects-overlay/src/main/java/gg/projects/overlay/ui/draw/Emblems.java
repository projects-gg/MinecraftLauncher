package gg.projects.overlay.ui.draw;

import gg.projects.overlay.ui.CrewLook;
import gg.projects.overlay.ui.Theme;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.gui.GuiGraphicsExtractor;

/**
 * A crew's mark in the overlay (TEAM6, the launcher's {@code CrewMark}): its frame filled with the crew colour under
 * a faint light, an edge in the colour's edge tone, and its emblem (a Phosphor fill icon, {@code icons/emblems.txt})
 * in the middle. The frames and the emblems are baked white into the shapes atlas at the mark's size and tinted when
 * drawn, so no colour needs a texture of its own.
 */
public final class Emblems {
    /** The mark in a tag's bubble, GUI pixels, and its emblem (48 % of it, rounded to an icon size). */
    public static final int MARK = 16;
    public static final int GLYPH = 8;
    /** The frame's edge in the frames' 40-unit box (the launcher's 2.2 at 22 px, thinner as it grows). */
    static final float EDGE_UNITS = 2.0f;
    /** The light over the fill: white this strong at the top, gone at the bottom. */
    static final float LIGHT = 0.14f;
    private static final String RESOURCE = "/assets/projects_overlay/icons/emblems.txt";
    private static final Map<String, String> GLYPH_KEYS = new HashMap<>();
    private static final Map<String, String[]> FRAME_KEYS = new HashMap<>();

    static {
        for (String[] e : CrewLook.EMBLEMS) {
            GLYPH_KEYS.put(e[0], "emblem:" + e[0]);
        }
        for (CrewLook.Shape s : CrewLook.SHAPES) {
            FRAME_KEYS.put(s.key(), new String[] {"crewframe:" + s.key() + ":fill", "crewframe:" + s.key() + ":edge",
                "crewframe:" + s.key() + ":light"});
        }
    }

    private Emblems() {
    }

    /** Every emblem's outline (emblem key to path), read from the mod's resources. */
    public static Map<String, String> loadPaths() throws IOException {
        Map<String, String> paths = new LinkedHashMap<>();
        try (InputStream in = Emblems.class.getResourceAsStream(RESOURCE)) {
            if (in == null) {
                throw new IOException("missing " + RESOURCE);
            }
            BufferedReader reader = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8));
            for (String line; (line = reader.readLine()) != null; ) {
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                String[] parts = line.split("\t", 3);
                if (parts.length == 3 && GLYPH_KEYS.containsKey(parts[0])) {
                    paths.put(parts[0], parts[2]);
                }
            }
        }
        for (String[] e : CrewLook.EMBLEMS) {
            if (!paths.containsKey(e[0])) {
                throw new IOException("no outline for emblem " + e[0]);
            }
        }
        return paths;
    }

    /** The atlas name of an emblem's glyph (an unknown emblem is the shield's). */
    public static String glyphKey(String emblem) {
        return GLYPH_KEYS.get(CrewLook.emblem(emblem));
    }

    /** The atlas names of a frame's fill, edge and light (an unknown frame is the cut's). */
    static String[] frameKeys(String frame) {
        return FRAME_KEYS.get(CrewLook.shape(frame).key());
    }

    /** Bakes the four frames (fill, edge, light) at the mark's size and every emblem at its glyph size. */
    static void bake(AtlasPacker packer, int k, Map<String, String> emblemPaths) {
        int pixels = MARK * k;
        float tolerance = CrewLook.VIEW_BOX / pixels / 20f;
        for (CrewLook.Shape s : CrewLook.SHAPES) {
            List<float[]> contours = SvgPath.parse(s.path(), tolerance).contours();
            String[] keys = FRAME_KEYS.get(s.key());
            ShapeBaker.Image fill = ShapeBaker.icon(contours, CrewLook.VIEW_BOX, MARK, k);
            packer.add(keys[0], fill);
            packer.add(keys[1], ShapeBaker.stroke(contours, CrewLook.VIEW_BOX, MARK, k,
                EDGE_UNITS * pixels / CrewLook.VIEW_BOX));
            packer.add(keys[2], light(fill));
        }
        if (emblemPaths == null) {
            return;
        }
        for (Map.Entry<String, String> e : emblemPaths.entrySet()) {
            packer.add(GLYPH_KEYS.get(e.getKey()), ShapeBaker.icon(Icon.contours(e.getValue(), GLYPH * k),
                Icon.VIEW_BOX, GLYPH, k));
        }
    }

    /** The fill's own shape as a white light, {@link #LIGHT} strong at the top and gone at the bottom. */
    static ShapeBaker.Image light(ShapeBaker.Image fill) {
        int w = fill.width();
        int h = fill.height();
        int[] out = new int[w * h];
        for (int y = 0; y < h; y++) {
            float strength = LIGHT * (1f - (y + 0.5f) / h);
            for (int x = 0; x < w; x++) {
                float a = ((fill.argb()[y * w + x] >>> 24) & 0xFF) / 255f;
                out[y * w + x] = Theme.withAlpha(0xFFFFFF, a * strength);
            }
        }
        return new ShapeBaker.Image(w, h, out);
    }

    /**
     * Draws a mark {@link #MARK} GUI pixels square at {@code x, y}: the frame's fill in the crew colour, its light,
     * its edge and the emblem. Nothing until the atlas is baked.
     */
    public static void draw(GuiGraphicsExtractor g, String color, String frame,
        String emblem, float x, float y) {
        CrewLook.Tone tone = CrewLook.tone(color);
        CrewLook.Shape shape = CrewLook.shape(frame);
        String[] keys = frameKeys(frame);
        Draw.mask(g, keys[0], x, y, MARK, tone.fill());
        Draw.mask(g, keys[2], x, y, MARK, 0xFFFFFFFF);
        Draw.mask(g, keys[1], x, y, MARK, tone.edge());
        float gy = y + shape.glyphY() / CrewLook.VIEW_BOX * MARK - GLYPH / 2f;
        Draw.mask(g, glyphKey(emblem), x + (MARK - GLYPH) / 2f, Draw.snap(gy), GLYPH, tone.glyph());
    }
}
