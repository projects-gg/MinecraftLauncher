package gg.projects.overlay.ui;

import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.util.FormattedCharSequence;

/**
 * A crew tag's chip before a name (TEAM6: "[GK] Ayşe"): the tag in Tektur, bone-50 on the crew colour at 24 %,
 * chamfered as the launcher's {@code TeamTag}. Laid out once (its text keeps its layout) and drawn with two calls,
 * the frame and the text. The chip never shrinks; the name beside it does.
 */
public final class TagChip {
    /** Beside a name in a list row or a toast's title. */
    public static final int ROW = 9;
    /** Over a group of messages in crew chat. */
    public static final int CHAT = 8;
    /** Between the chip and the name. */
    public static final float GAP = 3f;
    /** Lets the measurement driver leave the chips out, everything else as it is (what the chips cost). */
    public static volatile boolean drawn = true;
    private static final float PAD_X = 2.5f;

    public final OverlayState.Badge badge;
    public final float width;
    public final int height;
    private final FormattedCharSequence text;
    private final int tint;
    private final Frames frame;

    private TagChip(OverlayState.Badge badge, int height) {
        this.badge = badge;
        this.text = Texts.visual(badge.tag());
        this.tint = CrewLook.tagTint(badge.color());
        // The game's own font (GUI 1, "Force Unicode Font") is taller than ours: the chip grows with it.
        this.height = Math.max(height, Math.round(Fonts.capHeight(Fonts.Face.TAG) + 4f));
        this.width = Fonts.tag().width(badge.tag()) + 2 * PAD_X;
        // Three slices at the usual heights; nine only for the game's taller font.
        this.frame = this.height == ROW ? Frames.TAG_9 : this.height == CHAT ? Frames.TAG_8 : Frames.TAG;
    }

    /** The chip for {@code badge} at one of the heights above, or null for no badge. */
    public static TagChip of(OverlayState.Badge badge, int height) {
        return badge == null ? null : new TagChip(badge, height);
    }

    /** How far a name moves right for the chip before it ({@code 0} for none). */
    public static float room(TagChip chip) {
        return chip == null ? 0f : chip.width + GAP;
    }

    /** The chip's top when it sits centred on {@code centerY}, on a screen pixel. */
    public float top(float centerY) {
        return Draw.snap(centerY - height / 2f);
    }

    /** Draws the chip with its left edge at {@code x}, centred on {@code centerY}. */
    public void draw(GuiGraphicsExtractor g, float x, float centerY) {
        if (!drawn) {
            return;
        }
        float y = top(centerY);
        Draw.frame(g, frame, x, y, width, height, tint);
        Draw.text(g, Fonts.tag(), text, x + PAD_X, Fonts.centeredY(Fonts.Face.TAG, y + height / 2f), Theme.BONE_50);
    }
}
