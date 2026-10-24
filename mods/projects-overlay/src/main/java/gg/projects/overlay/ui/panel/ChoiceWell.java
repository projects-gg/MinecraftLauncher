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
 * A quiet choice across the panel's width: the settings page's chip well (chamfered, small type) with its selected
 * plate gliding on the sliding markers' spring. Quieter than the gold segmented tabs on purpose: next to it the gold
 * action is the one loud thing. Equal slots; a label that does not fit its slot is shortened. Arrow keys pick;
 * reduced motion jumps.
 */
public final class ChoiceWell extends PanelWidget {
    public static final int HEIGHT = 14;
    private static final int INSET = 2;

    private final Component[] options;
    private final String[] labels;
    private final IntConsumer onPick;
    private final Spring plate = Spring.snappy(0f);
    private String[] fitted;
    private float[] widths;
    private int fittedFor = -1;
    private int selected;
    private boolean placed;
    private boolean reducedMotion;
    private long lastFrame;

    public ChoiceWell(Component name, Component[] options, int selected, IntConsumer onPick) {
        super(0, 0, 100, HEIGHT, name);
        this.options = options;
        this.labels = new String[options.length];
        for (int i = 0; i < options.length; i++) {
            labels[i] = options[i].getString();
        }
        this.selected = selected;
        this.onPick = onPick;
    }

    public int selected() {
        return selected;
    }

    /** Selects without telling the listener (the screen opened on the world's own mode). */
    public void select(int option) {
        selected = Math.max(0, Math.min(options.length - 1, option));
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    private float slot() {
        return (getWidth() - 2f * INSET) / options.length;
    }

    private void pick(int option) {
        if (!active || option < 0 || option >= options.length || option == selected) {
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
        Font small = Fonts.small();
        float slot = slot();
        if (fittedFor != getWidth()) {
            fittedFor = getWidth();
            fitted = new String[labels.length];
            widths = new float[labels.length];
            for (int i = 0; i < labels.length; i++) {
                fitted[i] = Texts.fit(small, labels[i], Math.max(8, Math.round(slot) - 6));
                widths[i] = small.width(fitted[i]);
            }
        }
        float x = getX();
        float y = getY();
        float target = selected * slot;
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        if (!placed || reducedMotion) {
            plate.snap(target);
            placed = true;
        } else {
            plate.setTarget(target);
            plate.step(dt);
        }
        Draw.frame(g, Frames.CHIP, x, y, getWidth(), HEIGHT);
        float px = x + INSET + plate.value();
        Draw.frame(g, Frames.CHIP_SELECTED, px, y, slot, HEIGHT);
        if (keyboardFocus()) {
            Draw.frame(g, Frames.FOCUS_14, x, y, getWidth(), HEIGHT);
        }
        float mid = y + HEIGHT / 2f;
        for (int i = 0; i < labels.length; i++) {
            float ox = x + INSET + i * slot;
            float overlap = Math.max(0f, Math.min(ox + slot, px + slot) - Math.max(ox, px)) / slot;
            boolean over = isHovered() && active && mouseX >= ox && mouseX < ox + slot;
            int ink = Theme.mix(over ? Theme.BONE_100 : Theme.ASH_300, Theme.BONE_50, overlap);
            Draw.text(g, small, fitted[i], ox + (slot - widths[i]) / 2f, Fonts.centeredY(Fonts.Face.SMALL, mid), ink);
        }
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, Texts.tr("narration.setting", getMessage(), options[selected]));
        output.add(NarratedElementType.USAGE, Texts.tr("narration.tab.usage"));
    }
}
