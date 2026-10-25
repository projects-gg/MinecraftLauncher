package gg.projects.overlay.lang;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.Test;

/** The overlay's language (OVL-LANG): its words, where they come from, and who decides which language shows. */
class LanguageTest {
    @Test
    void theBridgeAndTheGameNameTheTwoLanguages() {
        assertSame(Language.TURKISH, Language.ofCode("tr"));
        assertSame(Language.ENGLISH, Language.ofCode(" en "));
        assertNull(Language.ofCode("de"));
        assertNull(Language.ofCode(""));
        assertNull(Language.ofCode(null));
        assertSame(Language.TURKISH, Language.ofGame("tr_tr"));
        assertSame(Language.TURKISH, Language.ofGame("TR_TR"));
        assertSame(Language.ENGLISH, Language.ofGame("en_us"));
        assertSame(Language.ENGLISH, Language.ofGame("de_de"), "the overlay ships two; any other reads English");
        assertSame(Language.ENGLISH, Language.ofGame(null));
        // The "Dil" row names each in its own language.
        assertEquals("Türkçe", Language.TURKISH.ownName);
        assertEquals("English", Language.ENGLISH.ownName);
        assertEquals("tr_tr", Language.TURKISH.gameCode);
        assertEquals("İ", "i".toUpperCase(Language.TURKISH.locale));
        assertEquals("I", "i".toUpperCase(Language.ENGLISH.locale));
    }

    @Test
    void theJarCarriesBothLanguagesWithTheSameWords() {
        Dictionary words = Dictionary.shared();
        assertEquals("Dil", words.template(Language.TURKISH, "projects_overlay.settings.language"));
        assertEquals("Language", words.template(Language.ENGLISH, "projects_overlay.settings.language"));
        assertEquals("Genel", words.template(Language.TURKISH, "projects_overlay.settings.general"));
        assertEquals("Off", words.template(Language.ENGLISH, "projects_overlay.settings.off"));
        // Every word in both files, with as many slots in each: a key added to one only would show the other language.
        assertFalse(words.keys(Language.TURKISH).isEmpty());
        assertEquals(words.keys(Language.TURKISH), words.keys(Language.ENGLISH));
        for (String key : words.keys(Language.TURKISH)) {
            assertEquals(slots(words, Language.TURKISH, key), slots(words, Language.ENGLISH, key), key);
        }
        assertSame(words, Dictionary.shared(), "read once");
    }

    private static long slots(Dictionary words, Language language, String key) {
        return words.parts(words.template(language, key)).stream().filter(Dictionary.Part::argument).count();
    }

    @Test
    void aMissingWordComesFromTheOtherLanguageElseItsKey() {
        Dictionary words = Dictionary.of(Map.of("a", "bir"), Map.of("a", "one", "b", "two"));
        assertEquals("bir", words.template(Language.TURKISH, "a"));
        assertEquals("one", words.template(Language.ENGLISH, "a"));
        assertEquals("two", words.template(Language.TURKISH, "b"));
        assertEquals("c", words.template(Language.ENGLISH, "c"));
        assertTrue(words.has(Language.ENGLISH, "b"));
        assertFalse(words.has(Language.TURKISH, "b"));
    }

