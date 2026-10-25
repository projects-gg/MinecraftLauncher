package gg.projects.overlay.emoji;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonParser;
import gg.projects.overlay.bridge.Protocol;
import java.util.List;
import org.junit.jupiter.api.Test;

class EmojiStateTest {
    @Test
    void theGameStartsWithTheLaunchersSwitchAndTheJarsList() {
        assertTrue(EmojiState.onAtStart(null), "an older launcher names no switch: on");
        assertTrue(EmojiState.onAtStart("on"));
        assertFalse(EmojiState.onAtStart("off"));
        assertFalse(EmojiState.onAtStart(" OFF "));
        EmojiState state = new EmojiState(true);
        assertFalse(state.heard());
        assertEquals(EmojiList.builtin().emojis(), state.projects().emojis());
    }

    @Test
    void theLaunchersFrameBringsTheSwitchAndTheList() {
        EmojiState state = new EmojiState(true);
        int before = state.version();
        state.apply(JsonParser.parseString("""
            {"t":"emojis","on":false,"hosts":["projects.gg"],"emojis":[{"code":":a:","look":"a"}]}""")
            .getAsJsonObject());
        assertTrue(state.heard());
        assertFalse(state.on());
        assertEquals(List.of(":a:"), state.projects().emojis().stream().map(Emoji::code).toList());
        assertNotEquals(before, state.version());
        // A frame without the switch keeps it.
        state.apply(JsonParser.parseString("{\"t\":\"emojis\",\"emojis\":[]}").getAsJsonObject());
        assertFalse(state.on());
        assertTrue(state.projects().emojis().isEmpty());
    }

    @Test
    void thePlayersSwitchHoldsAtOnce() {
        EmojiState state = new EmojiState(true);
        int before = state.version();
        state.switchTo(true);
        assertEquals(before, state.version(), "the same value changes nothing");
        state.switchTo(false);
        assertFalse(state.on());
        assertNotEquals(before, state.version());
        state.switchTo(true);
        state.endedByLauncher();
        assertFalse(state.on());
    }

    @Test
    void theFramesReadAndWriteAsTheLogSays() {
        // E1/E2: the hello names the picker; the picker alone says it has no panel and nothing of the panel's.
        String panel = Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr").getAsJsonArray("features").toString();
        assertTrue(panel.contains("\"emoji\"") && panel.contains("\"people2\"") && !panel.contains("panel-off"));
        String alone = Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr", false).getAsJsonArray("features").toString();
        assertEquals("[\"lang\",\"emoji\",\"panel-off\"]", alone);
        // E4: the player's switch.
        assertEquals("{\"t\":\"emoji\",\"on\":false}", Protocol.emoji(false).toString());
    }

    @Test
    void theUnicodeSetOffersOnlyWhatTheFontDraws() {
        assertTrue(UnicodeSet.all().size() > 40);
        List<Emoji> drawable = UnicodeSet.drawable(cp -> cp < 0x10000);
        assertTrue(drawable.size() < UnicodeSet.all().size(), "the emoji outside the first plane are left out");
        assertTrue(drawable.stream().allMatch(e -> e.code().codePoints().allMatch(cp -> cp < 0x10000)));
        assertEquals(UnicodeSet.all(), UnicodeSet.drawable(cp -> true));
        Emoji heart = UnicodeSet.all().get(0);
        assertEquals("❤", heart.code());
        assertEquals("Kalp", heart.name(true));
        assertEquals("Heart", heart.name(false));
        // Every entry is a valid look (written as it is, shown as it is).
        assertTrue(UnicodeSet.all().stream().allMatch(e -> Look.spans(e.look()) != null));
    }
}
