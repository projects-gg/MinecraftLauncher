package gg.projects.overlay.ui.panel;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Face;
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
 * "Arkadaş ekle" and "Ekibe katıl" over the panel (OVL-SOC2; the launcher's add page and "Kodla ya da bağlantıyla
 * katıl"): one field for a friend's code, a crew's code or an invite link, "Bul", then what the code leads to (their
 * face and name, or the crew and how many are in it) with the one gold action it allows ("İstek gönder", "Kabul et",
 * "Ekibe katıl", "Katılma isteği gönder"), or why not in a quiet line. Nothing goes before "Bul": a wrong code never
 * reaches someone. Under it the player's own code with "Kopyala" (the game's clipboard). The action closes the dialog;
 * how it went is said under the friend list. Esc and "Kapat" go back to the panel.
 */
public final class AddScreen extends Screen {
    private static final int WIDTH = 236;
    private static final int PAD = 12;
    private static final int FIELD_H = 18;
    /** The found row: the face, the name and its line, the action at its right. */
    private static final int RESULT_H = 30;
    private static final int FACE = 18;
    private static final int CODE_H = 16;
    /** A link is longer than a code; nothing longer is one. */
    private static final int MAX_CODE = 100;

    private final OverlayScreen panel;
    private final OverlayClient overlay;
    private final OverlayState state;
    /** Opened as "Ekibe katıl" (its title and the field's words; any code still works). */
    private final boolean crew;
    private ComposerField field;
    private ActionButton find;
    private ActionButton close;
    private ActionButton copy;
    private ActionButton act;
    private String title = "";
    private int cardX;
    private int cardY;
    private int cardH;
    private int fieldY;
    private int resultY;
    private int codeY;
    private long openedAt;
    /** The code the last "Bul" asked about, as typed (null: none yet), when, and whether its answer never came. */
    private String asked;
    private long askedAt;
    private boolean timedOut;
    /** When "Kopyala" was pressed (its "Kopyalandı" shows a moment). */
    private long copiedAt;
    /** How long "Aranıyor…" waits for its answer. */
    private static final long FIND_WAIT_MS = 10_000;
    private int foundRevision = -1;
    // The found row as laid out.
    private Face face;
    private FormattedCharSequence resultName = FormattedCharSequence.EMPTY;
    private List<FormattedCharSequence> resultLines = List.of();
    private int resultInk = Theme.ASH_300;
    private Runnable action;
    private FormattedCharSequence codeLine = FormattedCharSequence.EMPTY;
    private String codeLabel = "";

    public AddScreen(OverlayScreen panel, OverlayClient overlay, boolean crew) {
        super(Texts.tr(crew ? "add.crew.title" : "add.friend.title"));
        this.panel = panel;
        this.overlay = overlay;
        this.state = overlay.state();
        this.crew = crew;
    }

    /** The player's own code shows (the friends page's management is on and the launcher said it). */
    private boolean showsCode() {
        // Streamer mode keeps it off the stream (the launcher's add page too).
        return !crew && state.people5() && !state.myCode().isEmpty() && !state.streamer();
    }

