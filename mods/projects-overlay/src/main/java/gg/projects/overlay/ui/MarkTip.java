package gg.projects.overlay.ui;

import gg.projects.overlay.lang.Language;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.MarkSprites;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.util.FormattedCharSequence;

/**
 * The tooltip of a badge right of a name (BADGES-UI): the pointer resting on the badge names every badge of that
 * person in the catalogue's order, in the overlay's language: one badge, just its name; several, a line each with its
 * badge before its name. The chat's tooltip as it looks and moves (CHAT2): it waits 280 ms (one right after another
 * shows at once), fades in over 110 ms and goes as soon as the pointer leaves, over the badge or, without room there,
 * under it. Pointer only: the narrator already says every badge with its row. Laid out once per person and language.
 */
public final class MarkTip {
    /** The pointer rests this long on a badge before its tooltip shows (the chat's tooltips wait as long). */
    public static final long DELAY_MS = 280;
    /** Another badge within this long of the last tooltip: no wait. */
    static final long QUICK_MS = 400;
    static final long FADE_MS = 110;
    static final int PAD_X = 6;
    static final int PAD_Y = 3;
    static final int LINE_H = 10;
    /** A line with its badge: the badge, its shadow and a pixel of air. */
    static final int MARK_LINE_H = 12;
    /** From a line's badge to its name. */
    static final int NAME_X = MarkSprites.SIZE + 4;
    static final int MAX_NAME_W = 150;
    /** Between the badge and the tooltip. */
    private static final int GAP = 3;
    /** Not followed for this long (its list was not drawn): what it showed is forgotten. */
    private static final long STALE_MS = 250;

    private long trackedAt = Long.MIN_VALUE / 2;
    private Object key;
    private List<OverlayState.MarkDef> marks = List.of();
    private float anchorX;
    private float anchorY;
    private long since;
    private long closedAt = Long.MIN_VALUE / 2;
    private Language laidOutIn;
    private final List<FormattedCharSequence> names = new ArrayList<>(4);
    private float width;
    private float height;

    /**
     * Follows what the pointer rests on this frame: the badge drawn at {@code x, y} of the person {@code key} stands for
     * (their badges: {@code marks}), or nothing ({@code key} null).
     */
    public void track(Object key, List<OverlayState.MarkDef> marks, float x, float y, long now) {
        if (now - trackedAt > STALE_MS) {
            // The list was away (the panel closed, another page): a badge pointed at now waits its moment again.
            this.key = null;
            closedAt = Long.MIN_VALUE / 2;
        }
        trackedAt = now;
        if (key == null || marks == null || marks.isEmpty()) {
            if (this.key != null) {
                this.key = null;
                closedAt = now;
            }
            return;
        }
        anchorX = x;
        anchorY = y;
        Language language = Texts.current();
        if (key.equals(this.key)) {
            if (!marks.equals(this.marks) || language != laidOutIn) {
                // Still on it: the catalogue or the language changed under it.
                this.marks = marks;
                layout(language);
            }
            return;
        }
        boolean quick = this.key != null || now - closedAt < QUICK_MS;
        this.key = key;
        this.marks = marks;
        since = quick ? now - DELAY_MS : now;
        layout(language);
    }

    /** Whether the pointer is on a badge (its tooltip shows or waits to). */
    public boolean active() {
        return key != null;
    }

    private void layout(Language language) {
        laidOutIn = language;
        names.clear();
        Font small = Fonts.small();
        float widest = 0f;
        for (OverlayState.MarkDef def : marks) {
            String name = Texts.fit(small, def.name(language), MAX_NAME_W);
            names.add(Texts.visual(name));
            widest = Math.max(widest, small.width(name));
        }
        boolean one = marks.size() == 1;
        width = 2 * PAD_X + (one ? 0 : NAME_X) + widest;
        height = 2 * PAD_Y + (one ? LINE_H : marks.size() * MARK_LINE_H);
    }

    /** Draws it over the badge (under it without room above), inside {@code left..right}, from {@code top} down. */
    public void draw(GuiGraphicsExtractor g, long now, float left, float top, float right) {
        if (key == null || now - since < DELAY_MS) {
            return;
        }
        float x = Math.max(left, Math.min(right - width, anchorX + MarkSprites.SIZE / 2f - width / 2f));
        float y = anchorY - GAP - height;
        if (y < top) {
            y = anchorY + MarkSprites.FOOT + GAP;
        }
        x = Draw.snap(x);
        y = Draw.snap(y);
        float t = Math.min(1f, (now - since - DELAY_MS) / (float) FADE_MS);
        Draw.pushAlpha(Ease.OUT.at(t));
        Draw.frame(g, Frames.TOOLTIP, x, y, width, height);
        Font small = Fonts.small();
        if (marks.size() == 1) {
            Draw.text(g, small, names.get(0), x + PAD_X, Fonts.centeredY(Fonts.Face.SMALL, y + PAD_Y + LINE_H / 2f),
                Theme.BONE_100);
        } else {
            for (int i = 0; i < marks.size(); i++) {
                OverlayState.MarkDef def = marks.get(i);
                float mid = y + PAD_Y + i * MARK_LINE_H + MARK_LINE_H / 2f;
                MarkSprites.draw(g, def.color(), def.glyph(), def.shape(), x + PAD_X, Marks.top(mid));
                Draw.text(g, small, names.get(i), x + PAD_X + NAME_X, Fonts.centeredY(Fonts.Face.SMALL, mid),
                    Theme.BONE_100);
            }
        }
        Draw.popAlpha();
    }
}
