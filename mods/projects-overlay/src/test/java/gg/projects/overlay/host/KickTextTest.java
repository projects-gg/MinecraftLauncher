package gg.projects.overlay.host;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class KickTextTest {
    @Test
    void theGuestsOwnLanguageInTheWorldTurkishOrElseEnglish() {
        assertEquals("Bu dünyadan çıkarıldın.", KickText.inWorld(KickText.Refusal.REMOVED, "Ayşe", "tr_tr"));
        assertEquals("You were removed from this world.", KickText.inWorld(KickText.Refusal.REMOVED, "Ayşe",
            "en_us"));
        assertEquals("You were removed from this world.", KickText.inWorld(KickText.Refusal.REMOVED, "Ayşe",
            "de_de"));
        assertEquals("Ayşe paylaşımı kapattı.", KickText.inWorld(KickText.Refusal.CLOSED, "Ayşe", "TR_TR"));
        assertEquals("Ayşe stopped sharing their world.", KickText.inWorld(KickText.Refusal.CLOSED, "Ayşe", null));
        assertEquals("Dünya paylaşımı kapandı.", KickText.inWorld(KickText.Refusal.CLOSED, " ", "tr_tr"));
    }

    @Test
    void atLoginBothLanguagesTurkishFirst() {
        assertEquals("Bu adla giremezsin.\nYou can't join with this name.",
            KickText.atLogin(KickText.Refusal.NAME, null));
        assertEquals("Bu ad bu dünyada başka birine ait.\nSomeone else uses this name in this world.",
            KickText.atLogin(KickText.Refusal.NAME_TAKEN, null));
        assertEquals("Dünya dolu.\nThe world is full.", KickText.atLogin(KickText.Refusal.FULL, null));
    }

    @Test
    void onlyTurkishCodesReadTurkish() {
        assertTrue(KickText.turkish("tr"));
        assertTrue(KickText.turkish("tr-TR"));
        assertFalse(KickText.turkish("trk"));
        assertFalse(KickText.turkish(""));
    }

    @Test
    void namesCompareAsciiCaseInsensitivelyWhateverTheLocale() {
        java.util.Locale before = java.util.Locale.getDefault();
        try {
            java.util.Locale.setDefault(java.util.Locale.forLanguageTag("tr-TR"));
            assertTrue(Names.same("NOTCH_IS_IT", "notch_is_it"));
            assertEquals("notch_is_it", Names.key("NOTCH_IS_IT"));
            assertFalse(Names.same("Ayşe", "AYŞE"));
            assertTrue(Names.valid("jeb_"));
            assertFalse(Names.valid("jb"));
            assertFalse(Names.valid("a_very_long_name_17"));
        } finally {
            java.util.Locale.setDefault(before);
        }
    }
}
