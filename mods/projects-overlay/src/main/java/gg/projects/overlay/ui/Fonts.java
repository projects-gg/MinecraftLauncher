package gg.projects.overlay.ui;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import java.lang.reflect.Field;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GlyphSource;
import net.minecraft.client.gui.font.glyphs.EffectGlyph;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.resources.Identifier;
import net.minecraft.util.FormattedCharSequence;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The overlay's typefaces, the launcher's own: Figtree for text, Tektur for the panel's title.
 *
 * <p>Each face is a {@link Font} whose default glyphs come from the bundled TrueType definition for the
 * current GUI scale ({@code font/<face>_<scale>.json}): the game draws a TrueType glyph without filtering
 * from a bitmap of "size x oversample" pixels, so text is sharp only when the oversample equals the scale.
 * Because the swap happens inside the font, widths, wrapping, the text field's caret and mouse placement
 * all agree with what is drawn. At GUI scale 1 (the pixel font is the most legible there) and with
 * "Force Unicode Font" on, the game's own font is used. Lines made by {@link Texts} keep their layout between
 * frames ({@link PreparedLine}).
 */
public final class Fonts {
    /** A typeface, with the vertical metrics layouts need (GUI pixels from a line's top). */
    public enum Face {
        UI("ui", 8.5f),
        STRONG("ui_strong", 8.5f),
        SMALL("small", 7.5f),
        DISPLAY("display", 9f),
        /** A crew tag's chip (TEAM6): Tektur as the launcher's {@code TeamTag}, small. */
        TAG("tag", 6f);

        final String id;
        final float size;
        final FontDescription.Resource[] byScale = new FontDescription.Resource[MAX_SCALE + 1];

        Face(String id, float size) {
            this.id = id;
            this.size = size;
            for (int s = 1; s <= MAX_SCALE; s++) {
                byScale[s] = new FontDescription.Resource(Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID,
                    id + "_" + s));
            }
        }
    }

    static final int MAX_SCALE = 8;
    /** Where every font puts the baseline: 7 GUI pixels below the line's top (the game's own rule). */
    public static final float BASELINE = 7f;

    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");
    private static final Font[] FONTS = new Font[Face.values().length];
    private static boolean failed;

    private Fonts() {
    }

    public static Font get(Face face) {
        Font font = FONTS[face.ordinal()];
        if (font == null) {
            font = create(face);
            FONTS[face.ordinal()] = font;
        }
        return font;
    }

    public static Font ui() {
        return get(Face.UI);
    }

    public static Font strong() {
        return get(Face.STRONG);
    }

    public static Font small() {
        return get(Face.SMALL);
    }

    public static Font display() {
        return get(Face.DISPLAY);
    }

    public static Font tag() {
        return get(Face.TAG);
    }

    /** True while the bundled fonts draw (else the game's own font stands in). */
    public static boolean bundled() {
        return !failed && !useGameFont(Minecraft.getInstance());
    }

    /** Cap height in GUI pixels: for centering a line of {@code face} on something. */
    public static float capHeight(Face face) {
        return bundled() ? face.size * 0.7f : 7f;
    }

    /** The y at which to draw {@code face} so its capitals sit centred on {@code centerY}. */
    public static float centeredY(Face face, float centerY) {
        return centerY - BASELINE + capHeight(face) / 2f;
    }

    private static boolean useGameFont(Minecraft mc) {
        return mc.getWindow().getGuiScale() <= 1 || mc.options.forceUnicodeFont().get();
    }

    private static Font create(Face face) {
        Minecraft mc = Minecraft.getInstance();
        Font.Provider base = baseProvider(mc.font);
        if (base == null) {
            return mc.font;
        }
        return new KeepingFont(new Font.Provider() {
            @Override
            public GlyphSource glyphs(FontDescription description) {
                if (description.equals(FontDescription.DEFAULT)) {
                    Minecraft m = Minecraft.getInstance();
                    if (!useGameFont(m)) {
                        int scale = Math.min(MAX_SCALE, Math.max(1, m.getWindow().getGuiScale()));
                        return base.glyphs(face.byScale[scale]);
                    }
                }
                return base.glyphs(description);
            }

            @Override
            public EffectGlyph effect() {
                return base.effect();
            }
        });
    }

    /**
     * An overlay face: the game's own layout and drawing, except that a {@link PreparedLine} is laid out once
     * and then drawn with that layout for as long as nothing it depends on changes.
     */
    private static final class KeepingFont extends Font {
        private final Font.Provider glyphs;

        KeepingFont(Font.Provider glyphs) {
            super(glyphs);
            this.glyphs = glyphs;
        }

        @Override
        public Font.PreparedText prepareText(FormattedCharSequence text, float x, float y, int color, boolean shadow,
            boolean includeEmpty, int background) {
            if (text instanceof PreparedLine line && PreparedLine.keeps(color, background)) {
                // The glyph source changes with the GUI scale, "Force Unicode Font" and every resource reload.
                Object source = glyphs.glyphs(FontDescription.DEFAULT);
                Font.PreparedText kept = line.reuse(this, source, x, y, color, shadow, includeEmpty, background);
                if (kept != null) {
                    Perf.layoutReused();
                    return kept;
                }
                Font.PreparedText made = super.prepareText(text, x, y, color, shadow, includeEmpty, background);
                line.keep(this, source, x, y, color, shadow, includeEmpty, background, made);
                Perf.layoutMade();
                return made;
            }
            Perf.layoutMade();
            return super.prepareText(text, x, y, color, shadow, includeEmpty, background);
        }
    }

    /** The game font's glyph provider (a private field: if a later version renames it, the game font stays). */
    private static Font.Provider baseProvider(Font font) {
        try {
            Field field = Font.class.getDeclaredField("provider");
            field.setAccessible(true);
            return (Font.Provider) field.get(font);
        } catch (ReflectiveOperationException | RuntimeException e) {
            if (!failed) {
                LOG.warn("Bundled fonts are off, the game font draws the overlay: {}", e.toString());
            }
            failed = true;
            return null;
        }
    }
}
