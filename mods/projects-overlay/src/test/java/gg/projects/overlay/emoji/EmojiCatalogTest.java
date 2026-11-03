package gg.projects.overlay.emoji;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.Test;

/** OVL-SOC: the panel's emoji, read from the launcher's picker data, and their search ({@link EmojiFinder}). */
class EmojiCatalogTest {
    /** A few rows of the launcher's Turkish data, as its script writes them (the header's tone list cut short). */
    static final String TR = String.join("\n",
        "PL-EMOJI\t1\ttr\tUnicode Emoji 17.0\tCLDR 48.2.0 (Unicode License v3)\tgenerated\t261D 1F44D",
        "😀\t0\t\tsırıtan yüz\tgulen mutlu yuz\tgrinning face\tsmile happy",
        "😂\t0\t\tsevinç gözyaşları\tkahkaha komik gulmek\tface with tears of joy\tlol laugh",
        "❤️\t7\t\tkırmızı kalp\tkalp ask sevgi\tred heart\tlove",
        "🫀\t2\t\tanatomik kalp\tkalp organ\tanatomical heart\torgan",
        "👍\t1\t1\tbaş parmak yukarı\tbegen tamam\tthumbs up\tlike yes",
        "👨‍👩‍👧\t1\t\taile: erkek, kadın, kız\taile\tfamily: man, woman, girl\tfamily",
        "🇹🇷\t8\t\tbayrak: Türkiye\tbayrak\tflag: Türkiye\tturkey",
        "🏁\t8\t\tdama bayrağı\tyaris\tchequered flag\trace",
        "🔥\t7\t\tateş\talev yanmak\tfire\tflame hot",
        "",
        "broken row",
        "💀\t99\t\tkafatası\t\tskull\t");

    private static EmojiCatalog catalog() {
        // The faces have a picture of everything but the anatomical heart here.
        return EmojiCatalog.parse(TR, cp -> cp != 0x1FAC0);
    }

    @Test
    void onlyOneCodePointEmojiWithAPictureAreOffered() {
        List<String> shown = catalog().entries().stream().map(EmojiCatalog.Entry::emoji).toList();
        assertEquals(List.of("😀", "😂", "❤", "👍", "🏁", "🔥"), shown,
            "no sequence (family, flag), no picture-less one, no broken row; the style selector goes");
        assertEquals(List.of(EmojiCatalog.Group.SMILEYS, EmojiCatalog.Group.PEOPLE, EmojiCatalog.Group.SYMBOLS,
            EmojiCatalog.Group.FLAGS), catalog().groups(), "the launcher's order, only groups with any");
        EmojiCatalog.Entry heart = catalog().find("❤️");
        assertNotNull(heart, "found by its fully qualified form too");
        assertEquals("kırmızı kalp", heart.name());
        assertEquals(" kalp ask sevgi ", heart.words());
        assertEquals("red heart", heart.english());
        assertEquals(" love ", heart.fallback());
        assertNotNull(catalog().find("👍🏽"), "a skin tone finds the yellow one");
        assertTrue(EmojiCatalog.parse("not emoji data\n😀\t0\t\tx\t\t\t", cp -> true).entries().isEmpty());
    }

    @Test
    void foldingAndNamesAreTheLaunchers() {
        assertEquals("iris gulmek sasirmis", EmojiCatalog.fold("Iris Gülmek ŞAŞIRMIŞ"));
        assertEquals("i", EmojiCatalog.fold("İ"));
        assertEquals("Sevinç gözyaşları", EmojiCatalog.displayName("sevinç gözyaşları", true));
        assertEquals("İnce", EmojiCatalog.displayName("ince", true), "Turkish capitals");
        assertEquals("Ince", EmojiCatalog.displayName("ince", false));
        assertEquals("❤", EmojiCatalog.bare("❤️"));
        assertEquals("👍", EmojiCatalog.bare("👍🏿"));
    }

    @Test
    void searchRanksAsTheLaunchersDoes() {
        EmojiCatalog c = catalog();
        // A whole name word first; within whole words the most used first.
        assertEquals("❤", EmojiFinder.search(c, "kalp").get(0).emoji());
        assertEquals("😂", EmojiFinder.search(c, "sevinç").get(0).emoji());
        // Folded: no Turkish letters needed, capitals do not matter.
        assertEquals("😂", EmojiFinder.search(c, "SEVINC").get(0).emoji());
        assertEquals("😂", EmojiFinder.search(c, "gulmek").get(0).emoji(), "a keyword");
        // A Turkish search finds English names and words too.
        assertEquals("🔥", EmojiFinder.search(c, "fire").get(0).emoji());
        assertEquals("😂", EmojiFinder.search(c, "lol").get(0).emoji());
        // Three letters or more also match inside a word.
        assertTrue(EmojiFinder.search(c, "rmız").stream().anyMatch(e -> e.emoji().equals("❤")));
        assertTrue(EmojiFinder.search(c, "rm").isEmpty(), "two letters only start words");
        // A pasted emoji finds itself; nothing finds nothing.
        assertEquals(List.of(c.find("👍")), EmojiFinder.search(c, "👍🏽"));
        assertTrue(EmojiFinder.search(c, "   ").isEmpty());
        assertTrue(EmojiFinder.search(c, "zzzz").isEmpty());
        assertNull(c.find("🫀"));
    }
}
