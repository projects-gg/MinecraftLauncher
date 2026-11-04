package gg.projects.overlay.ui;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * LABEL-FIT: a button's or chip's label stays inside its box: smaller words first (as far as about three quarters of a
 * button's), then without the icon, only then cut with "…"; the game's pixel font has one size.
 */
class LabelFitTest {
    /** Each character as wide as the face's size in tenths (UI 8.5 → 8.5 a character, rounded up for the line). */
    private static final LabelFit.Measure MEASURE = (face, text) -> {
        double per = switch (face) {
            case UI, STRONG -> 6.0;
            case SMALL -> 5.3;
            case MINI -> 4.6;
            default -> 5.0;
        };
        return (int) Math.ceil(text.codePointCount(0, text.length()) * per);
    };
    /** The voice bar's icon and its gap. */
    private static final int ICON = 14;

    @Test
    void aLabelThatFitsStaysAsItIs() {
        LabelFit.Fit fit = LabelFit.fit("Leave", Fonts.Face.UI, ICON, 60, true, MEASURE);
        assertEquals(new LabelFit.Fit(Fonts.Face.UI, true, "Leave", false, ICON + 30), fit);
    }

    @Test
    void tooWideItGetsSmallerFirstThenLosesItsIconThenIsCut() {
        // "Microphone": 60 wide in UI, 53 small, 46 mini.
        assertEquals(Fonts.Face.SMALL, LabelFit.fit("Microphone", Fonts.Face.UI, ICON, 68, true, MEASURE).face());
        LabelFit.Fit mini = LabelFit.fit("Microphone", Fonts.Face.UI, ICON, 61, true, MEASURE);
        assertEquals(Fonts.Face.MINI, mini.face());
        assertTrue(mini.icon());
        // Not even the smallest words beside the icon: the icon goes, the words take the largest size that fits
        // ("Sağır": 30 in UI, 37 at the smallest with the icon).
        LabelFit.Fit noIcon = LabelFit.fit("Sağır", Fonts.Face.UI, ICON, 36, true, MEASURE);
        assertEquals(new LabelFit.Fit(Fonts.Face.UI, false, "Sağır", false, 30), noIcon);
        LabelFit.Fit small = LabelFit.fit("Microphone", Fonts.Face.UI, ICON, 55, true, MEASURE);
        assertEquals(Fonts.Face.SMALL, small.face());
        assertFalse(small.icon());
        // Narrower than the smallest words alone: cut at the smallest size, never wider than the box.
        LabelFit.Fit cut = LabelFit.fit("Microphone", Fonts.Face.UI, ICON, 30, true, MEASURE);
        assertTrue(cut.cut());
        assertFalse(cut.icon());
        assertEquals(Fonts.Face.MINI, cut.face());
        assertTrue(cut.text().endsWith("…"));
        assertTrue(cut.width() <= 30, cut.toString());
    }

    @Test
    void theGamesPixelFontHasOneSize() {
        assertEquals(List.of(Fonts.Face.UI), LabelFit.steps(Fonts.Face.UI, false));
        LabelFit.Fit fit = LabelFit.fit("Microphone", Fonts.Face.UI, ICON, 68, false, MEASURE);
        assertEquals(new LabelFit.Fit(Fonts.Face.UI, false, "Microphone", false, 60), fit, "the icon goes first");
    }

    @Test
    void aButtonsWordsStepDownToAboutThreeQuartersAChipsOneStep() {
        assertEquals(List.of(Fonts.Face.UI, Fonts.Face.SMALL, Fonts.Face.MINI), LabelFit.steps(Fonts.Face.UI, true));
        assertEquals(List.of(Fonts.Face.SMALL, Fonts.Face.MINI), LabelFit.steps(Fonts.Face.SMALL, true));
        assertEquals(List.of(Fonts.Face.TAG), LabelFit.steps(Fonts.Face.TAG, true));
        float floor = Fonts.Face.MINI.size / Fonts.Face.UI.size;
        assertTrue(floor > 0.7f && floor < 0.8f, "the smallest is about 75 %: " + floor);
    }

    @Test
    void aRowOfButtonsSharesByNeedOnlyWhenEqualSharesDoNotFit() {
        // Every label fits a third: thirds, the last taking the rounding.
        assertArrayEquals(new int[] {60, 60, 62}, LabelFit.row(182, new int[] {58, 50, 44}));
        // "Microphone" wants more than a third: by need, so it keeps its icon before anything shrinks.
        int[] byNeed = LabelFit.row(182, new int[] {74, 58, 57});
        assertArrayEquals(new int[] {71, 55, 56}, byNeed);
        assertEquals(182, byNeed[0] + byNeed[1] + byNeed[2]);
        assertArrayEquals(new int[] {}, LabelFit.row(100, new int[] {}));
        // A row takes the size its narrowest button needed.
        assertEquals(Fonts.Face.SMALL, LabelFit.smallest(Fonts.Face.UI, Fonts.Face.SMALL, Fonts.Face.UI));
        assertEquals(Fonts.Face.MINI, LabelFit.smallest(Fonts.Face.SMALL, Fonts.Face.MINI));
        assertEquals(Fonts.Face.UI, LabelFit.smallest(Fonts.Face.UI));
    }

    @Test
    void aCutNeverSplitsALetterNorLeavesTrailingSpace() {
        assertEquals("Ses ay…", LabelFit.cut("Ses ayarları", Fonts.Face.UI, 42, MEASURE));
        assertEquals("Ses…", LabelFit.cut("Ses ayarları", Fonts.Face.UI, 24, MEASURE));
        assertEquals("…", LabelFit.cut("Ses", Fonts.Face.UI, 7, MEASURE));
        assertEquals("", LabelFit.cut("Ses", Fonts.Face.UI, 5, MEASURE));
        // An emoji is one character of two Java chars: never half of it.
        assertEquals("A…", LabelFit.cut("A😀B", Fonts.Face.UI, 15, MEASURE));
    }
}
