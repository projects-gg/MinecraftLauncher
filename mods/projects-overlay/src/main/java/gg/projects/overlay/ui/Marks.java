package gg.projects.overlay.ui;

import gg.projects.overlay.lang.Language;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.MarkSprites;
import java.util.List;
import net.minecraft.client.gui.GuiGraphicsExtractor;

/**
 * A person's profile badges beside their name (BADGES-UI; docs/design.md §6, "Profile marks"): only the first one the
 * catalogue knows (its order is importance), right after the name with the overlay's usual small gap, the same size
 * wherever it stands. The name gives way, the badge never does; it has no motion of its own and dims with its row.
 * The narrator hears every badge's name after the person's, and the pointer resting on the badge names them all
 * ({@link MarkTip}). Nowhere else: never in the system's notifications.
 */
public final class Marks {
    /** Between a name and its badge (the crew chip's and the favourite star's gap). */
    public static final float GAP = 3f;

    private Marks() {
    }

    /** How much a name's room shrinks for its badge: the gap and the badge (its shadow falls in the gap after it). */
    public static float room(List<OverlayState.MarkDef> marks) {
        return marks == null || marks.isEmpty() ? 0f : GAP + MarkSprites.SIZE;
    }

    /** The left edge of the badge right of a name that ends at {@code nameEnd}, on a screen pixel. */
    public static float left(float nameEnd) {
        return Draw.snap(nameEnd + GAP);
    }

    /** The top of a badge centred on {@code centerY} (the middle of a name's capitals), on a screen pixel. */
    public static float top(float centerY) {
        return Draw.snap(centerY - MarkSprites.SIZE / 2f);
    }

    /** Draws the first of {@code marks} right of a name that ends at {@code nameEnd}, centred on {@code centerY}. */
    public static void draw(GuiGraphicsExtractor g, List<OverlayState.MarkDef> marks, float nameEnd, float centerY) {
        if (marks == null || marks.isEmpty()) {
            return;
        }
        OverlayState.MarkDef first = marks.get(0);
        MarkSprites.draw(g, first.color(), first.glyph(), first.shape(), left(nameEnd), top(centerY));
    }

    /** Whether {@code (mx, my)} is on a badge drawn at {@code x, y} (its shadow too). */
    public static boolean over(double mx, double my, float x, float y) {
        return mx >= x && mx < x + MarkSprites.FOOT && my >= y && my < y + MarkSprites.FOOT;
    }

    /**
     * How the narrator names a person: their name, then every badge's name in {@code language} ("Kuzey_77, Projects
     * yöneticisi, Premium Club üyesi", as the launcher's rows say it); the name alone without badges.
     */
    public static String spoken(String name, List<OverlayState.MarkDef> marks, Language language) {
        if (marks == null || marks.isEmpty()) {
            return name;
        }
        StringBuilder b = new StringBuilder(name);
        for (OverlayState.MarkDef def : marks) {
            b.append(", ").append(def.name(language));
        }
        return b.toString();
    }
}
