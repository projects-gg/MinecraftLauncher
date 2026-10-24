package gg.projects.overlay.ui.panel;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.link.LinkMessages;
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
 * Codeless Discord linking (SKIN1 §3.7, SKIN1-O): a Projects server checked the launcher's permit with ProBot and
 * asks whether this game account may be linked to the Discord account the launcher is signed in with. The Discord
 * account is shown as it is (display name and unique user name, plain), "Hayır" and "Evet, eşle" answer it.
 *
 * <p>It comes by itself, over the game: nothing has the focus, and for {@link #ARM_MS} a key or a click is not an
 * answer (a reflexive Enter must never be a 30-day "Hayır", nor a link). The buttons fade in over that time.
 * Escape and closing are "not now": nothing goes to the server.
 */
public final class DiscordLinkScreen extends Screen {
    private static final int WIDTH = 236;
    private static final int PAD = 12;
    private static final int CHIP_H = 14;
    /** Before this, the question is read, not answered. */
    static final long ARM_MS = 600;
    /** The buttons start to show here and are whole at {@link #ARM_MS}. */
    private static final long BUTTONS_FROM_MS = 200;

    private final OverlayClient overlay;
    private final LinkMessages.Question question;
    private ActionButton no;
    private ActionButton yes;
    private String title = "";
    private List<FormattedCharSequence> body = List.of();
    private List<FormattedCharSequence> hint = List.of();
    private String name = "";
    private String user = "";
    private float chipW;
    private int cardX;
    private int cardY;
    private int cardH;
    private long openedAt;
    private boolean answered;

    public DiscordLinkScreen(OverlayClient overlay, LinkMessages.Question question) {
        super(Texts.tr("codeless.title"));
        this.overlay = overlay;
        this.question = question;
    }

    /** The question it shows (the game closes it when it leaves that server). */
    public LinkMessages.Question question() {
        return question;
    }

    private static long now() {
        return System.nanoTime() / 1_000_000L;
    }

    @Override
    protected void init() {
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        Font strong = Fonts.strong();
        int inner = WIDTH - 2 * PAD;
        title = getTitle().getString();
        body = Texts.wrap(ui, Texts.str("codeless.body"), inner, 3);
        hint = Texts.wrap(small, Texts.str("codeless.hint"), inner, 2);
        // The chip: the Discord symbol, the display name, then the unique user name; the user name gives way first.
        float room = inner - (5 + 10 + 4 + 6);
        String at = "@" + question.username();
        String shownName = Texts.fit(strong, question.displayName(), (int) Math.max(40, room - small.width(at) - 4));
        float nameW = strong.width(shownName);
        String shownUser = Texts.fit(small, at, (int) Math.max(0, room - nameW - 4));
        name = shownName;
        user = shownUser;
        chipW = 5 + 10 + 4 + nameW + (shownUser.isEmpty() ? 0 : 4 + small.width(shownUser)) + 6;

        int h = PAD + 14 + 4 + body.size() * 10 + 6 + CHIP_H + 6 + hint.size() * 9 + 12 + ActionButton.HEIGHT + PAD;
        cardH = h;
        cardX = (width - WIDTH) / 2;
        cardY = (height - cardH) / 2;
        no = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("codeless.no"), null, () -> answer(false));
        yes = new ActionButton(ActionButton.Style.PRIMARY, Texts.tr("codeless.yes"), Icon.LINK, () -> answer(true));
        int bw = Math.max(yes.preferredWidth(), no.preferredWidth());
        int by = cardY + cardH - PAD - ActionButton.HEIGHT;
        PanelWidget.place(yes, cardX + WIDTH - PAD - bw, by, bw, ActionButton.HEIGHT);
        PanelWidget.place(no, cardX + WIDTH - PAD - 2 * bw - 6, by, bw, ActionButton.HEIGHT);
        // "Hayır" first for Tab, and nothing focused: Enter does nothing until the player picks a button.
        addWidget(no);
        addWidget(yes);
        if (openedAt == 0) {
            openedAt = now();
        }
    }

    private boolean armed() {
        return now() - openedAt >= ARM_MS;
    }

    private void answer(boolean link) {
        if (answered || !armed()) {
            return;
        }
        answered = true;
        overlay.answerLink(question, link);
        Compat.setScreen(minecraft, null);
    }

    @Override
    public void onClose() {
        if (!answered) {
            answered = true;
            overlay.dismissLink(question);
        }
        Compat.setScreen(minecraft, null);
    }

    /** The game left the server: the question means nothing on another one. */
    public void withdraw() {
        answered = true;
        Compat.setScreen(minecraft, null);
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
        float t = Ease.OUT.at(Math.min(1f, (now() - openedAt) / 180f));
        Draw.fill(g, 0, 0, width, height, Theme.withAlpha(0x000000, 0.45f * t));
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor g, int mouseX, int mouseY, float partialTick) {
        long since = now() - openedAt;
        float t = Math.min(1f, since / 240f);
        float rise = overlay.reducedMotion() ? 0f : (1f - Ease.OUT_EXPO.at(t)) * 8f;
        Draw.pushAlpha(Ease.OUT.at(Math.min(1f, since / 160f)));
        g.pose().pushMatrix();
        g.pose().translate(0f, rise);
        Draw.frame(g, Frames.DIALOG, cardX, cardY, WIDTH, cardH);
        float x = cardX + PAD;
        float y = cardY + PAD;
        Draw.icon(g, Icon.LINK, 12, x, y + 1, Theme.GOLD_300);
        Draw.text(g, Fonts.display(), title, x + 17, Fonts.centeredY(Fonts.Face.DISPLAY, y + 7), Theme.BONE_50);
        y += 14 + 4;
        for (FormattedCharSequence line : body) {
            Draw.text(g, Fonts.ui(), line, x, Fonts.centeredY(Fonts.Face.UI, y + 5), Theme.ASH_300);
            y += 10;
        }
        y += 6;
        Draw.frame(g, Frames.ADDRESS, x, y, chipW, CHIP_H);
        float cx = x + 5;
        Draw.icon(g, Icon.DISCORD, 10, cx, y + (CHIP_H - 10) / 2f, Theme.BONE_100);
        cx += 10 + 4;
        Draw.text(g, Fonts.strong(), name, cx, Fonts.centeredY(Fonts.Face.STRONG, y + CHIP_H / 2f), Theme.BONE_50);
        if (!user.isEmpty()) {
            cx += Fonts.strong().width(name) + 4;
            Draw.text(g, Fonts.small(), user, cx, Fonts.centeredY(Fonts.Face.SMALL, y + CHIP_H / 2f), Theme.ASH_300);
        }
        y += CHIP_H + 6;
        for (FormattedCharSequence line : hint) {
            Draw.text(g, Fonts.small(), line, x, Fonts.centeredY(Fonts.Face.SMALL, y + 4.5f), Theme.ASH_400);
            y += 9;
        }
        // The answers come in last, as the question becomes answerable (reduced motion keeps this fade).
        Draw.pushAlpha(Ease.OUT.at(Math.max(0f, Math.min(1f,
            (since - BUTTONS_FROM_MS) / (float) (ARM_MS - BUTTONS_FROM_MS)))));
        no.extractRenderState(g, mouseX, mouseY - (int) rise, partialTick);
        yes.extractRenderState(g, mouseX, mouseY - (int) rise, partialTick);
        Draw.popAlpha();
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    @Override
    public Component getNarrationMessage() {
        return Component.empty().append(getTitle()).append(". ").append(Texts.tr("codeless.body")).append(" ")
            .append(Component.literal(question.displayName() + " (@" + question.username() + ")")).append(". ")
            .append(Texts.tr("codeless.hint"));
    }
}
