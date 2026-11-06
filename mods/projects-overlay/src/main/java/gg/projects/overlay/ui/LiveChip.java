package gg.projects.overlay.ui;

import gg.projects.overlay.lang.Language;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.util.FormattedCharSequence;

/**
 * "YAYINDA" / "LIVE" right after the name of someone who shares their screen (SHARE-LIVE; the launcher's live tag): a
 * crew tag's chip (its height, its chamfer, its Figtree SemiBold) in jade, the real state's colour: jade at 18 % under
 * jade words. Never a light or a pulse. It never shrinks; the name before it does. Laid out once per language.
 */
public final class LiveChip {
    /** Between the name and the chip. */
    public static final float GAP = 3f;
    private static final float PAD_X = 2.5f;
    private static final int TINT = Theme.withAlpha(Theme.JADE_400, 0.18f);

    private Language laidOutIn;
    private boolean bundledFont;
    private FormattedCharSequence text;
    private float width;
    private int height;
    private Frames frame = Frames.TAG_9;

    private void layout() {
        Language language = Texts.current();
        boolean bundled = Fonts.bundled();
        if (text != null && language == laidOutIn && bundled == bundledFont) {
            return;
        }
        laidOutIn = language;
        bundledFont = bundled;
        String words = Texts.str("share.tag");
        text = Texts.visual(words);
        // As a crew tag's chip: the game's own taller font grows it, with room for the accents above and below.
        height = Math.max(TagChip.ROW, Math.round(Fonts.capHeight(Fonts.Face.TAG) + (bundled ? 4f : 6f)));
        width = Fonts.tag().width(words) + 2 * PAD_X;
        frame = height == TagChip.ROW ? Frames.TAG_9 : Frames.TAG;
    }

    /** How wide it is (in the current language). */
    public float width() {
        layout();
        return width;
    }

    /** How far a name gives way for it ({@code 0} while nobody shares). */
    public float room(boolean live) {
        return live ? width() + GAP : 0f;
    }

    /** Draws it with its left edge at {@code x}, centred on {@code centerY}. */
    public void draw(GuiGraphicsExtractor g, float x, float centerY) {
        layout();
        float y = Draw.snap(centerY - height / 2f);
        Draw.frame(g, frame, Draw.snap(x), y, width, height, TINT);
        Draw.text(g, Fonts.tag(), text, Draw.snap(x) + PAD_X, Fonts.centeredY(Fonts.Face.TAG, y + height / 2f),
            Theme.JADE_400);
    }

    /** Whether {@code px, py} is on the chip drawn at {@code x}, centred on {@code centerY}. */
    public boolean contains(double px, double py, float x, float centerY) {
        layout();
        float y = Draw.snap(centerY - height / 2f);
        return px >= x - 1 && px < x + width + 1 && py >= y - 1 && py < y + height + 1;
    }
}
