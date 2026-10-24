package gg.projects.overlay.ui.panel;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * "Rapor et" from a friend's menu (OVL-UX; MOD1): the launcher's report dialog for a player, made to fit the game's
 * shortest screens (240 GUI pixels tall at GUI 3 and 720p). The title with whom it is about, the one sentence that
 * nobody is told who reported, the player's reasons as a radio list (arrows move the pick, as the launcher's), a note
 * for moderators (200 characters), "Ayrıca engelle" and "Vazgeç" · "Gönder" ("Gönder" waits for a reason). Over the
 * panel, which stays where it was; Esc goes back to it. The launcher sends it; the panel says how it went.
 */
public final class ReportScreen extends Screen {
    private static final int WIDTH = 236;
    private static final int PAD = 10;
    private static final int REASON_H = 13;
    private static final int REASON_GAP = 1;
    private static final int FIELD_H = 18;
    private static final int CHECK_H = 12;

    private final OverlayScreen panel;
    private final String friendId;
    private final String friendName;
    private final OverlayState.Badge badge;
    private ReasonList reasons;
    private ComposerField note;
    private CheckRow block;
    private ActionButton cancel;
    private ActionButton send;
    private String title = "";
    private FormattedCharSequence nameLine;
    private TagChip chip;
    private List<FormattedCharSequence> privacy = List.of();
    private String help = "";
    private int cardX;
    private int cardY;
    private int cardH;
    private int noteY;
    private long openedAt;
    private String reason;

    public ReportScreen(OverlayScreen panel, String friendId, String friendName, OverlayState.Badge badge) {
        super(Texts.tr("report.title"));
        this.panel = panel;
        this.friendId = friendId;
        this.friendName = friendName == null || friendName.isEmpty() ? Texts.str("toast.someone") : friendName;
        this.badge = badge;
    }

    /** The panel this dialog stands over (it draws it behind). */
    OverlayScreen panel() {
        return panel;
    }

