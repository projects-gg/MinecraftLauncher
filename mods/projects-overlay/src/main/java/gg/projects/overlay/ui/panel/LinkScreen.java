package gg.projects.overlay.ui.panel;

import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.state.LinkRule;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import java.net.URI;
import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * A link from the chat, asked once before the browser opens (CHAT2 §9; leaving the game for a browser should
 * never happen by accident): which site it goes to, the address itself, "Vazgeç" (where the focus starts) and
 * "Aç". Only a link the launcher let the player press and the mod's own check passes ({@link LinkRule}) gets
 * here; the game's own way of opening a link opens it. Over the panel, which stays where it was.
 */
public final class LinkScreen extends Screen {
    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");
    private static final int WIDTH = 236;
    private static final int PAD = 12;

    private final OverlayScreen panel;
    private final URI uri;
    private final String url;
    private final String domain;
    private ActionButton open;
    private ActionButton cancel;
    private List<FormattedCharSequence> body = List.of();
    private FormattedCharSequence address;
    private float addressW;
    private FormattedCharSequence note;
    private String title = "";
    private int cardX;
    private int cardY;
    private int cardH;
    private long openedAt;

    private LinkScreen(OverlayScreen panel, String url, URI uri) {
        super(Texts.tr("link.title"));
        this.panel = panel;
        this.url = url;
        this.uri = uri;
        this.domain = LinkRule.domain(LinkRule.host(url));
    }

    /** The question for {@code url}, or null when the mod would never open it. */
    public static LinkScreen of(OverlayScreen panel, String url) {
        if (!LinkRule.safe(url)) {
            return null;
        }
        URI uri = LinkRule.uri(url);
        return uri == null ? null : new LinkScreen(panel, url, uri);
    }

    @Override
    protected void init() {
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        title = getTitle().getString();
        body = Texts.wrap(ui, Texts.str("link.body", domain), WIDTH - 2 * PAD, 3);
        String shown = Texts.fit(small, url, WIDTH - 2 * PAD - 12);
        address = Texts.visual(shown);
        addressW = small.width(shown) + 12;
        note = Texts.visual(Texts.fit(small, Texts.str("link.browser"), WIDTH - 2 * PAD));
        int h = PAD + 14 + 4 + body.size() * 10 + 6 + 14 + 6 + 9 + 12 + ActionButton.HEIGHT + PAD;
        cardH = h;
        cardX = (width - WIDTH) / 2;
        cardY = (height - cardH) / 2;
        cancel = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("link.cancel"), null, this::onClose);
        open = new ActionButton(ActionButton.Style.PRIMARY, Texts.tr("link.open"), Icon.OPEN, this::confirm);
        int bw = Math.max(open.preferredWidth(), cancel.preferredWidth());
        int by = cardY + cardH - PAD - ActionButton.HEIGHT;
        PanelWidget.place(open, cardX + WIDTH - PAD - bw, by, bw, ActionButton.HEIGHT);
        PanelWidget.place(cancel, cardX + WIDTH - PAD - 2 * bw - 6, by, bw, ActionButton.HEIGHT);
        addWidget(cancel);
        addWidget(open);
        // Leaving the game is the step to think about: the focus starts on staying.
        setInitialFocus(cancel);
        if (openedAt == 0) {
            openedAt = System.nanoTime() / 1_000_000L;
        }
    }

    private void confirm() {
        try {
            Compat.openUri(uri);
        } catch (RuntimeException e) {
            LOG.warn("Could not open a link from the overlay: {}", e.toString());
        }
        onClose();
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
        Draw.frame(g, Frames.TOAST, cardX, cardY, WIDTH, cardH);
        float x = cardX + PAD;
        float y = cardY + PAD;
        Draw.icon(g, Icon.GLOBE, 12, x, y + 1, Theme.GOLD_300);
        Draw.text(g, Fonts.display(), title, x + 17, Fonts.centeredY(Fonts.Face.DISPLAY, y + 7), Theme.BONE_50);
        y += 14 + 4;
        for (FormattedCharSequence line : body) {
            Draw.text(g, Fonts.ui(), line, x, Fonts.centeredY(Fonts.Face.UI, y + 5), Theme.ASH_300);
            y += 10;
        }
        y += 6;
        Draw.frame(g, Frames.CHIP_SELECTED, x, y, addressW, 14);
        Draw.text(g, Fonts.small(), address, x + 6, Fonts.centeredY(Fonts.Face.SMALL, y + 7), Theme.BONE_100);
        y += 14 + 6;
        Draw.text(g, Fonts.small(), note, x, Fonts.centeredY(Fonts.Face.SMALL, y + 4.5f), Theme.ASH_400);
        cancel.extractRenderState(g, mouseX, mouseY - (int) rise, partialTick);
        open.extractRenderState(g, mouseX, mouseY - (int) rise, partialTick);
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    @Override
    public Component getNarrationMessage() {
        return Component.empty().append(getTitle()).append(". ").append(Texts.tr("link.body", domain))
            .append(" ").append(Texts.tr("link.browser"));
    }
}