    @Test
    void templatesFillAsTheGamesTranslationsDo() {
        Dictionary words = Dictionary.of(Map.of(
            "with", "%s ile %s",
            "swap", "%2$s, %1$s",
            "share", "%%%s",
            "end", "100%",
            "odd", "%d kez",
            "plain", "Ayarlar"), Map.of());
        assertEquals("Ayşe ile Mert", words.format(Language.TURKISH, "with", "Ayşe", "Mert"));
        assertEquals("Mert, Ayşe", words.format(Language.TURKISH, "swap", "Ayşe", "Mert"));
        assertEquals("%70", words.format(Language.TURKISH, "share", 70));
        assertEquals("100%", words.format(Language.TURKISH, "end"));
        assertEquals("%d kez", words.format(Language.TURKISH, "odd"), "not a slot the game knows: as written");
        assertEquals("Ayşe ile ", words.format(Language.TURKISH, "with", "Ayşe"), "a missing argument reads as nothing");
        assertEquals("Ayarlar", words.format(Language.TURKISH, "plain"));
        List<Dictionary.Part> parts = words.parts("%s ile %s");
        assertEquals(3, parts.size());
        assertTrue(parts.get(0).argument() && parts.get(0).index() == 0);
        assertEquals(" ile ", parts.get(1).text());
        assertSame(parts, words.parts("%s ile %s"), "taken apart once");
        assertEquals(List.of(new Dictionary.Part("", -1)), words.parts(""));
    }

    @Test
    void withoutTheLaunchersWordTheGamesLanguageStays() {
        AtomicReference<String> game = new AtomicReference<>("tr_tr");
        LanguageChoice choice = new LanguageChoice(null, game::get);
        assertFalse(choice.managed(), "an older launcher: no Dil row");
        assertSame(Language.TURKISH, choice.current());
        assertEquals("tr_tr", choice.helloCode());
        game.set("de_de");
        assertSame(Language.ENGLISH, choice.current());
        assertEquals("de_de", choice.helloCode(), "the hello names the game's own language, as before");
        // A welcome without a language (an older launcher) changes nothing and keeps no pick.
        assertFalse(choice.welcome(null));
        assertFalse(choice.editable());
        assertNull(choice.pick(Language.TURKISH, 1_000));
        assertFalse(new LanguageChoice("xx", game::get).managed(), "a word that is no language is none");
    }

    @Test
    void theLaunchersWordHoldsWhateverTheGameSpeaks() {
        LanguageChoice choice = new LanguageChoice("en", () -> "tr_tr");
        assertTrue(choice.managed());
        assertSame(Language.ENGLISH, choice.current(), "the language the game started in");
        assertEquals("en_us", choice.helloCode());
        assertFalse(choice.editable(), "nothing keeps a pick before the welcome");
        int start = choice.revision();
        assertFalse(choice.welcome("en"));
        assertEquals(start, choice.revision(), "the same language: nothing to lay out again");
        assertTrue(choice.editable());
        assertTrue(choice.welcome("tr"));
        assertSame(Language.TURKISH, choice.current());
        assertNotEquals(start, choice.revision());
        // A welcome without a language keeps the one there is, and keeps no pick.
        int now = choice.revision();
        assertFalse(choice.welcome("??"));
        assertSame(Language.TURKISH, choice.current());
        assertEquals(now, choice.revision());
        assertFalse(choice.editable());
        assertFalse(choice.welcome("tr"));
        assertTrue(choice.editable());
        choice.offline();
        assertFalse(choice.editable(), "the connection went");
    }

    @Test
    void aPickGoesOnlyWhenTheLauncherKeepsItOnceAtATime() {
        LanguageChoice choice = new LanguageChoice("tr", () -> "en_us");
        assertNull(choice.pick(Language.ENGLISH, 1_000), "before the welcome nothing keeps it");
        choice.welcome("tr");
        assertNull(choice.pick(Language.TURKISH, 1_000), "it speaks Turkish already");
        assertNull(choice.pick(null, 1_000));
        assertEquals("en", choice.pick(Language.ENGLISH, 1_000));
        // The panel changes with the launcher's answer, not before.
        assertSame(Language.TURKISH, choice.current());
        assertTrue(choice.welcome("en"));
        assertNull(choice.pick(Language.TURKISH, 1_000 + LanguageChoice.PICK_GAP_MS - 1), "a held key: one change");
        assertEquals("tr", choice.pick(Language.TURKISH, 1_000 + LanguageChoice.PICK_GAP_MS));
        choice.offline();
        assertNull(choice.pick(Language.TURKISH, 10_000), "offline: no launcher keeps it");
    }
}
