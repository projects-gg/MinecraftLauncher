package gg.projects.overlay.settings;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import gg.projects.overlay.state.OverlayState;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

class SocialRowsTest {
    private static List<String> keys(List<SocialRows.Notify> rows) {
        List<String> out = new ArrayList<>();
        for (SocialRows.Notify n : rows) {
            out.add(n.key());
        }
        return out;
    }

    @Test
    void theCardRowsAreTheOlderThreeThenEveryNewerGroup() {
        assertEquals(List.of("message", "call", "invite"), SocialRows.CARDS.subList(0, 3));
        assertEquals(OverlaySettings.CARD_GROUPS, SocialRows.CARDS.subList(3, SocialRows.CARDS.size()));
        assertFalse(SocialRows.CARDS.contains("info"));
    }

    @Test
    void everyCardRowTurnedOffGoesToTheLauncher() {
        // "Kapattıkların launcher bildirimi olarak gelir.": each row off is a group the launcher hears.
        for (String group : SocialRows.CARDS) {
            assertTrue(SocialRows.shows(OverlaySettings.DEFAULTS, group), group);
            OverlaySettings off = SocialRows.show(OverlaySettings.DEFAULTS, group, false);
            assertFalse(SocialRows.shows(off, group), group);
            assertEquals(List.of(group), off.offGroups(), group);
            assertFalse(off.isDefault(), group);
            OverlaySettings back = SocialRows.show(off, group, true);
            assertTrue(SocialRows.shows(back, group), group);
            assertTrue(back.isDefault(), group);
        }
    }

    @Test
    void aCardRowLeavesTheOthersAlone() {
        OverlaySettings s = SocialRows.show(OverlaySettings.DEFAULTS, "plan", false);
        s = SocialRows.show(s, "call", false);
        for (String group : SocialRows.CARDS) {
            assertEquals(!group.equals("plan") && !group.equals("call"), SocialRows.shows(s, group), group);
        }
        assertTrue(s.infoToasts());
    }

    @Test
    void theNotifySwitchesFollowWhatTheLauncherTakes() {
        assertEquals(List.of("favoritesOnline", "favoritesProjects"), keys(SocialRows.notify(false, false)));
        assertEquals(List.of("favoritesOnline", "favoritesProjects", "crewChat"), keys(SocialRows.notify(false, true)));
        assertEquals(List.of("favoritesOnline", "favoritesProjects", "crewCalls", "joinRequests"),
            keys(SocialRows.notify(true, false)));
        assertEquals(List.of("favoritesOnline", "favoritesProjects", "crewCalls", "crewChat", "joinRequests"),
            keys(SocialRows.notify(true, true)));
    }

    @Test
    void aSwitchReadsItsOwnPref() {
        assertFalse(SocialRows.on(null, "favoritesOnline"));
        String[] order = {"favoritesOnline", "favoritesProjects", "crewCalls", "crewChat", "joinRequests"};
        for (int i = 0; i < order.length; i++) {
            OverlayState.Prefs prefs = new OverlayState.Prefs(i == 0, i == 1, i == 2, i == 3, i == 4);
            for (int j = 0; j < order.length; j++) {
                assertEquals(i == j, SocialRows.on(prefs, order[j]), order[i] + " / " + order[j]);
            }
        }
        assertFalse(SocialRows.on(new OverlayState.Prefs(true, true, true, true, true), "unknown"));
    }

    @Test
    void everyRowHasWordsInBothLanguages() throws IOException {
        List<String> needed = new ArrayList<>(List.of("settings.cards", "settings.cards.hint", "settings.friend_notify",
            "settings.friend_notify.hint", "settings.blocked", "settings.blocked.empty", "settings.unblock",
            "people.note.unblocked"));
        for (String group : SocialRows.CARDS) {
            needed.add(SocialRows.words(group));
        }
        for (SocialRows.Notify n : SocialRows.notify(true, true)) {
            needed.add(n.words());
        }
        for (String file : List.of("tr_tr", "en_us")) {
            JsonObject words = lang(file);
            for (String key : needed) {
                assertTrue(words.has("projects_overlay." + key), file + ": " + key);
                assertFalse(words.get("projects_overlay." + key).getAsString().isBlank(), file + ": " + key);
            }
        }
    }

    private static JsonObject lang(String file) throws IOException {
        try (InputStream in = SocialRowsTest.class.getResourceAsStream("/assets/projects_overlay/lang/" + file
            + ".json")) {
            assertNotNull(in, file);
            try (Reader reader = new InputStreamReader(in, StandardCharsets.UTF_8)) {
                return JsonParser.parseReader(reader).getAsJsonObject();
            }
        }
    }
}
