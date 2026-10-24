package gg.projects.overlay.ui.panel;

import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * "Katıl", asked once: where the player is going (Projects, or the other server's address, shown in full),
 * that this game closes for it (a singleplayer world is saved first), "Vazgeç" and "Bağlan". Over the panel,
 * which stays where it was; Esc goes back to it.
 */
public final class JoinScreen extends Screen {
    /** Where to go: a friend's server or a crew call's. */
    public record Target(String friendId, String name, String host, boolean projects) {
    }

    private static final int WIDTH = 236;
    private static final int PAD = 12;

    private final OverlayScreen panel;
    private final Target target;
    private ActionButton connect;
    private ActionButton cancel;
    private List<FormattedCharSequence> body = List.of();
    private String title = "";
    private String saved;
    private int cardX;
    private int cardY;
    private int cardH;
    private long openedAt;

    public JoinScreen(OverlayScreen panel, Target target) {
        super(Texts.tr(target.projects() ? "join.title.projects" : "join.title.server"));
        this.panel = panel;
        this.target = target;
    }

    /** The panel this question stands over (it draws it behind). */
    OverlayScreen panel() {
        return panel;
    }

    @Override
    protected void init() {
        Font ui = Fonts.ui();
        title = getTitle().getString();
        String text = Texts.str("join.body", target.name());
        body = Texts.wrap(ui, text, WIDTH - 2 * PAD, 4);
        saved = minecraft.hasSingleplayerServer() ? Texts.str("join.saved") : null;
        int h = PAD + 14 + 4 + body.size() * 10;
        if (!target.projects()) {
            h += 6 + 14;
        }
        if (saved != null) {
            h += 4 + 9;
        }
        h += 12 + ActionButton.HEIGHT + PAD;
        cardH = h;
        cardX = (width - WIDTH) / 2;
        cardY = (height - cardH) / 2;
        cancel = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("join.cancel"), null, this::onClose);
        connect = new ActionButton(ActionButton.Style.PRIMARY, Texts.tr("join.connect"), Icon.JOIN, this::confirm);
        int bw = Math.max(connect.preferredWidth(), cancel.preferredWidth());
        int by = cardY + cardH - PAD - ActionButton.HEIGHT;
        PanelWidget.place(connect, cardX + WIDTH - PAD - bw, by, bw, ActionButton.HEIGHT);
        PanelWidget.place(cancel, cardX + WIDTH - PAD - 2 * bw - 6, by, bw, ActionButton.HEIGHT);
        addWidget(cancel);
        addWidget(connect);
        setInitialFocus(connect);
        if (openedAt == 0) {
            openedAt = System.nanoTime() / 1_000_000L;
        }
    }

    private void confirm() {
        OverlayClient overlay = OverlayClient.get();
        if (overlay != null) {
            overlay.connect(target);
        }
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
        return false;
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
        Draw.icon(g, Icon.JOIN, 12, x, y + 1, Theme.GOLD_300);
        Draw.text(g, Fonts.display(), title, x + 17, Fonts.centeredY(Fonts.Face.DISPLAY, y + 7), Theme.BONE_50);
        y += 14 + 4;
        for (FormattedCharSequence line : body) {
            Draw.text(g, Fonts.ui(), line, x, Fonts.centeredY(Fonts.Face.UI, y + 5), Theme.ASH_300);
            y += 10;
        }
        if (!target.projects()) {
            y += 6;
            Font display = Fonts.display();
            String host = Texts.fit(display, target.host(), WIDTH - 2 * PAD - 12);
            float hw = display.width(host) + 12;
            Draw.frame(g, Frames.ADDRESS, x, y, hw, 14);
            Draw.text(g, display, host, x + 6, Fonts.centeredY(Fonts.Face.DISPLAY, y + 7), Theme.GOLD_200);
            y += 14;
        }
        if (saved != null) {
            y += 4;
            Draw.text(g, Fonts.small(), saved, x, Fonts.centeredY(Fonts.Face.SMALL, y + 4.5f), Theme.ASH_400);
        }
        cancel.extractRenderState(g, mouseX, mouseY - (int) rise, partialTick);
        connect.extractRenderState(g, mouseX, mouseY - (int) rise, partialTick);
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    @Override
    public Component getNarrationMessage() {
        return Component.empty().append(getTitle()).append(". ").append(Texts.tr("join.body", target.name()));
    }
}
