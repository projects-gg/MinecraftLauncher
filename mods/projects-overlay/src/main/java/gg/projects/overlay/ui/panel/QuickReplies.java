package gg.projects.overlay.ui.panel;

import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import java.util.function.Consumer;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;

/**
 * Crew chat's quick replies, the launcher's four ("Geliyorum", "5 dk", "Neredesin?", "Gel"): one press
 * sends. They fade away while the player writes something of their own.
 */
public final class QuickReplies extends PanelWidget {
    public static final int HEIGHT = 14;
    private static final String[] KEYS = {"chat.quick.coming", "chat.quick.five", "chat.quick.where", "chat.quick.come"};

    private final Consumer<String> onSend;
    private final String[] labels = new String[KEYS.length];
    private final float[] widths = new float[KEYS.length];
    private final Tween shown = new Tween(1f);
    private int selected;
    private boolean hidden;

    public QuickReplies(Consumer<String> onSend) {
        super(0, 0, 100, HEIGHT, Texts.tr("chat.quick"));
        this.onSend = onSend;
        Font small = Fonts.small();
        for (int i = 0; i < KEYS.length; i++) {
            labels[i] = Texts.str(KEYS[i]);
            widths[i] = small.width(labels[i]) + 12;
        }
    }

    /** Hidden while the field has text: still laid out (nothing moves), just not there. */
    public void setHidden(boolean value, long now) {
        if (value != hidden) {
            hidden = value;
            shown.animateTo(value ? 0f : 1f, now, value ? 110 : 160, Ease.OUT);
        }
        active = !value;
    }

    private int at(double mouseX) {
        float x = getX();
        for (int i = 0; i < labels.length; i++) {
            if (mouseX >= x && mouseX < x + widths[i]) {
                return i;
            }
            x += widths[i] + 4;
        }
        return -1;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        int i = at(event.x());
        if (i >= 0) {
            selected = i;
            pressed();
            onSend.accept(labels[i]);
        }
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (event.isLeft() && selected > 0) {
            PanelInput.usedKeyboard();
            selected--;
            return true;
        }
        if (event.isRight() && selected < labels.length - 1) {
            PanelInput.usedKeyboard();
            selected++;
            return true;
        }
        if (event.isSelection() || event.isConfirmation()) {
            PanelInput.usedKeyboard();
            pressed();
            onSend.accept(labels[selected]);
            return true;
        }
        return false;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float a = shown.value(now);
        if (a <= 0.01f) {
            return;
        }
        Draw.pushAlpha(a);
        Font small = Fonts.small();
        float x = getX();
        float y = getY();
        for (int i = 0; i < labels.length; i++) {
            boolean lit = isHovered() && mouseX >= x && mouseX < x + widths[i];
            Draw.frame(g, lit ? Frames.CHIP_HOVER : Frames.CHIP, x, y, widths[i], HEIGHT);
            if (keyboardFocus() && i == selected) {
                Draw.frame(g, Frames.FOCUS_14, x, y, widths[i], HEIGHT);
            }
            Draw.text(g, small, labels[i], x + 6, Fonts.centeredY(Fonts.Face.SMALL, y + HEIGHT / 2f),
                lit ? Theme.BONE_50 : Theme.ASH_300);
            x += widths[i] + 4;
        }
        Draw.popAlpha();
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, Texts.tr("narration.quick", labels[selected]));
    }
}
