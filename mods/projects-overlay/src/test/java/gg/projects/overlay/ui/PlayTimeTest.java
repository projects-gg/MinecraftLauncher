package gg.projects.overlay.ui;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import gg.projects.overlay.chat.MyLocation;
import gg.projects.overlay.lang.LanguageChoice;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

/**
 * CREW-PLUS: a friend's play time as the launcher's {@code played.ts} says it, and "Konumum"'s line (whose numbers never
 * read as a phone number to the friends service).
 */
class PlayTimeTest {
    private static final long SINCE = 1_792_400_100_000L;

    private static long after(long seconds) {
        return SINCE + seconds * 1000;
    }

    @AfterEach
    void gameLanguageAgain() {
        Texts.speak(new LanguageChoice(null, Texts::gameLanguage));
    }

    @Test
    void nothingUnderFiveMinutesThenFiveMinuteStepsThenWholeHours() {
        assertEquals(-1, PlayTime.bucket(0, after(900)));
        assertEquals(-1, PlayTime.bucket(SINCE, after(-60)));
        assertEquals(-1, PlayTime.bucket(SINCE, after(299)));
        assertEquals(5, PlayTime.bucket(SINCE, after(300)));
        assertEquals(25, PlayTime.bucket(SINCE, after(1_799)));
        assertEquals(55, PlayTime.bucket(SINCE, after(3_599)));
        assertEquals(1_001, PlayTime.bucket(SINCE, after(3_600)));
        assertEquals(1_001, PlayTime.bucket(SINCE, after(7_199)));
        assertEquals(1_010, PlayTime.bucket(SINCE, after(36_000)));
    }

    @Test
    void theWordsInBothLanguages() {
        Texts.speak(new LanguageChoice("tr", () -> "tr_tr"));
        assertEquals("25 dk", PlayTime.text(25));
        assertEquals("2 sa", PlayTime.text(1_002));
        assertNull(PlayTime.text(-1));
        assertEquals("1 saattir oyunda", PlayTime.spoken(SINCE, after(3_700)));
        assertEquals("25 dakikadır oyunda", PlayTime.spoken(SINCE, after(1_500)));
        Texts.speak(new LanguageChoice("en", () -> "tr_tr"));
        assertEquals("25 min", PlayTime.text(25));
        assertEquals("playing for 1 hour", PlayTime.spoken(SINCE, after(3_700)));
        assertEquals("playing for 3 hours", PlayTime.spoken(SINCE, after(3 * 3_600)));
    }

    @Test
    void myLocationKeepsLettersBetweenItsNumbers() {
        Texts.speak(new LanguageChoice("tr", () -> "tr_tr"));
        assertEquals("Konumum: X 120, Y 64, Z -340", MyLocation.text(120, 64, -340, null));
        assertEquals("Konumum: X 5321, Y 64, Z -5432 (Nether)", MyLocation.text(5321, 64, -5432, "nether"));
        Texts.speak(new LanguageChoice("en", () -> "tr_tr"));
        assertEquals("My location: X -12, Y -60, Z 7 (End)", MyLocation.text(-12, -60, 7, "end"));
        // The friends service's phone check (chattext.ts): digits with at most three separators between them, ten
        // or more in a run. With a letter between the numbers no run ever gets there, at the world's very edge too.
        java.util.regex.Matcher runs = java.util.regex.Pattern.compile("\\+?\\d(?:[\\s.\\-/()_,]{0,3}\\d)+")
            .matcher(MyLocation.text(29_999_984, 320, -29_999_984, "nether"));
        while (runs.find()) {
            assertEquals(true, runs.group().replaceAll("\\D", "").length() < 10, runs.group());
        }
    }
}
