package gg.projects.overlay.ui;

import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.resources.Identifier;
import net.minecraft.util.FormattedCharSequence;

/**
 * A person's face where the overlay shows one (OVL-UX2): the launcher's {@code PlayerHead} ({@code @pl/ui},
 * docs/design.md §6). Their picture once the launcher sent it (OVL-AVATARS, {@link Avatars}: the launcher's own
 * rule, under the face's white hairline); until then, and for anyone without one, a square with the player head's
 * 4 px corners, burnt gold under a white hairline, and the name's first letter in Tektur, gold-200. The friend list,
 * the world's guests, voice chat and the message cards draw the same face, so a person looks the same wherever they
 * show (and as in the launcher). A crew's message card puts the crew sign in the same square.
 */
public final class Face {
    private final String letter;
    private final FormattedCharSequence line;
    private final Icon icon;
    /** Whose face it is (their user id, for their picture), or null for the letter alone. */
    private final String id;
    // The letter's width, measured again only when the GUI scale (and with it the font) changes.
    private float width;
    private int widthScale;

    private Face(String letter, Icon icon, String id) {
        this.letter = letter;
        this.line = letter == null ? null : Texts.visual(letter);
        this.icon = icon;
        this.id = id == null || id.isEmpty() ? null : id;
    }

    /** {@code name}'s face: its first letter. */
    public static Face of(String name) {
        return new Face(Texts.initial(name), null, null);
    }

    /** The face of {@code name}, user {@code id}: their picture once the launcher sent it, else the first letter. */
    public static Face of(String name, String id) {
        return new Face(Texts.initial(name), null, id);
    }

    /** A sign in the face's square (a crew's message card). */
    public static Face sign(Icon icon) {
        return new Face(null, icon, null);
    }

    /** The letter it shows, or null for a sign. */
    public String letter() {
        return letter;
    }

    /** Whose picture it shows, or null. */
    public String id() {
        return id;
    }

    /** Whether it is the face {@link #of(String, String)} makes for {@code name}, user {@code id}. */
    public boolean is(String name, String id) {
        String person = id == null || id.isEmpty() ? null : id;
        return Texts.initial(name).equals(letter) && java.util.Objects.equals(person, this.id);
    }

    /** Draws it {@code size} GUI pixels square, centred on {@code cx, cy} (on whole pixels, so its edges stay sharp). */
    public void draw(GuiGraphicsExtractor g, int size, float cx, float cy) {
        float x = Draw.snap(cx - size / 2f);
        float y = Draw.snap(cy - size / 2f);
        Identifier picture = id == null ? null : Avatars.picture(id, size * Draw.scale());
        if (picture == null) {
            letter(g, size, x, y);
            return;
        }
        float fade = Avatars.fade();
        if (fade < 1f) {
            // A picture that has just come: over the letter, fading in.
            letter(g, size, x, y);
            Draw.pushAlpha(fade);
        }
        int texels = Avatars.size();
        Draw.texture(g, picture, x, y, size, size, 0, 0, texels, texels, texels, texels, 0xFFFFFFFF);
        Draw.frame(g, Frames.FACE_RING, x, y, size, size);
        if (fade < 1f) {
            Draw.popAlpha();
        }
    }

    /** The letter's face (or the sign's) at {@code x, y}. */
    private void letter(GuiGraphicsExtractor g, int size, float x, float y) {
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