    @Override
    protected void init() {
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        title = getTitle().getString();
        int textW = WIDTH - 2 * PAD;
        // Whom it is about, at the title's right: "[GK] Ayşe", the name giving way first.
        chip = TagChip.of(badge, TagChip.ROW);
        float titleW = Fonts.display().width(title);
        int nameRoom = Math.max(30, Math.round(textW - titleW - 12 - TagChip.room(chip)));
        String fitted = Texts.fit(strong, friendName, nameRoom);
        nameLine = Texts.visual(fitted);
        privacy = Texts.wrap(small, Texts.str("report.private"), textW, 2);
        help = Texts.str("report.note.help");
        int reasonsH = Protocol.REPORT_REASONS.size() * (REASON_H + REASON_GAP) - REASON_GAP;
        cardH = PAD + 14 + 3 + privacy.size() * 9 + 8 + reasonsH + 6 + FIELD_H + 2 + 9 + 6 + CHECK_H + 9
            + ActionButton.HEIGHT + PAD;
        cardX = cardX(width);
        cardY = Math.max(4, (height - cardH) / 2);
        int y = cardY + PAD + 14 + 3 + privacy.size() * 9 + 8;
        if (reasons == null) {
            reasons = new ReasonList(this::picked);
            note = new ComposerField(Texts.tr("report.note"));
            note.setMaxLength(Protocol.MAX_REPORT_NOTE_CHARS);
            note.setPlaceholder(Texts.tr("report.note"));
            block = new CheckRow(Texts.tr("report.block"));
            cancel = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("people.cancel"), null, this::onClose);
            send = new ActionButton(ActionButton.Style.PRIMARY, Texts.tr("report.send"), null, this::sendReport);
        }
        PanelWidget.place(reasons, cardX + PAD, y, textW, reasonsH);
        y += reasonsH + 6;
        noteY = y;
        PanelWidget.place(note, cardX + PAD + 7, y + (FIELD_H - 12) / 2, textW - 14, 12);
        y += FIELD_H + 2 + 9 + 6;
        PanelWidget.place(block, cardX + PAD, y, Math.min(textW, block.preferredWidth()), CHECK_H);
        int bw = Math.max(send.preferredWidth(), cancel.preferredWidth());
        int by = cardY + cardH - PAD - ActionButton.HEIGHT;
        PanelWidget.place(send, cardX + WIDTH - PAD - bw, by, bw, ActionButton.HEIGHT);
        PanelWidget.place(cancel, cardX + WIDTH - PAD - 2 * bw - 6, by, bw, ActionButton.HEIGHT);
        send.active = reason != null;
        addWidget(reasons);
        addWidget(note);
        addWidget(block);
        addWidget(cancel);
        addWidget(send);
        setInitialFocus(reasons);
        if (openedAt == 0) {
            openedAt = System.nanoTime() / 1_000_000L;
        }
    }

    private static int cardX(int screenWidth) {
        return (screenWidth - WIDTH) / 2;
    }

    /** Picks a reason, writes a note and ticks "Ayrıca engelle" as the player would (the scenario driver). */
    public void fillForDriver(String reasonKey, String noteText, boolean alsoBlock) {
        int i = Protocol.REPORT_REASONS.indexOf(reasonKey);
        if (i >= 0) {
            reasons.pick(i);
        }
        if (noteText != null) {
            note.setValue(noteText);
        }
        if (alsoBlock != block.checked()) {
            block.toggle();
        }
    }

    /** "Gönder" as a click would (the scenario driver); false while it waits for a reason. */
    public boolean sendForDriver() {
        if (!send.active) {
            return false;
        }
        sendReport();
        return true;
    }

    private void picked(String value) {
        reason = value;
        send.active = true;
    }

    private void sendReport() {
        if (reason == null) {
            return;
        }
        OverlayClient overlay = OverlayClient.get();
        if (overlay != null) {
            overlay.report(friendId, reason, note.getValue(), block.checked());
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
        if (PanelInput.pointerSet()) {
            // The scenario driver points somewhere (never the player's own pointer, never the OS's).
            mouseX = PanelInput.pointerX();
            mouseY = PanelInput.pointerY();
        }
        long now = System.nanoTime() / 1_000_000L;
        float t = Math.min(1f, (now - openedAt) / 240f);
        float rise = panel.reducedMotion() ? 0f : (1f - Ease.OUT_EXPO.at(t)) * 8f;
        Draw.pushAlpha(Ease.OUT.at(Math.min(1f, (now - openedAt) / 160f)));
        g.pose().pushMatrix();
        g.pose().translate(0f, rise);
        int my = mouseY - (int) rise;
        Draw.frame(g, Frames.DIALOG, cardX, cardY, WIDTH, cardH);
        float x = cardX + PAD;
        float y = cardY + PAD;
        Draw.text(g, Fonts.display(), title, x, Fonts.centeredY(Fonts.Face.DISPLAY, y + 7), Theme.BONE_50);
        float nx = cardX + WIDTH - PAD - Fonts.strong().width(nameLine) - TagChip.room(chip);
        if (chip != null) {
            chip.draw(g, nx, y + 7);
        }
        Draw.text(g, Fonts.strong(), nameLine, nx + TagChip.room(chip), Fonts.centeredY(Fonts.Face.STRONG, y + 7),
            Theme.BONE_100);
        y += 14 + 3;
        for (FormattedCharSequence line : privacy) {
            Draw.text(g, Fonts.small(), line, x, Fonts.centeredY(Fonts.Face.SMALL, y + 4.5f), Theme.ASH_300);
            y += 9;
        }
        reasons.extractRenderState(g, mouseX, my, partialTick);
        // The note: the field's frame here, the game's text box inside it.
        boolean focused = note.isFocused();
        boolean lit = mouseX >= x && mouseX < x + WIDTH - 2 * PAD && my >= noteY && my < noteY + FIELD_H;
        Draw.frame(g, focused ? Frames.FIELD_FOCUS : lit ? Frames.FIELD_HOVER : Frames.FIELD, x, noteY,
            WIDTH - 2 * PAD, FIELD_H);
        note.extractRenderState(g, mouseX, my, partialTick);
        float hy = noteY + FIELD_H + 2;
        Font small = Fonts.small();
        Draw.text(g, small, help, x, Fonts.centeredY(Fonts.Face.SMALL, hy + 4.5f), Theme.ASH_400);
        int length = note.getValue().codePointCount(0, note.getValue().length());
        if (length > Protocol.MAX_REPORT_NOTE_CHARS - 40) {
            String count = length + "/" + Protocol.MAX_REPORT_NOTE_CHARS;
            Draw.text(g, small, count, cardX + WIDTH - PAD - small.width(count), Fonts.centeredY(Fonts.Face.SMALL,
                hy + 4.5f), length >= Protocol.MAX_REPORT_NOTE_CHARS ? Theme.FLARE_300 : Theme.ASH_400);
        }
        block.extractRenderState(g, mouseX, my, partialTick);
        cancel.extractRenderState(g, mouseX, my, partialTick);
        send.extractRenderState(g, mouseX, my, partialTick);
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    @Override
    public Component getNarrationMessage() {
        return Component.empty().append(getTitle()).append(". ").append(Component.literal(friendName)).append(". ")
            .append(Texts.tr("report.private"));
    }

    /**
     * The player's reasons as one radio group (the launcher's): rows of a ring and the words, the picked one gold;
     * arrows move the pick, a click or Space picks.
     */
    static final class ReasonList extends PanelWidget {
        /** How far a row's fill reaches past the radio on each side. */
        private static final int BLEED = 4;
        private final java.util.function.Consumer<String> onPick;
        private final List<FormattedCharSequence> labels = new ArrayList<>();
        private final List<String> words = new ArrayList<>();
        private int picked = -1;
        /** The row the keys are on (before anything is picked). */
        private int cursor;
        private final Tween[] lit;

        ReasonList(java.util.function.Consumer<String> onPick) {
            super(0, 0, 100, 100, Texts.tr("report.reasons"));
            this.onPick = onPick;
            lit = new Tween[Protocol.REPORT_REASONS.size()];
            for (int i = 0; i < lit.length; i++) {
                lit[i] = new Tween(0f);
            }
            for (String reason : Protocol.REPORT_REASONS) {
                String text = Texts.str("report.reason." + reason);
                words.add(text);
                labels.add(Texts.visual(text));
            }
        }

        private int rowAt(double my) {
            int i = (int) Math.floor((my - getY()) / (REASON_H + REASON_GAP));
            return i >= 0 && i < labels.size() && my - getY() - i * (REASON_H + REASON_GAP) < REASON_H ? i : -1;
        }

        private void pick(int i) {
            if (i < 0 || i >= labels.size()) {
                return;
            }
            picked = i;
            cursor = i;
            pressed();
            onPick.accept(Protocol.REPORT_REASONS.get(i));
        }

        @Override
        public void onClick(MouseButtonEvent event, boolean doubleClick) {
            PanelInput.usedMouse();
            pick(rowAt(event.y()));
        }

        @Override
        public boolean keyPressed(KeyEvent event) {
            if (event.isUp() || event.isDown()) {
                PanelInput.usedKeyboard();
                int from = picked >= 0 ? picked : cursor;
                pick(Math.floorMod(from + (event.isUp() ? -1 : 1), labels.size()));
                return true;
            }
            if (event.isSelection() || event.isConfirmation()) {
                PanelInput.usedKeyboard();
                pick(picked >= 0 ? picked : cursor);
                return true;
            }
            return false;
        }

        @Override
        protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
            Font ui = Fonts.ui();
            int under = isHovered() ? rowAt(mouseY) : -1;
            for (int i = 0; i < labels.size(); i++) {
                float y = getY() + i * (REASON_H + REASON_GAP);
                lit[i].animateTo(i == under ? 1f : 0f, now, HOVER_MS, Ease.OUT);
                boolean on = i == picked;
                // The radio stands on the dialog's left edge with the note and the tick box; a row's fill reaches past it.
                float fx = getX() - BLEED;
                float fw = getWidth() + 2 * BLEED;
                if (on) {
                    Draw.frame(g, Frames.CHOICE_PICKED, fx, y, fw, REASON_H);
                } else {
                    float l = lit[i].value(now);
                    if (l > 0.01f) {
                        Draw.pushAlpha(l);
                        Draw.frame(g, Frames.ROW_HOVER, fx, y, fw, REASON_H, 0x99FFFFFF);
                        Draw.popAlpha();
                    }
                }
                if (keyboardFocus() && i == (picked >= 0 ? picked : cursor)) {
                    Draw.frame(g, Frames.FOCUS_ROW, fx, y, fw, REASON_H);
                }
                Draw.icon(g, on ? Icon.RADIO_ON : Icon.RADIO_OFF, 10, getX(), y + (REASON_H - 10) / 2f,
                    on ? Theme.GOLD_400 : Theme.ASH_400);
                Draw.text(g, ui, labels.get(i), getX() + 14, Fonts.centeredY(Fonts.Face.UI, y + REASON_H / 2f),
                    on ? Theme.BONE_50 : Theme.BONE_100);
            }
        }

        @Override
        protected void updateWidgetNarration(NarrationElementOutput output) {
            output.add(NarratedElementType.TITLE, getMessage());
            int i = picked >= 0 ? picked : cursor;
            output.add(NarratedElementType.HINT, i == picked ? Texts.tr("report.narration.picked", words.get(i))
                : Component.literal(words.get(i)));
        }
    }

    /** "Ayrıca engelle": a small chamfered box, gold with a tick once on; a click or Space turns it. */
    static final class CheckRow extends PanelWidget {
        private boolean checked;
        private final String label;

        CheckRow(Component label) {
            super(0, 0, 100, CHECK_H, label);
            this.label = label.getString();
        }

        int preferredWidth() {
            return 10 + 6 + Math.round(Fonts.ui().width(label)) + 2;
        }

        boolean checked() {
            return checked;
        }

        void toggle() {
            checked = !checked;
            pressed();
        }

        @Override
        public void onClick(MouseButtonEvent event, boolean doubleClick) {
            PanelInput.usedMouse();
            checked = !checked;
            pressed();
        }

        @Override
        public boolean keyPressed(KeyEvent event) {
            if (event.isSelection() || event.isConfirmation()) {
                PanelInput.usedKeyboard();
                checked = !checked;
                pressed();
                return true;
            }
            return false;
        }

        @Override
        protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
            float x = getX();
            float y = getY() + (CHECK_H - 10) / 2f;
            Draw.frame(g, checked ? Frames.CHECK_ON : Frames.CHECK_OFF, x, y, 10, 10);
            if (checked) {
                Draw.icon(g, Icon.CHECK, 8, x + 1, y + 1, Theme.GOLD_INK);
            }
            if (keyboardFocus()) {
                Draw.frame(g, Frames.FOCUS_10, x, y, 10, 10);
            }
            float h = hover.value(now);
            Draw.text(g, Fonts.ui(), label, x + 16, Fonts.centeredY(Fonts.Face.UI, getY() + CHECK_H / 2f),
                Theme.mix(Theme.BONE_100, Theme.BONE_50, h));
        }

        @Override
        protected void updateWidgetNarration(NarrationElementOutput output) {
            output.add(NarratedElementType.TITLE, Texts.tr(checked ? "report.narration.on" : "report.narration.off",
                label));
        }
    }
}
