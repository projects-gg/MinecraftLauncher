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
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * Codeless Discord linking (SKIN1 §3.7, SKIN1-O): a Projects server checked the launcher's permit with ProBot and
 * asks whether the game account may be linked to the Discord account the launcher is signed in with. The question
 * names the game account; the Discord account shows on its own card (the Discord symbol, the display name, the
 * unique user name under it, plain); "Hayır" (no question for 30 days) and "Eşle" answer it.
 *
 * <p>It comes by itself, over the game: nothing has the focus, and like the launcher's press guard (design.md,
 * POLISH5) a key or a click in its first {@link #ARM_MS} is not an answer, so a reflexive Enter is never a 30-day
 * "Hayır" or a link. Nothing dims meanwhile. Escape and closing are "not now": nothing goes to the server.
 */
public final class DiscordLinkScreen extends Screen {
    private static final int WIDTH = 236;
    private static final int PAD = 12;
    private static final int CARD_PAD = 7;
    private static final int MARK = 12;
    /** The card's two lines: the display name, then the user name. */
    private static final int CARD_H = CARD_PAD + 10 + 9 + CARD_PAD - 1;
    /** Before this, the question is read, not answered (the launcher's press guard). */
    static final long ARM_MS = 800;

    private final OverlayClient overlay;
    private final LinkMessages.Question question;
    private ActionButton no;
    private ActionButton yes;
    private List<FormattedCharSequence> title = List.of();
    private List<FormattedCharSequence> hint = List.of();
    private String name = "";
    private String user = "";
    private float cardW;
    private int cardX;
    private int cardY;
    private int cardH;
    private long openedAt;
    private boolean answered;

    public DiscordLinkScreen(OverlayClient overlay, LinkMessages.Question question) {
        super(Texts.tr("codeless.question", account()));
        this.overlay = overlay;
        this.question = question;
    }

    /** The game account the question is about: the one this game plays as. */
    private static String account() {
        return Minecraft.getInstance().getUser().getName();
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
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        int inner = WIDTH - 2 * PAD;
        title = balanced(Fonts.display(), getTitle().getString(), inner);
        hint = balanced(small, Texts.str("codeless.hint"), inner);
        // The Discord account's card fits its words, up to the full row; each line cuts on its own.
        int textRoom = inner - (CARD_PAD + MARK + 8 + CARD_PAD);
        name = Texts.fit(strong, question.displayName(), textRoom);
        user = Texts.fit(small, "@" + question.username(), textRoom);
        cardW = CARD_PAD + MARK + 8 + Math.max(strong.width(name), small.width(user)) + CARD_PAD;

        cardH = PAD + title.size() * 13 + 8 + CARD_H + 8 + hint.size() * 9 + 12 + ActionButton.HEIGHT + PAD;
        cardX = (width - WIDTH) / 2;
        cardY = (height - cardH) / 2;
        no = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("codeless.no"), null, () -> answer(false));
        yes = new ActionButton(ActionButton.Style.PRIMARY, Texts.tr("codeless.yes"), null, () -> answer(true));
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

    /**
     * {@code text} on at most two lines, the two as even as they go (as CSS {@code text-wrap: balance}): a long game
     * name never leaves a lone "mi?" or "account?" on the second line.
     */
    private static List<FormattedCharSequence> balanced(Font font, String text, int width) {
        List<FormattedCharSequence> lines = Texts.wrap(font, text, width, 2);
        if (lines.size() < 2) {
            return lines;
        }
        int low = width / 2;
        int high = width;
        while (low < high) {
            int mid = (low + high) / 2;
            if (Texts.wrap(font, text, mid, 3).size() <= 2) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        return Texts.wrap(font, text, high, 2);
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
        for (FormattedCharSequence line : title) {
            Draw.text(g, Fonts.display(), line, x, Fonts.centeredY(Fonts.Face.DISPLAY, y + 6.5f), Theme.BONE_50);
            y += 13;
        }
        y += 8;
        Draw.frame(g, Frames.CARD, x, y, cardW, CARD_H);
        Draw.icon(g, Icon.DISCORD, MARK, x + CARD_PAD, y + (CARD_H - MARK) / 2f, Theme.BONE_100);
        float tx = x + CARD_PAD + MARK + 8;
        Draw.text(g, Fonts.strong(), name, tx, Fonts.centeredY(Fonts.Face.STRONG, y + CARD_PAD + 5), Theme.BONE_50);
        Draw.text(g, Fonts.small(), user, tx, Fonts.centeredY(Fonts.Face.SMALL, y + CARD_PAD + 10 + 4.5f),
            Theme.ASH_300);
        y += CARD_H + 8;
        for (FormattedCharSequence line : hint) {
            Draw.text(g, Fonts.small(), line, x, Fonts.centeredY(Fonts.Face.SMALL, y + 4.5f), Theme.ASH_400);
            y += 9;
        }
        no.extractRenderState(g, mouseX, mouseY - (int) rise, partialTick);
        yes.extractRenderState(g, mouseX, mouseY - (int) rise, partialTick);
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    @Override
    public Component getNarrationMessage() {
        return Component.empty().append(getTitle()).append(" ")
            .append(Component.literal(question.displayName() + " (@" + question.username() + ")")).append(". ")
            .append(Texts.tr("codeless.hint"));
    }
}
