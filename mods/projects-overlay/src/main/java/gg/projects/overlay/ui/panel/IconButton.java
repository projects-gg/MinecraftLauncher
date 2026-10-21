package gg.projects.overlay.ui.panel;

import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;

/** A square icon button (settings, back, close): bare until hovered, its name read by the narrator. */
public class IconButton extends PanelWidget {
    public static final int SIZE = 16;

    private Icon icon;
    private final Runnable onPress;
    private int color = Theme.ASH_300;
    private int hoverColor = Theme.BONE_100;

    public IconButton(Icon icon, Component name, Runnable onPress) {
        super(0, 0, SIZE, SIZE, name);
        this.icon = icon;
        this.onPress = onPress;
    }

    public void setIcon(Icon next) {
        icon = next;
    }

    public void setColors(int idle, int lit) {
        color = idle;
        hoverColor = lit;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        pressed();
        onPress.run();
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (active && visible && (event.isSelection() || event.isConfirmation())) {
            PanelInput.usedKeyboard();
            pressed();
            onPress.run();
            return true;
        }
        return false;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float h = hover.value(now);
        float x = getX();
        float y = getY();
        if (h > 0f) {
            Draw.pushAlpha(h);
            Draw.frame(g, Frames.BUTTON_GHOST_HOVER, x, y, SIZE, SIZE);
            Draw.popAlpha();
        }
        if (keyboardFocus()) {
            Draw.frame(g, Frames.FOCUS_16, x, y, SIZE, SIZE);
        }
        float dy = press.value(now) * 0.5f;
        Draw.icon(g, icon, 10, x + 3, y + 3 + dy, Theme.mix(color, hoverColor, Math.max(h, isFocused() ? 1f : 0f)));
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        defaultButtonNarrationText(output);
    }
}
