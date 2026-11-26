package gg.projects.overlay.tweaks;

import gg.projects.overlay.tweaks.mixin.TooltipText;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.WeakHashMap;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.navigation.ScreenRectangle;
import net.minecraft.locale.Language;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;
import org.jetbrains.annotations.Nullable;

/**
 * TOOLTIP-FIT: a tooltip always shows whole and readable. The game puts a widget's tooltip below the widget, or above
 * it when it does not fit below, and never moves it back onto the screen, so a long one (a server window's button with
 * a page of text) runs off the top while there is room below. With this tweak:
 * <ul>
 * <li>a widget's tooltip that fits beside its widget nowhere at the game's width is wrapped wider, only as much as it
 * takes to fit below or above the widget ({@link TooltipPlace#wrap});
 * <li>any tooltip the game leaves partly off the screen moves the least to show whole ({@link TooltipPlace#fit});
 * <li>one larger than the whole screen is drawn smaller to fit ({@link TooltipPlace#scale}), never below half.
 * </ul>
 * The game's mixins ({@code tweaks.mixin}) call in; while the tweak is off, or after it failed once, they leave the
 * game's tooltips exactly as the game makes them.
 */
public final class TooltipFit implements ClientTweak {
    /** Its id ({@link ClientTweaks#PROPERTY}; the launcher's `pl_overlay::tweaks::TOOLTIP_FIT`). */
    static final String ID = "tooltip-fit";

    @Nullable
    private static volatile TweakGuard guard;
    /** Each widget tooltip's wrap for the screen it was last made for (the game asks every frame). Render thread. */
    private static final Map<Tooltip, Wrapped> WRAPPED = new WeakHashMap<>();
    /** The last tooltip placed (render thread), for the game test. */
    @Nullable
    private static volatile Placed last;

    @Override
    public String id() {
        return ID;
    }

    @Override
    public void install(TweakGuard guard) {
        TooltipFit.guard = guard;
    }

    /** The tweak is on and has not failed. */
    public static boolean on() {
        TweakGuard at = guard;
        return at != null && at.on();
    }

    /** Where a tooltip's text area starts and how large it is drawn (1: as it is). */
    public record Place(int x, int y, float scale) {
    }

    /** A placed tooltip: where it went, its text area as the game measured it, and the screen. */
    public record Placed(Place place, int w, int h, int screenW, int screenH) {
    }

    /** The last tooltip this tweak placed in this game, or null. For the game test. */
    @Nullable
    public static Placed last() {
        return last;
    }

    /**
     * Where a tooltip of {@code w} by {@code h} goes, given where the game put it ({@code x}, {@code y}): moved the
     * least to show whole, and drawn smaller when it is larger than the screen. Null while the tweak is off: the game's
     * place stands.
     */
    @Nullable
    public static Place place(int x, int y, int w, int h, int screenW, int screenH) {
        TweakGuard at = guard;
        if (at == null) {
            return null;
        }
        return at.call(() -> {
            float scale = TooltipPlace.scale(w, h, screenW, screenH);
            Place place;
            if (scale >= 1F) {
                place = new Place(TooltipPlace.fit(x, w, screenW), TooltipPlace.fit(y, h, screenH), 1F);
            } else {
                int scaledW = Math.round(w * scale);
                int scaledH = Math.round(h * scale);
                place = new Place(TooltipPlace.fit(x, scaledW, screenW), TooltipPlace.fit(y, scaledH, screenH), scale);
            }
            last = new Placed(place, w, h, screenW, screenH);
            return place;
        }, null);
    }

    /**
     * The lines of a widget's tooltip for this frame: the game's own ({@code lines}) while they fit beside the widget,
     * else the tooltip's text wrapped wider ({@link TooltipPlace#wrap}). A tooltip with a picture keeps the game's
     * lines.
     */
    public static List<FormattedCharSequence> lines(Tooltip tooltip, List<FormattedCharSequence> lines, Font font,
                                                    int screenW, int screenH, ScreenRectangle widget) {
        TweakGuard at = guard;
        if (at == null || lines.isEmpty() || tooltip.component().isPresent()) {
            return lines;
        }
        return at.call(() -> {
            int beside = Math.max(
                TooltipPlace.roomBelow(widget.bottom(), widget.height(), screenH),
                TooltipPlace.roomAbove(widget.top()));
            if (TooltipPlace.height(lines.size()) <= beside) {
                return lines;
            }
            Language language = Language.getInstance();
            Wrapped wrapped = WRAPPED.get(tooltip);
            if (wrapped != null && wrapped.fits(screenW, screenH, widget, language, lines)) {
                return wrapped.lines();
            }
            Component message = ((TooltipText) tooltip).projects$message();
            Map<Integer, List<FormattedCharSequence>> split = new HashMap<>();
            split.put(TooltipPlace.GAME_WRAP, lines);
            int width = TooltipPlace.wrap(
                w -> TooltipPlace.height(split.computeIfAbsent(w, wide -> font.split(message, wide)).size()),
                beside,
                TooltipPlace.roomOnScreen(screenH),
                TooltipPlace.widest(screenW));
            List<FormattedCharSequence> chosen = split.computeIfAbsent(width, wide -> font.split(message, wide));
            WRAPPED.put(tooltip, new Wrapped(screenW, screenH, widget.top(), widget.bottom(), language, lines, chosen));
            return chosen;
        }, lines);
    }

    /** A tooltip's wrap, and what it was made for. */
    private record Wrapped(int screenW, int screenH, int top, int bottom, Language language,
                           List<FormattedCharSequence> from, List<FormattedCharSequence> lines) {
        boolean fits(int screenW, int screenH, ScreenRectangle widget, Language language,
                     List<FormattedCharSequence> from) {
            return this.screenW == screenW && this.screenH == screenH && top == widget.top()
                && bottom == widget.bottom() && this.language == language && this.from == from;
        }
    }
}
