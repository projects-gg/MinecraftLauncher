package gg.projects.overlay.ui.panel;

import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * A setting that is on or off: its words on the left, the settings page's switch on the right (the control on the
 * right, so a longer label grows left). The whole row toggles; the thumb glides on the snappy spring.
 */
public final class ToggleRow extends PanelWidget {
    public static final int HEIGHT = 16;
    private static final int TOGGLE_W = 18;
    private static final int TOGGLE_H = 10;

    private final Spring thumb = Spring.snappy(0f);
    private final Tween lit = new Tween(0f);
    private boolean on;
    private boolean placed;
    private boolean reducedMotion;
    private long lastFrame;
    private FormattedCharSequence line;
    private int lineFor = -1;

    public ToggleRow(Component label, boolean on) {
        super(0, 0, 100, HEIGHT, label);
        this.on = on;
    }

    public boolean on() {
        return on;
    }

    public void set(boolean value) {
        on = value;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    private void flip() {
        if (active) {
            on = !on;
            pressed();
        }
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        flip();
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (event.isSelection() || event.isConfirmation()) {
            PanelInput.usedKeyboard();
            flip();
            return true;
        }
        return false;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        Font ui = Fonts.ui();
        if (lineFor != getWidth()) {
            lineFor = getWidth();
            line = Texts.visual(Texts.fit(ui, getMessage().getString(), getWidth() - TOGGLE_W - 8));
        }
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        float target = on ? 1f : 0f;
        if (!placed || reducedMotion) {
            thumb.snap(target);
            lit.snap(target);
            placed = true;
        } else {
            thumb.setTarget(target);
            thumb.step(dt);
            if (lit.target() != target) {
                lit.animateTo(target, now, 150, Ease.OUT);
            }
        }
        float mid = getY() + HEIGHT / 2f;
        float h = hover.value(now);
        Draw.text(g, ui, line, getX(), Fonts.centeredY(Fonts.Face.UI, mid), Theme.mix(Theme.ASH_300, Theme.BONE_100,
            Math.max(h, 0.35f)));
        float tx = getX() + getWidth() - TOGGLE_W;
        float ty = mid - TOGGLE_H / 2f;
        float l = lit.value(now);
        Draw.frame(g, Frames.TOGGLE_OFF, tx, ty, TOGGLE_W, TOGGLE_H);
        if (l > 0f) {
            Draw.pushAlpha(l);
            Draw.frame(g, Frames.TOGGLE_ON, tx, ty, TOGGLE_W, TOGGLE_H);
            Draw.popAlpha();
        }
        if (keyboardFocus()) {
            Draw.frame(g, Frames.FOCUS_10, tx, ty, TOGGLE_W, TOGGLE_H);
        }
        float travel = TOGGLE_W - TOGGLE_H;
        Draw.disc(g, 8, tx + TOGGLE_H / 2f + travel * thumb.value(), mid, Theme.mix(Theme.ASH_300, Theme.BONE_50, l));
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, Texts.tr("narration.setting", getMessage(),
            Texts.tr(on ? "host.narration.on" : "host.narration.off")));
    }
}
