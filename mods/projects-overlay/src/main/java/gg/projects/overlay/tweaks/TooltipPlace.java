package gg.projects.overlay.tweaks;

import java.util.function.IntUnaryOperator;

/**
 * TOOLTIP-FIT's rules, free of the game (unit-tested): where a tooltip goes so the whole of it shows, how wide a
 * widget's tooltip is wrapped so it fits beside the widget, and how far one larger than the screen is shrunk.
 *
 * <p>Sizes are the tooltip's text area as the game measures it (GUI pixels); its frame adds {@link #FRAME} on every
 * side.
 */
final class TooltipPlace {
    /** The tooltip's frame beyond its text area, each side (the game's padding and border). */
    static final int FRAME = 4;
    /** The least room between the frame and the screen's edge. */
    static final int EDGE = 2;
    /** The game's wrap width for a widget's tooltip ({@code Tooltip.splitTooltip}). */
    static final int GAME_WRAP = 170;
    /** The widest wrap: longer lines read badly. */
    static final int MAX_WRAP = 360;
    /** Wider wraps are tried in steps of this. */
    static final int WRAP_STEP = 20;
    /** A line of a text tooltip: the font's 9 pixels and one between lines. */
    static final int LINE = 10;
    /** A tooltip larger than the screen is never drawn smaller than this. */
    static final float MIN_SCALE = 0.5F;

    private TooltipPlace() {
    }

    /**
     * Where a tooltip {@code size} long on one axis starts on a screen {@code screen} long: {@code at}, where the game
     * put it, moved the least so that its frame shows whole; the screen's start when it is longer than the screen.
     */
    static int fit(int at, int size, int screen) {
        int least = FRAME + EDGE;
        int most = screen - FRAME - EDGE - size;
        if (most < least) {
            return least;
        }
        return Math.max(least, Math.min(at, most));
    }

    /** The text height of a tooltip of {@code lines} lines of text, as the game adds it up. */
    static int height(int lines) {
        return Math.max(0, lines * LINE - 2);
    }

    /**
     * The scale that shows a tooltip {@code w} by {@code h} whole on the screen: 1 when it fits as it is, never below
     * {@link #MIN_SCALE}.
     */
    static float scale(int w, int h, int screenW, int screenH) {
        int frame = 2 * (FRAME + EDGE);
        float across = (screenW - frame) / (float) Math.max(1, w);
        float down = (screenH - frame) / (float) Math.max(1, h);
        return Math.max(MIN_SCALE, Math.min(1F, Math.min(across, down)));
    }

    /**
     * The text height a widget's tooltip may have for the game to put it below the widget: the game's own test
     * ({@code MenuTooltipPositioner}), which keeps a widget's height of room under it.
     */
    static int roomBelow(int widgetBottom, int widgetHeight, int screenH) {
        return screenH - 5 - 6 - widgetBottom - widgetHeight;
    }

    /**
     * The text height a widget's tooltip may have above the widget with its frame on the screen (the game leaves up to
     * 10 between them).
     */
    static int roomAbove(int widgetTop) {
        return widgetTop - 10 - FRAME - EDGE;
    }

    /** The text height that shows whole on a screen {@code screenH} high. */
    static int roomOnScreen(int screenH) {
        return screenH - 2 * (FRAME + EDGE);
    }

    /**
     * The widest wrap on a screen {@code screenW} wide: the tooltip still fits on one side of the pointer (the game puts
     * it 12 to the right of it, or as far to the left), so it never covers the pointer. Never below the game's own.
     */
    static int widest(int screenW) {
        return Math.max(GAME_WRAP, Math.min(MAX_WRAP, (screenW - 38) / 2));
    }

    /**
     * The width to wrap a widget's tooltip at: the game's while the tooltip fits beside its widget (below or above it);
     * else the narrowest wider one that fits beside it; else the narrowest that fits on the screen at all (it then covers
     * part of the widget, moved by {@link #fit}); else the widest, which {@link #scale} shrinks.
     *
     * @param heightAt the text height at a width
     * @param beside   the text height that fits beside the widget ({@link #roomBelow}, {@link #roomAbove})
     * @param onScreen the text height that fits on the screen ({@link #roomOnScreen})
     * @param widest   the widest wrap ({@link #widest})
     */
    static int wrap(IntUnaryOperator heightAt, int beside, int onScreen, int widest) {
        int game = heightAt.applyAsInt(GAME_WRAP);
        if (game <= beside || widest <= GAME_WRAP) {
            return GAME_WRAP;
        }
        int fitsScreen = game <= onScreen ? GAME_WRAP : -1;
        for (int width = GAME_WRAP + WRAP_STEP; ; width += WRAP_STEP) {
            int at = Math.min(width, widest);
            int height = heightAt.applyAsInt(at);
            if (height <= beside) {
                return at;
            }
            if (fitsScreen < 0 && height <= onScreen) {
                fitsScreen = at;
            }
            if (at == widest) {
                break;
            }
        }
        return fitsScreen > 0 ? fitsScreen : widest;
    }
}
