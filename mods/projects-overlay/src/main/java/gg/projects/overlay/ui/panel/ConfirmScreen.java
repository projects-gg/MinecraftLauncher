package gg.projects.overlay.ui.panel;

import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import java.util.List;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * One question over the panel (HOST1): a title, one or two short lines, maybe a quiet note, "Vazgeç" and the action.
 * The panel stays where it was behind; Esc and "Vazgeç" go back to it. An action that ends something (sending a
 * friend off, closing the world to friends) is in the danger style and the keyboard starts on "Vazgeç".
 */
public final class ConfirmScreen extends Screen {
    /** What is asked: {@code icon} beside the title, {@code note} quieter under the body, both optional. */
    public record Question(Component title, String body, String note, Component action, Icon icon,
        boolean destructive) {
    }

    private static final int WIDTH = 236;
    private static final int PAD = 12;

    private final OverlayScreen panel;
    private final Question question;
    private final Runnable onConfirm;
    private ActionButton confirm;
    private ActionButton cancel;
    private List<FormattedCharSequence> body = List.of();
    private List<FormattedCharSequence> note = List.of();
    private String title = "";
    private int cardX;
    private int cardY;
    private int cardH;
    private long openedAt;

    public ConfirmScreen(OverlayScreen panel, Question question, Runnable onConfirm) {
        super(question.title());
        this.panel = panel;
        this.question = question;
        this.onConfirm = onConfirm;
    }

    /** The panel this question stands over (it draws it behind). */
    OverlayScreen panel() {
        return panel;
    }

    @Override
    protected void init() {
        int textW = WIDTH - 2 * PAD;
        title = Texts.fit(Fonts.display(), getTitle().getString(), textW - (question.icon() == null ? 0 : 17));
        body = question.body() == null ? List.of() : Texts.wrap(Fonts.ui(), question.body(), textW, 4);
        note = question.note() == null ? List.of() : Texts.wrap(Fonts.small(), question.note(), textW, 2);
        int h = PAD + 14 + (body.isEmpty() ? 0 : 4 + body.size() * 10) + (note.isEmpty() ? 0 : 4 + note.size() * 9);
        h += 12 + ActionButton.HEIGHT + PAD;
        cardH = h;
        cardX = (width - WIDTH) / 2;
        cardY = (height - cardH) / 2;
        cancel = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("join.cancel"), null, this::onClose);
        confirm = new ActionButton(question.destructive() ? ActionButton.Style.DANGER : ActionButton.Style.PRIMARY,
            question.action(), null, this::confirm);
        int bw = Math.max(confirm.preferredWidth(), cancel.preferredWidth());
        int by = cardY + cardH - PAD - ActionButton.HEIGHT;
        PanelWidget.place(confirm, cardX + WIDTH - PAD - bw, by, bw, ActionButton.HEIGHT);
        PanelWidget.place(cancel, cardX + WIDTH - PAD - 2 * bw - 6, by, bw, ActionButton.HEIGHT);
        addWidget(cancel);
        addWidget(confirm);
        setInitialFocus(question.destructive() ? cancel : confirm);
        if (openedAt == 0) {
            openedAt = System.nanoTime() / 1_000_000L;
        }
    }

    private void confirm() {
        Compat.setScreen(minecraft, panel);
        onConfirm.run();
    }

    @Override
    public void onClose() {
        Compat.setScreen(minecraft, panel);
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        PanelInput.usedKeyboard();
        return super.keyPressed(event);
    }

    @Override
    public boolean isPauseScreen() {
        return panel.isPauseScreen();
    }

    @Override
    public void extractBackground(GuiGraphicsExtractor g, int mouseX, int mouseY, float partialTick) {
        long now = System.nanoTime() / 1_000_000L;
        panel.drawStill(g, now);
        float t = Ease.OUT.at(Math.min(1f, (now - openedAt) / 180f));
        Draw.fill(g, 0, 0, width, height, Theme.withAlpha(0x000000, 0.45f * t));
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor g, int mouseX, int mouseY, float partialTick) {
        long now = System.nanoTime() / 1_000_000L;
        float t = Math.min(1f, (now - openedAt) / 240f);
        float rise = panel.reducedMotion() ? 0f : (1f - Ease.OUT_EXPO.at(t)) * 8f;
        Draw.pushAlpha(Ease.OUT.at(Math.min(1f, (now - openedAt) / 160f)));
        g.pose().pushMatrix();
        g.pose().translate(0f, rise);
        Draw.frame(g, Frames.DIALOG, cardX, cardY, WIDTH, cardH);
        float x = cardX + PAD;
        float y = cardY + PAD;
        float tx = x;
        if (question.icon() != null) {
            Draw.icon(g, question.icon(), 12, x, y + 1, question.destructive() ? Theme.FLARE_300 : Theme.GOLD_300);
            tx += 17;
        }
        Draw.text(g, Fonts.display(), title, tx, Fonts.centeredY(Fonts.Face.DISPLAY, y + 7), Theme.BONE_50);
        y += 14;
        if (!body.isEmpty()) {
            y += 4;
            for (FormattedCharSequence line : body) {
                Draw.text(g, Fonts.ui(), line, x, Fonts.centeredY(Fonts.Face.UI, y + 5), Theme.ASH_300);
                y += 10;
            }
        }
        if (!note.isEmpty()) {
            y += 4;
            for (FormattedCharSequence line : note) {
                Draw.text(g, Fonts.small(), line, x, Fonts.centeredY(Fonts.Face.SMALL, y + 4.5f), Theme.ASH_400);
                y += 9;
            }
        }
        cancel.extractRenderState(g, mouseX, mouseY - (int) rise, partialTick);
        confirm.extractRenderState(g, mouseX, mouseY - (int) rise, partialTick);
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    @Override
    public Component getNarrationMessage() {
        Component out = Component.empty().append(getTitle());
        if (question.body() != null) {
            out = Component.empty().append(out).append(". ").append(question.body());
        }
        return out;
    }
}
