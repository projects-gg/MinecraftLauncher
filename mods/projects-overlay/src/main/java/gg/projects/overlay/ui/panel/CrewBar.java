package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.cursor.CursorTypes;
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
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * Crew chat's own line (OVL-SOC2; the owner: "sürekli launcherı açmak gerekmesin sosyal işler için"): "Ekibi çağır" while
 * the player may call this crew and no call is open, and the crew's next plan ("Bugün 21:00 · 3", "+1" for more) or
 * "Plan yap" while there is none and the player may make one. Quiet pills in the conversation chips' language (no gold:
 * none of them is the page's one action); a press calls the crew, or opens the plans over the panel (their answers,
 * "Plan yap"). Keys as the chips: ← and → pick a pill, Enter or Space presses it. Nothing runs while it is hidden, and
 * it lays out only when the state or the crew changed.
 */
public final class CrewBar extends PanelWidget {
    public static final int HEIGHT = ChipRow.HEIGHT;
    private static final int GAP = 4;
    private static final int PAD = 6;
    private static final int SIGN = 8;
    private static final int SIGN_GAP = 3;

    /** What a pill does. */
    public interface Actions {
        /** "Ekibi çağır". */
        void call(String crew);

        /** The crew's plans over the panel ({@code make}: straight to "Plan yap"). */
        void plans(String crew, boolean make);
    }

    private enum Kind { CALL, PLAN, MAKE }

    private record Pill(Kind kind, Icon icon, FormattedCharSequence label, int labelWidth, String extra,
        int extraWidth, int width, boolean waiting, Component narration) {
    }

    private final Actions actions;
    private final List<Pill> pills = new ArrayList<>();
    private String crew;
    private int focused;
    private int revision = -1;
    private long laidOutMinute = -1;

    public CrewBar(Actions actions) {
        super(0, 0, 100, HEIGHT, Texts.tr("crew.bar.title"));
        this.actions = actions;
    }

    /**
     * What it shows for {@code crew} now; false when it has nothing to show (the panel gives it no room then). Lays out
     * again only when the state, the crew or the minute changed (a plan's "Bugün" turns into "Başladı").
     */
    public boolean show(OverlayState state, String crew, long wall) {
        long minute = wall / 60_000L;
        if (crew.equals(this.crew) && state.revision() == revision && minute == laidOutMinute) {
            return !pills.isEmpty();
        }
        if (!crew.equals(this.crew)) {
            focused = 0;
        }
        this.crew = crew;
        revision = state.revision();
        laidOutMinute = minute;
        pills.clear();
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        if (state.mayCall(crew) && state.activeCall(wall) == null) {
            String label = Texts.str("crew.call");
            boolean waiting = state.pressing("crew-call", crew);
            pills.add(pill(Kind.CALL, Icon.CALL, label, null, waiting, Texts.tr("crew.call.narration"), ui, small));
        }
        List<OverlayState.Plan> plans = state.plansOn() ? state.plans(crew, wall) : List.of();
        if (!plans.isEmpty()) {
            OverlayState.Plan next = plans.get(0);
            String when = next.start() <= wall ? Texts.str("plan.live") : PlanWhen.of(next.at(), wall, false);
            // How many come, in words ("Bugün 21:00 3 kişi"): two bare numbers side by side read as one (the
            // screenshots' second look). The other plans are on the plans screen the pill opens.
            String extra = next.yes() > 0 ? Texts.str("plan.pill.coming", next.yes()) : null;
            pills.add(pill(Kind.PLAN, Icon.CLOCK, when, extra, false, Texts.tr("plan.pill.narration", when), ui,
                small));
        } else if (state.plansOn() && state.mayPlan(crew, wall)) {
            String label = Texts.str("plan.make");
            pills.add(pill(Kind.MAKE, Icon.PLUS, label, null, false, Texts.tr("plan.make"), ui, small));
        }
        focused = Math.max(0, Math.min(focused, pills.size() - 1));
        return !pills.isEmpty();
    }

