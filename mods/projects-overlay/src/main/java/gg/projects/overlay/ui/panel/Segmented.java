package gg.projects.overlay.ui.panel;

import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import java.util.function.IntConsumer;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;

/**
 * The launcher's segmented control: a chamfered well whose gold plate glides to the chosen option on the
 * sliding markers' spring (a pick during a glide carries on from where the plate is). Each label is ink
 * exactly as much as the plate covers it, so nothing snaps. Arrow keys pick; reduced motion jumps.
 */
public class Segmented extends PanelWidget {
    public static final int HEIGHT = 16;
    private static final int INSET = 2;

    private final Component[] options;
    private final String[] labels;
    private final int[] labelWidths;
    private final boolean[] markers;
    private final IntConsumer onPick;
    private final Spring plateX = Spring.snappy(0f);
    private int selected;
    private boolean placed;
    private boolean reducedMotion;
    private long lastFrame;

    public Segmented(Component name, Component[] options, int selected, IntConsumer onPick) {
        super(0, 0, 100, HEIGHT, name);
        this.options = options;
        this.labels = new String[options.length];
        this.labelWidths = new int[options.length];
        this.markers = new boolean[options.length];
        for (int i = 0; i < options.length; i++) {
            labels[i] = options[i].getString();
            labelWidths[i] = Fonts.ui().width(labels[i]);
        }
        this.selected = selected;
        this.onPick = onPick;
    }

    public int selected() {
        return selected;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    /** A small gold dot after an option's label (unread messages under a tab). */
    public void setMarker(int option, boolean on) {
        markers[option] = on;
    }

    /** Selects without telling the listener (the panel changed page by itself). */
    public void select(int option) {
        selected = Math.max(0, Math.min(options.length - 1, option));
    }

    /** The smallest width that fits every label. */
    public int preferredWidth() {
        int widest = 0;
        for (int w : labelWidths) {
            widest = Math.max(widest, w);
        }
        return (widest + 18) * options.length + 2 * INSET;
    }

    private float slot() {
        return (getWidth() - 2f * INSET) / options.length;
    }

    private void pick(int option) {
        if (option < 0 || option >= options.length || option == selected) {
            return;
        }
        selected = option;
        onPick.accept(option);
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        int option = (int) ((event.x() - getX() - INSET) / slot());
        pick(Math.max(0, Math.min(options.length - 1, option)));
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (event.isLeft()) {
            PanelInput.usedKeyboard();
            pick(selected - 1);
            return true;
        }
        if (event.isRight()) {
            PanelInput.usedKeyboard();
            pick(selected + 1);
            return true;
        }
        return false;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float x = getX();
        float y = getY();
        float slot = slot();
        float target = INSET + selected * slot;
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        if (!placed || reducedMotion) {
            plateX.snap(target);
            placed = true;
        } else {
            plateX.setTarget(target);
            plateX.step(dt);
        }
        Draw.frame(g, Frames.SEGMENT_WELL, x, y, getWidth(), HEIGHT);
        float px = x + plateX.value();
        Draw.frame(g, Frames.SEGMENT_PLATE, px, y + INSET, slot, HEIGHT - 2 * INSET);
        if (keyboardFocus()) {
            Draw.frame(g, Frames.FOCUS_16, x, y, getWidth(), HEIGHT);
        }
        Font font = Fonts.ui();
        float textY = Fonts.centeredY(Fonts.Face.UI, y + HEIGHT / 2f);
        for (int i = 0; i < options.length; i++) {
            float ox = x + INSET + i * slot;
            float overlap = Math.max(0f, Math.min(ox + slot, px + slot) - Math.max(ox, px)) / slot;
            boolean lit = isHovered() && mouseX >= ox && mouseX < ox + slot;
            int idle = lit ? Theme.BONE_100 : Theme.ASH_300;
            int ink = Theme.mix(idle, Theme.GOLD_INK, overlap);
            float lx = ox + (slot - labelWidths[i]) / 2f - (markers[i] ? 3f : 0f);
            Draw.text(g, font, labels[i], lx, textY, ink);
            if (markers[i]) {
                Draw.disc(g, 5, lx + labelWidths[i] + 5f, y + HEIGHT / 2f,
                    Theme.mix(Theme.GOLD_500, Theme.GOLD_INK, overlap));
            }
        }
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, Texts.tr("narration.tab", options[selected], selected + 1,
            options.length));
        output.add(NarratedElementType.USAGE, Texts.tr("narration.tab.usage"));
    }
}
