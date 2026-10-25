package gg.projects.overlay.ui;

import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.util.FormattedCharSequence;

/**
 * A person's face where the overlay shows one (OVL-UX2): the launcher's {@code PlayerHead} without a picture
 * ({@code @pl/ui}, docs/design.md §6), since the bridge carries no pictures. A square with the player head's 4 px
 * corners, burnt gold under a white hairline, and the name's first letter in Tektur, gold-200. The friend list, the
 * world's guests and the message cards draw the same face, so a person looks the same wherever they show (and as in
 * the launcher). A crew's message card puts the crew sign in the same square.
 */
public final class Face {
    private final String letter;
    private final FormattedCharSequence line;
    private final Icon icon;
    // The letter's width, measured again only when the GUI scale (and with it the font) changes.
    private float width;
    private int widthScale;

    private Face(String letter, Icon icon) {
        this.letter = letter;
        this.line = letter == null ? null : Texts.visual(letter);
        this.icon = icon;
    }

    /** {@code name}'s face: its first letter. */
    public static Face of(String name) {
        return new Face(Texts.initial(name), null);
    }

    /** A sign in the face's square (a crew's message card). */
    public static Face sign(Icon icon) {
        return new Face(null, icon);
    }

    /** The letter it shows, or null for a sign. */
    public String letter() {
        return letter;
    }

    /** Draws it {@code size} GUI pixels square, centred on {@code cx, cy} (on whole pixels, so its edges stay sharp). */
    public void draw(GuiGraphicsExtractor g, int size, float cx, float cy) {
        float x = Draw.snap(cx - size / 2f);
        float y = Draw.snap(cy - size / 2f);
        Draw.frame(g, Frames.FACE, x, y, size, size);
        if (line != null) {
            Font font = Fonts.face();
            // The game's own font stands in at GUI scale 1 and with "Force Unicode Font".
            int scale = Draw.scale() * 2 + (Fonts.bundled() ? 1 : 0);
            if (scale != widthScale) {
                width = font.width(line);
                widthScale = scale;
            }
            Draw.text(g, font, line, x + (size - width) / 2f, Fonts.centeredY(Fonts.Face.FACE, y + size / 2f),
                Theme.GOLD_200);
        } else if (icon != null) {
            Draw.icon(g, icon, 10, x + (size - 10) / 2f, y + (size - 10) / 2f, Theme.GOLD_200);
        }
    }
}