    private Pill pill(Kind kind, Icon icon, String label, String extra, boolean waiting, Component narration, Font ui,
        Font small) {
        int room = Math.max(30, getWidth() / 2 - PAD * 2 - SIGN - SIGN_GAP);
        String fitted = Texts.fit(ui, label, room);
        int labelWidth = ui.width(fitted);
        int extraWidth = extra == null ? 0 : small.width(extra);
        int width = PAD + SIGN + SIGN_GAP + labelWidth + (extra == null ? 0 : 5 + extraWidth) + PAD;
        return new Pill(kind, icon, Texts.visual(fitted), labelWidth, extra, extraWidth, width, waiting, narration);
    }

    /** A new minute since it laid out (a plan's "Bugün 21:00" turns into "Başladı", one over goes). */
    public boolean stale(long wall) {
        return wall / 60_000L != laidOutMinute;
    }

    /** The crew it shows, or null. */
    public String crew() {
        return crew;
    }

    private float pillX(int index) {
        float x = getX();
        for (int i = 0; i < index; i++) {
            x += pills.get(i).width + GAP;
        }
        return x;
    }

    private int pillAt(double mx) {
        float x = getX();
        for (int i = 0; i < pills.size(); i++) {
            Pill p = pills.get(i);
            if (mx >= x && mx < x + p.width) {
                return i;
            }
            x += p.width + GAP;
        }
        return -1;
    }

    private void press(int index) {
        Pill p = pills.get(index);
        focused = index;
        pressed();
        switch (p.kind) {
            case CALL -> {
                if (!p.waiting) {
                    actions.call(crew);
                }
            }
            case PLAN -> actions.plans(crew, false);
            case MAKE -> actions.plans(crew, true);
        }
    }

    /** Presses "Ekibi çağır" ({@code call}) or the plan pill as a click would (the scenario driver); false without it. */
    public boolean pressForDriver(boolean call) {
        for (int i = 0; i < pills.size(); i++) {
            if ((pills.get(i).kind == Kind.CALL) == call) {
                press(i);
                return true;
            }
        }
        return false;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        int i = pillAt(event.x());
        if (i >= 0 && !doubleClick) {
            press(i);
        }
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (!visible || pills.isEmpty()) {
            return false;
        }
        if (event.isLeft() && focused > 0) {
            PanelInput.usedKeyboard();
            focused--;
            return true;
        }
        if (event.isRight() && focused < pills.size() - 1) {
            PanelInput.usedKeyboard();
            focused++;
            return true;
        }
        if (event.isSelection() || event.isConfirmation()) {
            PanelInput.usedKeyboard();
            press(focused);
            return true;
        }
        return false;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        int y0 = getY();
        Font ui = Fonts.ui();
        Font small = Fonts.small();
        int lit = isHovered() ? pillAt(mouseX) : -1;
        float textY = Fonts.centeredY(Fonts.Face.UI, y0 + HEIGHT / 2f);
        for (int i = 0; i < pills.size(); i++) {
            Pill p = pills.get(i);
            float x = pillX(i);
            if (x + p.width > getX() + getWidth() + 1) {
                break;
            }
            boolean hot = i == lit && !p.waiting;
            Draw.pushAlpha(p.waiting ? 0.45f : 1f);
            Draw.frame(g, hot ? Frames.CHIP_HOVER : Frames.CHIP, x, y0, p.width, HEIGHT);
            if (keyboardFocus() && i == focused) {
                Draw.frame(g, Frames.FOCUS_14, x, y0, p.width, HEIGHT);
            }
            if (hot) {
                g.requestCursor(CursorTypes.POINTING_HAND);
            }
            int ink = hot ? Theme.BONE_100 : Theme.ASH_300;
            float cx = x + PAD;
            Draw.icon(g, p.icon, SIGN, cx, y0 + (HEIGHT - SIGN) / 2f, hot ? Theme.BONE_100 : Theme.ASH_400);
            cx += SIGN + SIGN_GAP;
            Draw.text(g, ui, p.label, cx, textY, ink);
            cx += p.labelWidth;
            if (p.extra != null) {
                Draw.text(g, small, p.extra, cx + 5, Fonts.centeredY(Fonts.Face.SMALL, y0 + HEIGHT / 2f),
                    Theme.ASH_400);
            }
            Draw.popAlpha();
        }
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, getMessage());
        if (focused >= 0 && focused < pills.size()) {
            output.add(NarratedElementType.HINT, pills.get(focused).narration);
        }
    }
}
