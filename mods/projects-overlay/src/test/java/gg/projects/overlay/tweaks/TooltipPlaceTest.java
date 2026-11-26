package gg.projects.overlay.tweaks;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.function.IntUnaryOperator;
import org.junit.jupiter.api.Test;

/**
 * TOOLTIP-FIT's rules: on the screen whole, wrapped wider only when it helps, smaller only when larger than the
 * screen.
 */
class TooltipPlaceTest {
    /** A text of {@code chars} characters of 6 pixels, wrapped at a width: its text height. */
    private static IntUnaryOperator text(int chars) {
        return width -> TooltipPlace.height((int) Math.ceil(chars * 6.0 / width));
    }

    @Test
    void aTooltipOnTheScreenStaysWhereTheGamePutIt() {
        assertEquals(40, TooltipPlace.fit(40, 100, 400));
        assertEquals(6, TooltipPlace.fit(6, 100, 400));
        assertEquals(294, TooltipPlace.fit(294, 100, 400));
    }

    @Test
    void aTooltipOffTheTopComesDownTheLeastAndOneOffTheBottomComesUp() {
        // The server window's case: put above its button, running off the top.
        assertEquals(6, TooltipPlace.fit(-95, 260, 410));
        // Off the bottom, and off the right edge.
        assertEquals(410 - 6 - 260, TooltipPlace.fit(300, 260, 410));
        assertEquals(789 - 6 - 182, TooltipPlace.fit(700, 182, 789));
    }

    @Test
    void aTooltipLongerThanTheScreenStartsAtItsTop() {
        assertEquals(6, TooltipPlace.fit(-300, 500, 410));
        assertEquals(6, TooltipPlace.fit(50, 500, 410));
    }

    @Test
    void onlyATooltipLargerThanTheScreenIsDrawnSmallerAndNeverBelowHalf() {
        assertEquals(1F, TooltipPlace.scale(170, 300, 789, 410));
        assertEquals(1F, TooltipPlace.scale(170, 398, 789, 410));
        float scale = TooltipPlace.scale(170, 796, 789, 410);
        assertEquals(0.5F, scale, 0.001F);
        assertTrue(TooltipPlace.scale(170, 600, 789, 410) < 1F);
        assertTrue(Math.round(600 * TooltipPlace.scale(170, 600, 789, 410)) <= 410 - 12);
        assertEquals(TooltipPlace.MIN_SCALE, TooltipPlace.scale(170, 5000, 789, 410));
        assertTrue(TooltipPlace.scale(2000, 50, 789, 410) < 1F);
    }

    @Test
    void textHeightsAddUpAsTheGamesDo() {
        assertEquals(8, TooltipPlace.height(1));
        assertEquals(260, TooltipPlace.height(26));
        assertEquals(0, TooltipPlace.height(0));
    }

    @Test
    void roomBesideAWidgetFollowsTheGamesOwnTests() {
        // A 20 high button from 172 to 192 on a 410 high screen.
        assertEquals(410 - 11 - 192 - 20, TooltipPlace.roomBelow(192, 20, 410));
        assertEquals(172 - 16, TooltipPlace.roomAbove(172));
        assertEquals(398, TooltipPlace.roomOnScreen(410));
    }

    @Test
    void theWidestWrapLeavesRoomBesideThePointerAndIsNeverBelowTheGames() {
        assertEquals(360, TooltipPlace.widest(1920));
        assertEquals(360, TooltipPlace.widest(789));
        assertEquals((600 - 38) / 2, TooltipPlace.widest(600));
        assertEquals(TooltipPlace.GAME_WRAP, TooltipPlace.widest(300));
    }

    @Test
    void theGamesWrapStandsWhileTheTooltipFitsBesideItsWidget() {
        assertEquals(TooltipPlace.GAME_WRAP, TooltipPlace.wrap(text(400), 150, 398, 360));
    }

    @Test
    void aTooltipThatFitsNowhereBesideItsWidgetWrapsOnlyAsWideAsItTakes() {
        // 26 lines at 170: 258 high; 187 fits below the button. The narrowest width that fits is taken.
        IntUnaryOperator page = text(740);
        int width = TooltipPlace.wrap(page, 187, 398, 375);
        assertTrue(page.applyAsInt(width) <= 187);
        assertTrue(page.applyAsInt(width - TooltipPlace.WRAP_STEP) > 187 || width == TooltipPlace.GAME_WRAP);
        assertTrue(width > TooltipPlace.GAME_WRAP && width <= 375);
    }

    @Test
    void theWidestWrapIsTriedEvenOffTheSteps() {
        List<Integer> tried = new ArrayList<>();
        IntUnaryOperator page = width -> {
            tried.add(width);
            return width >= 205 ? 100 : 300;
        };
        assertEquals(205, TooltipPlace.wrap(page, 120, 398, 205));
        assertEquals(List.of(170, 190, 205), tried);
    }

    @Test
    void aTooltipThatFitsBesideItsWidgetAtNoWidthKeepsTheNarrowestThatFitsTheScreen() {
        // Too tall beside the widget at every width, but on the screen at the game's: the game's lines, moved.
        assertEquals(TooltipPlace.GAME_WRAP, TooltipPlace.wrap(width -> 300, 150, 398, 360));
        // On the screen only from 250 on.
        assertEquals(250, TooltipPlace.wrap(width -> width >= 250 ? 380 : 420, 150, 398, 360));
        // Nowhere: the widest, which is then drawn smaller.
        assertEquals(360, TooltipPlace.wrap(width -> 900, 150, 398, 360));
    }

    @Test
    void aNarrowScreenKeepsTheGamesWrap() {
        assertEquals(TooltipPlace.GAME_WRAP, TooltipPlace.wrap(width -> 900, 150, 398, TooltipPlace.GAME_WRAP));
    }
}