    @Override
    protected void init() {
        int textW = WIDTH - 2 * PAD;
        title = Texts.fit(Fonts.display(), getTitle().getString(), textW - 17);
        if (field == null) {
            field = new ComposerField(Texts.tr(crew ? "add.crew.field" : "add.friend.field"));
            field.setMaxLength(MAX_CODE);
            field.setPlaceholder(Texts.tr(crew ? "add.crew.field" : "add.friend.field"));
            find = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("add.find"), null, this::find);
            close = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("add.close"), null, this::onClose);
            copy = new ActionButton(ActionButton.Style.GHOST, Texts.tr("add.copy"), null, this::copy);
        }
        layoutResult(textW);
        cardH = PAD + 14 + 8 + FIELD_H + 6 + RESULT_H + (showsCode() ? 6 + CODE_H : 0) + 10 + ActionButton.HEIGHT
            + PAD;
        cardX = (width - WIDTH) / 2;
        cardY = Math.max(4, (height - cardH) / 2);
        int y = cardY + PAD + 14 + 8;
        fieldY = y;
        int fw = find.preferredWidth();
        PanelWidget.place(find, cardX + WIDTH - PAD - fw, y + (FIELD_H - ActionButton.HEIGHT) / 2, fw,
            ActionButton.HEIGHT);
        PanelWidget.place(field, cardX + PAD + 7, y + (FIELD_H - 12) / 2, textW - fw - 6 - 14, 12);
        y += FIELD_H + 6;
        resultY = y;
        if (act != null) {
            int aw = Math.min(act.preferredWidth(), textW / 2);
            PanelWidget.place(act, cardX + WIDTH - PAD - aw, y + (RESULT_H - ActionButton.HEIGHT) / 2, aw,
                ActionButton.HEIGHT);
        }
        y += RESULT_H;
        if (showsCode()) {
            y += 6;
            codeY = y;
            int cw = copy.preferredWidth();
            PanelWidget.place(copy, cardX + WIDTH - PAD - cw, y + (CODE_H - ActionButton.HEIGHT) / 2, cw,
                ActionButton.HEIGHT);
            codeLabel = Texts.str("add.code");
            codeLine = Texts.visual(state.myCode());
        }
        int cw = close.preferredWidth();
        PanelWidget.place(close, cardX + WIDTH - PAD - cw, cardY + cardH - PAD - ActionButton.HEIGHT, cw,
            ActionButton.HEIGHT);
        addWidget(field);
        addWidget(find);
        if (act != null) {
            addWidget(act);
        }
        if (showsCode()) {
            addWidget(copy);
        }
        addWidget(close);
        setInitialFocus(field);
        if (openedAt == 0) {
            openedAt = System.nanoTime() / 1_000_000L;
        }
    }

    /** What the found row says now: nothing yet, "Aranıyor…", a reason in flare, or a person or a crew. */
    private void layoutResult(int textW) {
        face = null;
        action = null;
        act = null;
        resultName = FormattedCharSequence.EMPTY;
        resultLines = List.of();
        resultInk = Theme.ASH_300;
        Font small = Fonts.small();
        OverlayState.Found found = state.found();
        if (asked == null) {
            if (!crew) {
                // Before the first "Bul": what a code looks like, quietly.
                resultLines = Texts.wrap(small, Texts.str("add.none.invalid"), textW, 2);
                resultInk = Theme.ASH_400;
            }
            return;
        }
        if (found == null || !found.code().equals(asked)) {
            if (timedOut) {
                resultLines = Texts.wrap(small, Texts.str("add.none.failed"), textW, 2);
                resultInk = Theme.FLARE_300;
                return;
            }
            resultLines = List.of(Texts.visual(Texts.str("add.finding")));
            return;
        }
        if (found.kind().equals("none")) {
            String key = switch (found.reason() == null ? "" : found.reason()) {
                case "invalid" -> crew ? "add.none.invalid_crew" : "add.none.invalid";
                case "not-found" -> crew ? "add.none.not_found_crew" : "add.none.not_found";
                case "rate" -> "add.none.rate";
                case "offline" -> "add.none.offline";
                default -> "add.none.failed";
            };
            resultLines = Texts.wrap(small, Texts.str(key), textW, 2);
            resultInk = Theme.FLARE_300;
            return;
        }
        boolean person = found.kind().equals("friend");
        String name = found.name().isEmpty() ? Texts.str("toast.someone") : found.name();
        face = person ? Face.of(name, found.id()) : Face.sign(Icon.CREW);
        String line;
        String button = null;
        if (person) {
            line = switch (found.relation()) {
                case "friend", "outgoing", "incoming", "blocked", "self" -> Texts.str("add.relation." + found.relation());
                default -> "";
            };
            if (found.relation().equals("none")) {
                button = "add.send";
                action = () -> overlay.addCode(found.code(), name);
            } else if (found.relation().equals("incoming") && state.people4()) {
                button = "add.accept";
                action = () -> overlay.answerRequest(found.id(), name, true);
            }
        } else {
            String members = Texts.str("add.members", found.members());
            line = switch (found.relation()) {
                case "member", "elsewhere", "full", "limit" -> Texts.str("add.relation." + found.relation());
                case "blocked" -> Texts.str("add.relation.crew_blocked");
                default -> found.requested() ? Texts.str("add.relation.requested") : members;
            };
            if (found.relation().equals("none") && !found.requested() && state.crews2()) {
                button = found.approval() ? "add.ask" : "add.join";
                action = () -> overlay.joinCrew(found.code(), name);
            }
        }
        if (button != null) {
            act = new ActionButton(ActionButton.Style.PRIMARY, Texts.tr(button), null, this::act);
        }
        int room = textW - FACE - 6 - (act == null ? 0 : Math.min(act.preferredWidth(), textW / 2) + 6);
        Font strong = Fonts.strong();
        String shown = person || found.tag().isEmpty() ? name : "[" + found.tag() + "] " + name;
        resultName = Texts.visual(Texts.fit(strong, shown, room));
        resultLines = line.isEmpty() ? List.of() : List.of(Texts.visual(Texts.fit(small, line, room)));
    }

    /** "Bul": what the typed code leads to (the answer lays the dialog out again). */
    private void find() {
        String code = field.getValue().trim();
        if (code.isEmpty()) {
            return;
        }
        asked = code;
        askedAt = System.currentTimeMillis();
        timedOut = false;
        overlay.find(code);
        foundRevision = state.foundRevision();
        rebuildWidgets();
    }

    /** Types {@code code} into the field and presses "Bul", as the player would (the scenario driver). */
    public void findForDriver(String code) {
        field.setValue(code);
        find();
    }

    /** The found row's action; how it went is said under the friend list. */
    /** "Kopyala": the code to the clipboard, "Kopyalandı" beside it for a moment. */
    private void copy() {
        overlay.copyCode();
        copiedAt = System.currentTimeMillis();
    }

    private void act() {
        Runnable run = action;
        onClose();
        if (run != null) {
            run.run();
        }
    }

    @Override
    public void tick() {
        if (state.foundRevision() != foundRevision) {
            foundRevision = state.foundRevision();
            rebuildWidgets();
            if (act != null && asked != null && field.getValue().trim().equals(asked)) {
                // The answer came for the code still in the field: the keys go to its action (Enter takes it).
                setFocused(act);
            }
        } else if (asked != null && !timedOut && System.currentTimeMillis() - askedAt > FIND_WAIT_MS
            && (state.found() == null || !state.found().code().equals(asked))) {
            // No answer: the launcher went, or the friends service. Said here, not under the list behind.
            timedOut = true;
            rebuildWidgets();
        }
    }

    @Override
    public void onClose() {
        Compat.setScreen(minecraft, panel);
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        PanelInput.usedKeyboard();
        if (getFocused() == field && event.isConfirmation()) {
            // Enter in the field: "Bul" for a new code, the action for the one found.
            if (!field.getValue().trim().equals(asked) || action == null) {
                find();
            } else {
                act();
            }
            return true;
        }
        return super.keyPressed(event);
    }

    @Override
    public boolean isPauseScreen() {
        return panel.isPauseScreen();
    }

    @Override
    public void extractBackground(GuiGraphicsExtractor g, int mouseX, int mouseY, float partialTick) {
        if (minecraft != null && minecraft.level == null) {
            // OVL-RPC: over the panel opened from the game's menus, no world behind: the menus' background first.
            super.extractBackground(g, mouseX, mouseY, partialTick);
        }
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
        Draw.icon(g, crew ? Icon.CREW : Icon.USER_PLUS, 12, x, y + 1, Theme.ASH_300);
        Draw.text(g, Fonts.display(), title, x + 17, Fonts.centeredY(Fonts.Face.DISPLAY, y + 7), Theme.BONE_50);
        // The field: its frame here, the game's text box inside it.
        int fieldW = WIDTH - 2 * PAD - find.getWidth() - 6;
        boolean focused = field.isFocused();
        boolean lit = mouseX >= x && mouseX < x + fieldW && my >= fieldY && my < fieldY + FIELD_H;
        Draw.frame(g, focused ? Frames.FIELD_FOCUS : lit ? Frames.FIELD_HOVER : Frames.FIELD, x, fieldY, fieldW,
            FIELD_H);
        field.extractRenderState(g, mouseX, my, partialTick);
        find.extractRenderState(g, mouseX, my, partialTick);
        drawResult(g, x);
        if (act != null) {
            act.extractRenderState(g, mouseX, my, partialTick);
        }
        if (showsCode()) {
            Font small = Fonts.small();
            float cy = codeY + CODE_H / 2f;
            Draw.text(g, small, codeLabel, x, Fonts.centeredY(Fonts.Face.SMALL, cy), Theme.ASH_400);
            Draw.text(g, Fonts.strong(), codeLine, x + small.width(codeLabel) + 6,
                Fonts.centeredY(Fonts.Face.STRONG, cy), Theme.BONE_100);
            if (System.currentTimeMillis() - copiedAt < 2_000) {
                String copied = Texts.str("add.copied");
                Draw.text(g, small, copied, copy.getX() - 6 - small.width(copied), Fonts.centeredY(Fonts.Face.SMALL,
                    cy), Theme.JADE_400);
            }
            copy.extractRenderState(g, mouseX, my, partialTick);
        }
        close.extractRenderState(g, mouseX, my, partialTick);
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    private void drawResult(GuiGraphicsExtractor g, float x) {
        float cy = resultY + RESULT_H / 2f;
        if (face == null) {
            float ly = cy - resultLines.size() * 9 / 2f;
            for (FormattedCharSequence line : resultLines) {
                Draw.text(g, Fonts.small(), line, x, Fonts.centeredY(Fonts.Face.SMALL, ly + 4.5f), resultInk);
                ly += 9;
            }
            return;
        }
        face.draw(g, FACE, x + FACE / 2f, cy);
        float tx = x + FACE + 6;
        boolean two = !resultLines.isEmpty();
        Draw.text(g, Fonts.strong(), resultName, tx, Fonts.centeredY(Fonts.Face.STRONG, two ? cy - 5 : cy),
            Theme.BONE_100);
        if (two) {
            Draw.text(g, Fonts.small(), resultLines.get(0), tx, Fonts.centeredY(Fonts.Face.SMALL, cy + 5),
                Theme.ASH_300);
        }
    }

    @Override
    public Component getNarrationMessage() {
        return getTitle();
    }
}
