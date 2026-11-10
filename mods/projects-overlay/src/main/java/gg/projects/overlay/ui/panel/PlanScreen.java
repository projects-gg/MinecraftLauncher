package gg.projects.overlay.ui.panel;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.PlanWhen;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * A crew's plans over the panel (OVL-SOC2; the launcher's {@code crew-plans-view.tsx}): each plan not over yet with its
 * time ("Bugün 21:00", "Başladı"), who planned it, how many come, its note, and the player's answer as three quiet
 * buttons ("Geliyorum", "Belki", "Gelemem"; the chosen one on the quiet plate, never gold); "Plan yap" while the player
 * may make one. "Plan yap" is the form: the day as chips (today while a quarter of it is left and the six after), the
 * time in quarter hours with ‹ and ›, a note of at most 80 characters while the crew's plans take one, "Vazgeç" and the
 * one gold "Planla". Esc goes back (from the form to the plans when they were open, else to the panel).
 */
public final class PlanScreen extends Screen {
    /** Wider than the other dialogs: seven day chips keep "Bugün" and "Yarın" whole. */
    private static final int WIDTH = 264;
    private static final int PAD = 12;
    private static final int FIELD_H = 18;
    private static final int NOTE_CHARS = 80;
    private static final String[] ANSWERS = {"yes", "maybe", "no"};

    private final OverlayScreen panel;
    private final OverlayClient overlay;
    private final OverlayState state;
    private final String crew;
    private final String crewName;
    /** The form ("Plan yap") rather than the plans. */
    private boolean making;
    /** Opened straight on the form (Esc then closes). */
    private final boolean formOnly;
    private long openedAt;
    /** What the plans showed when laid out (the crew's plans and which answers were on their way). */
    private String shown = "";
    private int cardX;
    private int cardY;
    private int cardH;
    private String title = "";
    private FormattedCharSequence subtitle = FormattedCharSequence.EMPTY;
    private final List<PlanBlock> blocks = new ArrayList<>();
    private final List<AbstractWidgetRef> widgets = new ArrayList<>();
    private ActionButton close;
    private ActionButton make;
    // The form.
    private List<PlanWhen.Day> days = List.of();
    private int day;
    private long at;
    private ComposerField note;
    private ActionButton submit;
    private ActionButton cancel;
    private IconButton earlier;
    private IconButton later;
    private final List<ActionButton> dayChips = new ArrayList<>();
    private int dayY;
    private int timeY;
    private int noteY;

    /** A plan as laid out: its lines and its three answer buttons. */
    private record PlanBlock(OverlayState.Plan plan, FormattedCharSequence when, boolean live,
        FormattedCharSequence by, FormattedCharSequence counts, List<FormattedCharSequence> note, int y, int h) {
    }

    /** A widget drawn by this screen (they are added in order, drawn in order). */
    private record AbstractWidgetRef(net.minecraft.client.gui.components.AbstractWidget widget) {
    }

    public PlanScreen(OverlayScreen panel, OverlayClient overlay, String crew, String crewName, boolean make) {
        super(Texts.tr(make ? "plan.make" : "plan.title"));
        this.panel = panel;
        this.overlay = overlay;
        this.state = overlay.state();
        this.crew = crew;
        this.crewName = crewName == null ? "" : crewName;
        this.making = make;
        this.formOnly = make;
    }

    private static long wall() {
        return System.currentTimeMillis();
    }

    /** Each answer button's plan and answer ({@code p1:yes}), for the keys after a new layout. */
    private final java.util.Map<net.minecraft.client.gui.components.events.GuiEventListener, String> focusKeys =
        new java.util.HashMap<>();

    @Override
    protected void init() {
        widgets.clear();
        focusKeys.clear();
        blocks.clear();
        shown = plansKey();
        if (making) {
            initForm();
        } else {
            initPlans();
        }
        if (openedAt == 0) {
            openedAt = System.nanoTime() / 1_000_000L;
        }
    }

    // ---- The plans --------------------------------------------------------------------------------------------

