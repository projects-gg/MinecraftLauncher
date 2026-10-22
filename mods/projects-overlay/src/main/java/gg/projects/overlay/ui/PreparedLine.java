package gg.projects.overlay.ui;

import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.font.TextRenderable;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.util.FormattedCharSink;

/**
 * A line of text that keeps the game's layout of it (every glyph looked up and placed) from one frame to the
 * next. The game lays a line out again each time it is drawn, so a toast that only sits or glides on screen
 * paid for all of its glyphs on every frame; the overlay's fonts ({@link Fonts}) ask the line for its layout
 * first. What reaches the screen is the game's own layout, drawn by the game: nothing changes but how often
 * it is made.
 *
 * <p>The layout is made again when anything it depends on changes: the font, the glyph source (GUI scale,
 * "Force Unicode Font", a resource reload), the colour or the position, or when the game has released a glyph
 * texture the layout points into (a font reload in place). Only opaque text is kept: a fading line changes
 * colour on every frame, so it is laid out as before. Moving a line is free: its position lives in the
 * drawing transform, not in the layout. Render thread only.
 */
public final class PreparedLine implements FormattedCharSequence {
    /** Lets the measurement driver compare with the old way (every line laid out on every frame). */
    public static volatile boolean keepLayouts = true;

    private static final TextRenderable[] NONE = new TextRenderable[0];

    private final FormattedCharSequence text;
    private Font font;
    private Object source;
    private float x;
    private float y;
    private int color;
    private int background;
    private boolean shadow;
    private boolean includeEmpty;
    private Font.PreparedText layout;
    /** One glyph per texture the layout draws from: if one of those textures is gone, so is the layout. */
    private TextRenderable[] pages = NONE;

    private PreparedLine(FormattedCharSequence text) {
        this.text = text;
    }

    /** {@code text} as a line that keeps its layout (the empty line stays as it is: it has nothing to lay out). */
    public static FormattedCharSequence of(FormattedCharSequence text) {
        if (text == null || text == FormattedCharSequence.EMPTY || text instanceof PreparedLine) {
            return text;
        }
        return new PreparedLine(text);
    }

    @Override
    public boolean accept(FormattedCharSink sink) {
        return text.accept(sink);
    }

    /** Whether a layout for these inputs may be kept at all. */
    static boolean keeps(int color, int background) {
        return keepLayouts && (color >>> 24) == 0xFF && background == 0;
    }

    /** The kept layout, if it was made for exactly these inputs and its glyph textures are still alive. */
    Font.PreparedText reuse(Font font, Object source, float x, float y, int color, boolean shadow,
        boolean includeEmpty, int background) {
        Font.PreparedText kept = layout;
        if (kept == null || font != this.font || source != this.source || x != this.x || y != this.y
            || color != this.color || shadow != this.shadow || includeEmpty != this.includeEmpty
            || background != this.background) {
            return null;
        }
        for (TextRenderable glyph : pages) {
            var view = glyph.textureView();
            if (view == null || view.isClosed() || view.texture().isClosed()) {
                drop();
                return null;
            }
        }
        return kept;
    }

    /** Keeps a layout the font just made for these inputs. */
    void keep(Font font, Object source, float x, float y, int color, boolean shadow, boolean includeEmpty,
        int background, Font.PreparedText made) {
        this.font = font;
        this.source = source;
        this.x = x;
        this.y = y;
        this.color = color;
        this.shadow = shadow;
        this.includeEmpty = includeEmpty;
        this.background = background;
        this.layout = made;
        this.pages = pagesOf(made);
    }

    /** Forgets the layout (its glyph textures went away). */
    void drop() {
        layout = null;
        pages = NONE;
        font = null;
        source = null;
    }

    private static TextRenderable[] pagesOf(Font.PreparedText made) {
        List<TextRenderable> found = new ArrayList<>(2);
        made.visit(new Font.GlyphVisitor() {
            @Override
            public void acceptGlyph(TextRenderable.Styled glyph) {
                var view = glyph.textureView();
                if (view == null) {
                    return;
                }
                for (TextRenderable seen : found) {
                    if (seen.textureView() == view) {
                        return;
                    }
                }
                found.add(glyph);
            }
        });
        return found.isEmpty() ? NONE : found.toArray(NONE);
    }
}
