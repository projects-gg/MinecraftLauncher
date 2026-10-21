package gg.projects.overlay.ui.panel;

import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import java.util.function.BooleanSupplier;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;

/**
 * The send button inside the message field, as in the launcher: quiet while there is nothing to send, its
 * gold face fading in once there is (a gradient cannot fade by itself, so the face is a layer).
 */
public final class SendButton extends PanelWidget {
    public static final int SIZE = 18;

    private final BooleanSupplier ready;
    private final Runnable onSend;
    private final Tween face = new Tween(0f);

    public SendButton(BooleanSupplier ready, Runnable onSend) {
        super(0, 0, SIZE, SIZE, Texts.tr("chat.send"));
        this.ready = ready;
        this.onSend = onSend;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        send();
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (event.isSelection() || event.isConfirmation()) {
            PanelInput.usedKeyboard();
            send();
            return true;
        }
        return false;
    }

    private void send() {
        if (ready.getAsBoolean()) {
            pressed();
            onSend.run();
        }
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        boolean on = ready.getAsBoolean();
        face.animateTo(on ? 1f : 0f, now, 150, Ease.OUT);
        float f = face.value(now);
        float h = hover.value(now);
        float x = getX();
        float y = getY();
        if (f < 1f && h > 0f) {
            Draw.pushAlpha(h * (1f - f));
            Draw.frame(g, Frames.SEND_HOVER, x, y, SIZE, SIZE);
            Draw.popAlpha();
        }
        if (f > 0f) {
            Draw.pushAlpha(f);
            Draw.frame(g, Frames.SEND_READY, x, y, SIZE, SIZE);
            Draw.popAlpha();
        }
        if (keyboardFocus()) {
            Draw.frame(g, Frames.FOCUS_18, x, y, SIZE, SIZE);
        }
        float dy = press.value(now) * 0.5f;
        Draw.icon(g, Icon.SEND, 10, x + 4, y + 4 + dy, Theme.mix(Theme.ASH_400, Theme.GOLD_INK, f));
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        defaultButtonNarrationText(output);
    }
}