    private void initPlans() {
        int textW = WIDTH - 2 * PAD;
        long now = wall();
        title = Texts.fit(Fonts.display(), Texts.str("plan.title"), textW - 17);
        subtitle = Texts.visual(Texts.fit(Fonts.small(), crewName, textW));
        List<OverlayState.Plan> plans = state.plans(crew, now);
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        int y = PAD + 14 + 2 + 9 + 8;
        int bw = (textW - 8) / 3;
        List<ActionButton> answerButtons = new ArrayList<>();
        // The card never grows past the screen: the plans that do not fit are counted in the subtitle.
        int room = height - 8 - 10 - ActionButton.HEIGHT - PAD;
        int left = 0;
        for (OverlayState.Plan plan : plans) {
            if (!blocks.isEmpty() && y + 10 + 9 + 9 + 6 + ActionButton.HEIGHT > room) {
                left++;
                continue;
            }
            boolean live = plan.start() <= now;
            String when = live ? Texts.str("plan.live") + " · " + PlanWhen.of(plan.at(), now, false)
                : PlanWhen.of(plan.at(), now, false);
            String by = plan.mine() ? Texts.str("plan.by_me")
                : Texts.str("plan.by", plan.by().isEmpty() ? Texts.str("toast.crewmate") : plan.by());
            List<String> parts = new ArrayList<>(2);
            if (plan.yes() > 0) {
                parts.add(Texts.str("plan.coming", plan.yes()));
            }
            if (plan.maybe() > 0) {
                parts.add(Texts.str("plan.maybe_count", plan.maybe()));
            }
            String counts = parts.isEmpty() ? Texts.str("plan.nobody") : String.join(", ", parts);
            List<FormattedCharSequence> noteLines = plan.note() == null || state.streamer() ? List.of()
                : Texts.wrap(small, "\"" + plan.note() + "\"", textW, 2);
            if (!blocks.isEmpty() && y + 10 + 9 + 9 + noteLines.size() * 9 + 6 + ActionButton.HEIGHT > room) {
                noteLines = List.of();
            }
            int h = 10 + 9 + 9 + noteLines.size() * 9 + 6 + ActionButton.HEIGHT;
            blocks.add(new PlanBlock(plan, Texts.visual(Texts.fit(strong, when, textW)), live,
                Texts.visual(Texts.fit(small, by, textW)), Texts.visual(Texts.fit(small, counts, textW)), noteLines,
                y, h));
            boolean waiting = state.pressing("plan-answer", plan.id());
            for (int i = 0; i < ANSWERS.length; i++) {
                String answer = ANSWERS[i];
                ActionButton b = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("plan." + answer), null,
                    () -> overlay.answerPlan(plan.id(), answer));
                b.setChosen(answer.equals(plan.my()));
                b.active = !waiting && state.online();
                answerButtons.add(b);
                focusKeys.put(b, plan.id() + ":" + answer);
                PanelWidget.place(b, 0, y + h - ActionButton.HEIGHT, bw, ActionButton.HEIGHT);
            }
            y += h + 10;
        }
        if (left > 0) {
            subtitle = Texts.visual(Texts.fit(Fonts.small(), crewName + " · " + Texts.str("plan.more", left), textW));
        }
        cardH = y + ActionButton.HEIGHT + PAD;
        cardX = (width - WIDTH) / 2;
        cardY = Math.max(4, (height - cardH) / 2);
        // The answers now that the card's place is known.
        for (int i = 0; i < answerButtons.size(); i++) {
            ActionButton b = answerButtons.get(i);
            PanelWidget.place(b, cardX + PAD + (i % 3) * (bw + 4), cardY + b.getY(), bw, ActionButton.HEIGHT);
            add(b);
        }
        if (close == null) {
            close = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("add.close"), null, this::onClose);
        }
        int cw = close.preferredWidth();
        int by = cardY + cardH - PAD - ActionButton.HEIGHT;
        PanelWidget.place(close, cardX + WIDTH - PAD - cw, by, cw, ActionButton.HEIGHT);
        make = null;
        if (state.mayPlan(crew, now)) {
            make = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("plan.make"), Icon.PLUS, this::openForm);
            int mw = make.preferredWidth();
            PanelWidget.place(make, cardX + WIDTH - PAD - cw - 6 - mw, by, mw, ActionButton.HEIGHT);
            add(make);
        }
        add(close);
        if (!answerButtons.isEmpty()) {
            setInitialFocus(answerButtons.get(0));
        } else {
            setInitialFocus(make != null ? make : close);
        }
    }

    private void openForm() {
        making = true;
        rebuildWidgets();
    }

    /**
     * Presses "Plan yap" under the plans and types {@code note} (null: none) into the form's note, as the player would
     * (the scenario driver); false when there is no "Plan yap".
     */
    public boolean makeForDriver(String note) {
        if (making || make == null) {
            return false;
        }
        openForm();
        if (note != null && this.note != null) {
            this.note.setValue(note);
        }
        return true;
    }

    // ---- "Plan yap" -------------------------------------------------------------------------------------------

    private void initForm() {
        int textW = WIDTH - 2 * PAD;
        long now = wall();
        title = Texts.fit(Fonts.display(), Texts.str("plan.make"), textW - 17);
        subtitle = Texts.visual(Texts.fit(Fonts.small(), Texts.str("plan.form.crew", crewName), textW));
        if (days.isEmpty()) {
            days = PlanWhen.days(now);
            at = PlanWhen.defaultTime(now);
            day = 0;
            for (int i = 0; i < days.size(); i++) {
                long next = i + 1 < days.size() ? days.get(i + 1).start() : Long.MAX_VALUE;
                if (at >= days.get(i).start() && at < next) {
                    day = i;
                }
            }
            if (!days.isEmpty()) {
                at = PlanWhen.on(days.get(day), at, now);
            }
        }
        int y = PAD + 14 + 2 + 9 + 10;
        // The day: "Gün" over its chips.
        dayY = y;
        y += 9 + 3;
        dayChips.clear();
        for (int i = 0; i < days.size(); i++) {
            int index = i;
            ActionButton chip = new ActionButton(ActionButton.Style.SECONDARY,
                Component.literal(PlanWhen.dayChip(days.get(i))), null, () -> pickDay(index));
            chip.setChosen(i == day);
            // "Tomorrow" and five weekdays share a 240-pixel row: less room around the words than a button's.
            chip.setPadding(4);
            dayChips.add(chip);
        }
        // Each chip as wide as its words ("Yarın" and "Tomorrow" need more than "Cmt"), the room left shared out;
        // when even that does not fit, the room in proportion and the words get smaller (LABEL-FIT).
        int[] chipW = chipWidths(dayChips, textW, 3);
        for (int i = 0; i < dayChips.size(); i++) {
            PanelWidget.place(dayChips.get(i), 0, y, chipW[i], ActionButton.HEIGHT);
        }
        y += ActionButton.HEIGHT + 8;
        // The time: "Saat", ‹ 21:00 ›.
        timeY = y;
        y += 9 + 3;
        if (earlier == null) {
            earlier = new IconButton(Icon.BACK, Texts.tr("plan.form.earlier"), () -> step(-1));
            later = new IconButton(Icon.NEXT, Texts.tr("plan.form.later"), () -> step(1));
        }
        int timeRow = y;
        y += IconButton.SIZE + 8;
        // The note, while the crew's plans take one.
        noteY = -1;
        if (state.planNotes()) {
            noteY = y;
            if (note == null) {
                note = new ComposerField(Texts.tr("plan.form.note"));
                note.setMaxLength(NOTE_CHARS);
                note.setPlaceholder(Texts.tr("plan.form.note"));
            }
            y += FIELD_H + 8;
        }
        cardH = y + ActionButton.HEIGHT + PAD;
        cardX = (width - WIDTH) / 2;
        cardY = Math.max(4, (height - cardH) / 2);
        int chipX = cardX + PAD;
        for (int i = 0; i < dayChips.size(); i++) {
            ActionButton chip = dayChips.get(i);
            PanelWidget.place(chip, chipX, cardY + chip.getY(), chipW[i], ActionButton.HEIGHT);
            chipX += chipW[i] + 3;
            add(chip);
        }
        int tw = 60;
        int tx = cardX + PAD;
        PanelWidget.place(earlier, tx, cardY + timeRow, IconButton.SIZE, IconButton.SIZE);
        PanelWidget.place(later, tx + IconButton.SIZE + tw, cardY + timeRow, IconButton.SIZE, IconButton.SIZE);
        add(earlier);
        add(later);
        if (note != null && noteY >= 0) {
            PanelWidget.place(note, cardX + PAD + 7, cardY + noteY + (FIELD_H - 12) / 2, textW - 14, 12);
            add(note);
        }
        if (submit == null) {
            submit = new ActionButton(ActionButton.Style.PRIMARY, Texts.tr("plan.form.submit"), null, this::submit);
            cancel = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("plan.form.cancel"), null,
                this::back);
        }
        int bw = Math.max(submit.preferredWidth(), cancel.preferredWidth());
        int by = cardY + cardH - PAD - ActionButton.HEIGHT;
        PanelWidget.place(submit, cardX + WIDTH - PAD - bw, by, bw, ActionButton.HEIGHT);
        PanelWidget.place(cancel, cardX + WIDTH - PAD - 2 * bw - 6, by, bw, ActionButton.HEIGHT);
        submit.active = !days.isEmpty() && state.mayPlan(crew, now) && state.online();
        add(cancel);
        add(submit);
        setInitialFocus(dayChips.isEmpty() ? cancel : dayChips.get(day));
    }

    /** Widths for the day chips across {@code room} with {@code gap} between them ({@link PlanWhen#chipWidths}). */
    private static int[] chipWidths(List<ActionButton> chips, int room, int gap) {
        int[] wanted = new int[chips.size()];
        for (int i = 0; i < wanted.length; i++) {
            wanted[i] = chips.get(i).preferredWidth();
        }
        return PlanWhen.chipWidths(wanted, room, gap);
    }

    private void pickDay(int index) {
        long now = wall();
        day = index;
        at = PlanWhen.on(days.get(index), at, now);
        rebuildWidgets();
    }

    /** ‹ and ›: a quarter of an hour earlier or later within the day's offered times. */
    private void step(int direction) {
        if (days.isEmpty()) {
            return;
        }
        List<Long> times = PlanWhen.times(days.get(day), wall());
        int i = times.indexOf(at);
        if (i < 0) {
            at = PlanWhen.on(days.get(day), at, wall());
            return;
        }
        at = times.get(Math.max(0, Math.min(times.size() - 1, i + direction)));
    }

    private void submit() {
        if (!submit.active || days.isEmpty()) {
            return;
        }
        long now = wall();
        // The time again (a minute may have passed since the form opened).
        long fresh = PlanWhen.on(days.get(day), at, now);
        if (fresh < 0) {
            return;
        }
        at = fresh;
        String text = note == null ? "" : note.getValue().trim();
        overlay.makePlan(crew, at, text, PlanWhen.of(at * 1000, now, false));
        Compat.setScreen(minecraft, panel);
    }

    /** "Vazgeç" and Esc on the form: back to the plans when they were open, else to the panel. */
    private void back() {
        if (formOnly) {
            onClose();
            return;
        }
        making = false;
        rebuildWidgets();
    }

    // ---- The screen ------------------------------------------------------------------------------------------

    private void add(net.minecraft.client.gui.components.AbstractWidget w) {
        widgets.add(new AbstractWidgetRef(w));
        addWidget(w);
    }

    @Override
    public void tick() {
        // The plans changed (an answer came back, a plan went): laid out again, the keys where they were; the form
        // keeps its picks.
        if (!making && !plansKey().equals(shown)) {
            // The keys stay on the same plan's same answer (by its plan, never by its place).
            String focused = focusKeys.get(getFocused());
            rebuildWidgets();
            if (focused != null) {
                focusKeys.forEach((widget, key) -> {
                    if (key.equals(focused)) {
                        setFocused(widget);
                    }
                });
            }
        }
    }

    /** The crew's plans as they show now, with the answers on their way (a change lays the plans out again). */
    private String plansKey() {
        StringBuilder key = new StringBuilder();
        for (OverlayState.Plan plan : state.plans(crew, wall())) {
            key.append(plan).append(state.pressing("plan-answer", plan.id())).append(plan.start() <= wall());
        }
        return key.append(state.mayPlan(crew, wall())).append(state.online()).toString();
    }

    @Override
    public void onClose() {
        Compat.setScreen(minecraft, panel);
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        PanelInput.usedKeyboard();
        if (making && event.key() == com.mojang.blaze3d.platform.InputConstants.KEY_ESCAPE && !formOnly) {
            back();
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
        Draw.icon(g, making ? Icon.PLUS : Icon.CLOCK, 12, x, y + 1, Theme.ASH_300);
        Draw.text(g, Fonts.display(), title, x + 17, Fonts.centeredY(Fonts.Face.DISPLAY, y + 7), Theme.BONE_50);
        Font small = Fonts.small();
        Draw.text(g, small, subtitle, x, Fonts.centeredY(Fonts.Face.SMALL, y + 14 + 2 + 4.5f), Theme.ASH_400);
        if (making) {
            drawForm(g, x);
        } else {
            drawPlans(g, x);
        }
        for (AbstractWidgetRef ref : widgets) {
            if (ref.widget() == note) {
                boolean focused = note.isFocused();
                float fx = cardX + PAD;
                float fy = cardY + noteY;
                boolean lit = mouseX >= fx && mouseX < fx + WIDTH - 2 * PAD && my >= fy && my < fy + FIELD_H;
                Draw.frame(g, focused ? Frames.FIELD_FOCUS : lit ? Frames.FIELD_HOVER : Frames.FIELD, fx, fy,
                    WIDTH - 2 * PAD, FIELD_H);
            }
            ref.widget().extractRenderState(g, mouseX, my, partialTick);
        }
        g.pose().popMatrix();
        Draw.popAlpha();
    }

    private void drawPlans(GuiGraphicsExtractor g, float x) {
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        if (blocks.isEmpty()) {
            Draw.text(g, small, Texts.str("plan.none"), x, Fonts.centeredY(Fonts.Face.SMALL,
                cardY + PAD + 14 + 2 + 9 + 8 + 4.5f), Theme.ASH_400);
            return;
        }
        for (PlanBlock b : blocks) {
            float y = cardY + b.y();
            // A plan under way says so in jade (a live thing, never gold).
            Draw.text(g, strong, b.when(), x, Fonts.centeredY(Fonts.Face.STRONG, y + 5), b.live() ? Theme.JADE_400
                : Theme.BONE_100);
            y += 10;
            Draw.text(g, small, b.by(), x, Fonts.centeredY(Fonts.Face.SMALL, y + 4.5f), Theme.ASH_300);
            y += 9;
            Draw.text(g, small, b.counts(), x, Fonts.centeredY(Fonts.Face.SMALL, y + 4.5f), Theme.ASH_400);
            y += 9;
            for (FormattedCharSequence line : b.note()) {
                Draw.text(g, small, line, x, Fonts.centeredY(Fonts.Face.SMALL, y + 4.5f), Theme.BONE_100);
                y += 9;
            }
        }
    }

    private void drawForm(GuiGraphicsExtractor g, float x) {
        Font small = Fonts.small();
        Draw.text(g, small, Texts.str("plan.form.day"), x, Fonts.centeredY(Fonts.Face.SMALL, cardY + dayY + 4.5f),
            Theme.ASH_400);
        Draw.text(g, small, Texts.str("plan.form.time"), x, Fonts.centeredY(Fonts.Face.SMALL,
            cardY + timeY + 4.5f), Theme.ASH_400);
        String time = PlanWhen.clock(at);
        Font strong = Fonts.strong();
        float tx = cardX + PAD + IconButton.SIZE + (60 - strong.width(time)) / 2f;
        Draw.text(g, strong, time, tx, Fonts.centeredY(Fonts.Face.STRONG, cardY + timeY + 9 + 3
            + IconButton.SIZE / 2f), Theme.BONE_50);
        if (note != null && noteY >= 0) {
            int length = note.getValue().codePointCount(0, note.getValue().length());
            if (length > NOTE_CHARS - 20) {
                String count = length + "/" + NOTE_CHARS;
                Draw.text(g, small, count, cardX + WIDTH - PAD - small.width(count),
                    Fonts.centeredY(Fonts.Face.SMALL, cardY + noteY + FIELD_H + 4.5f),
                    length >= NOTE_CHARS ? Theme.FLARE_300 : Theme.ASH_400);
            }
        }
    }

    @Override
    public Component getNarrationMessage() {
        return Component.empty().append(getTitle()).append(". ").append(Component.literal(crewName));
    }
}
