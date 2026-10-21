package gg.projects.overlay.ui.panel;

import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.ui.draw.Draw;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.sounds.SoundManager;
import net.minecraft.network.chat.Component;

/**
 * Base of the panel's controls: the game's widget (focus, keyboard navigation, narration, clicks) with our
 * own drawing. Hover and keyboard focus fade in and out (opacity only); the press darkens for a moment.
 * The panel is quiet: no click sounds (its cues are for messages and calls).
 */
public abstract class PanelWidget extends AbstractWidget {
    static final long HOVER_MS = 120;
    static final long PRESS_MS = 140;

    protected final Tween hover = new Tween(0f);
    protected final Tween press = new Tween(0f);
    private boolean wasLit;

    protected PanelWidget(int x, int y, int w, int h, Component message) {
        super(x, y, w, h, message);
    }

    static long now() {
        return System.nanoTime() / 1_000_000L;
    }

    /** Puts a widget at {@code x, y} with the given size. */
    public static void place(AbstractWidget widget, int x, int y, int width, int height) {
        widget.setX(x);
        widget.setY(y);
        widget.setWidth(width);
        widget.setHeight(height);
    }

    @Override
    protected final void extractWidgetRenderState(GuiGraphicsExtractor g, int mouseX, int mouseY, float partialTick) {
        long now = now();
        boolean lit = active && isHovered();
        if (lit != wasLit) {
            hover.animateTo(lit ? 1f : 0f, now, HOVER_MS, Ease.OUT);
            wasLit = lit;
        }
        Draw.pushAlpha(active ? 1f : 0.45f);
        draw(g, mouseX, mouseY, now);
        Draw.popAlpha();
    }

    /** Draws the control; {@link #hover} and {@link #press} are current. */
    protected abstract void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now);

    /** Keyboard focus that should show a ring (the game moves focus by mouse too; the ring is for keys). */
    protected boolean keyboardFocus() {
        return isFocused() && PanelInput.keyboardMode();
    }

    protected void pressed() {
        long now = now();
        press.snap(1f);
        press.animateTo(0f, now, PRESS_MS, Ease.OUT);
    }

    @Override
    public void playDownSound(SoundManager manager) {
        // Quiet on purpose.
    }
}
