package gg.projects.overlay.ui.panel;

import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.LabelFit;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import net.minecraft.client.gui.GuiGraphicsExtractor;

/**
 * The chat picker's two tabs, "Emoji" and "Çıkartmalar" (OVL-SOC; the launcher's picker panel puts them under one
 * segmented control): a well with the chosen tab on the quiet plate and its words in bone, the other in ash (bone under
 * the pointer). Each picker draws them at its top; a press on the other tab, or ← and → while they are picked with the
 * keys, swaps the picker in place. No gold: a chosen option is never gold (docs/design.md §6), and the panel's own tabs
 * may be on screen beside it.
 */
final class PickerTabs {
    static final int HEIGHT = 16;
    /** The room under the tabs before the picker's own rows. */
    static final int BELOW = 5;
    static final int EMOJI = 0;
    static final int STICKERS = 1;
    private static final int INSET = 1;
    /** Each side of a tab's words inside it. */
    private static final int LABEL_PAD = 5;
    private static final String[] KEYS = {"chat.emoji", "chat.stickers"};

    /** A tab asked for: by the pointer, or by the keys ({@code keys}: they stay on the tabs). */
    interface Switch {
        void to(int tab, boolean keys);
    }

    private PickerTabs() {
    }

    /** The tab under {@code (mx, my)} for tabs at {@code x, y} {@code w} wide, or -1. */
    static int at(double mx, double my, float x, float y, float w) {
        if (my < y || my >= y + HEIGHT || mx < x || mx >= x + w) {
            return -1;
        }
        return mx < x + w / 2f ? EMOJI : STICKERS;
    }

    /** Draws them; {@code focus}: the tabs are picked with the keys (a ring around the chosen one). */
    static void draw(GuiGraphicsExtractor g, float x, float y, float w, int chosen, int mouseX, int mouseY,
        boolean focus) {
        Draw.frame(g, Frames.SEGMENT_WELL, x, y, w, HEIGHT);
        float slot = (w - 2f * INSET) / 2f;
        float px = x + INSET + chosen * slot;
        Draw.frame(g, Frames.CHIP_HOVER, px, y + INSET, slot, HEIGHT - 2 * INSET);
        if (focus) {
            Draw.frame(g, Frames.FOCUS_14, px, y + INSET, slot, HEIGHT - 2 * INSET);
        }
        int lit = at(mouseX, mouseY, x, y, w);
        for (int i = 0; i < 2; i++) {
            String label = Texts.str(KEYS[i]);
            float ox = x + INSET + i * slot;
            int ink = i == chosen ? Theme.BONE_50 : i == lit ? Theme.BONE_100 : Theme.ASH_300;
            // LABEL-FIT: the words inside their tab, smaller or cut only when they must be.
            LabelFit.Fit fit = LabelFit.fit(label, Fonts.Face.UI, 0, Math.round(slot) - 2 * LABEL_PAD,
                Fonts.bundled(), LabelFit.FONTS);
            LabelFit.noteForDriver(label, Fonts.Face.UI, false, Draw.scale(), fit);
            Draw.text(g, Fonts.get(fit.face()), fit.text(), ox + (slot - fit.width()) / 2f,
                Fonts.centeredY(fit.face(), y + HEIGHT / 2f), ink);
        }
    }
}
