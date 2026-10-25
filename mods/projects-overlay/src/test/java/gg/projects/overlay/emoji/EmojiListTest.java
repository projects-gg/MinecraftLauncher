package gg.projects.overlay.emoji;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.util.List;
import org.junit.jupiter.api.Test;

class EmojiListTest {
    private static JsonObject json(String text) {
        return JsonParser.parseString(text).getAsJsonObject();
    }

    @Test
    void theJarCarriesTheServersListAsTheLauncherHasIt() {
        EmojiList builtin = EmojiList.builtin();
        // crates/pl-content/data/chat-emojis.json: the launcher's ChatEmojis::builtin reads the same 45.
        assertEquals(45, builtin.emojis().size());
        assertEquals(List.of("projects.gg"), builtin.hosts());
        Emoji kalp = builtin.emojis().get(0);
        assertEquals(":kalp:", kalp.code());
        assertEquals("&c❤", kalp.look());
        assertEquals("VIP", kalp.rank());
        assertTrue(kalp.answersTo("<3"));
        assertTrue(kalp.answersTo(":KALP:"));
        long pack = builtin.emojis().stream().filter(e -> e.kind() == EmojiKind.PACK).count();
        assertEquals(10, pack);
        assertTrue(builtin.emojis().stream().filter(e -> e.kind() == EmojiKind.PACK)
            .allMatch(e -> e.glyph() >= 0xE106 && e.glyph() <= 0xE10F && e.servers().equals(List.of("Survival"))));
        Emoji ping = builtin.emojis().stream().filter(e -> e.code().equals(":ping:")).findFirst().orElseThrow();
        assertEquals(EmojiKind.SHORTCUT, ping.kind());
        assertEquals("Pingini yazar", ping.name(true));
        assertEquals("Writes your ping", ping.name(false));
        assertNull(ping.look());
    }

    @Test
    void itAppliesOnlyOnItsHosts() {
        EmojiList list = EmojiList.builtin();
        assertTrue(list.appliesTo("play.projects.gg"));
        assertTrue(list.appliesTo("PLAY.Projects.GG.:25565"));
        assertTrue(list.appliesTo("projects.gg"));
        assertFalse(list.appliesTo("notprojects.gg"));
        assertFalse(list.appliesTo("mc.hypixel.net"));
        assertFalse(list.appliesTo(null));
        assertTrue(EmojiList.hostMatches("10.0.0.5:25565", List.of("10.0.0.5")));
        assertFalse(EmojiList.hostMatches("x.10.0.0.5", List.of("10.0.0.5")));
    }

    @Test
    void theLaunchersFrameReplacesTheList() {
        EmojiList read = EmojiList.read(json("""
            {"t":"emojis","on":true,"hosts":["projects.gg","not a host"],"emojis":[
              {"code":":a:","look":"&ca","kind":"symbol","rank":"VIP+"},
              {"code":":hmm:","look":"&e(・・?)","glyph":"\\ue106","kind":"pack","servers":["Survival"]},
              {"code":":p:","kind":"shortcut","meaning":{"tr":"Yazar","en":"Writes"}},
              {"code":":x:","look":"x","kind":"sticker","later":1}
            ]}"""));
        assertEquals(List.of("projects.gg"), read.hosts());
        assertEquals(4, read.emojis().size());
        assertEquals(0xE106, read.emojis().get(1).glyph());
        assertEquals(EmojiKind.SYMBOL, read.emojis().get(3).kind(), "an unknown kind shows with the symbols");
    }

    @Test
    void entriesThatBreakTheRulesAreLeftOut() {
        EmojiList read = EmojiList.read(json("""
            {"hosts":["projects.gg"],"emojis":[
              {"code":":ok:","look":"ok"},
              {"code":"bad code","look":"x"},
              {"code":":nolook:"},
              {"code":":amp:","look":"&"},
              {"code":":obf:","look":"&kx"},
              {"code":":pua:","look":"\\ue106"},
              {"code":":g:","look":"x","glyph":"\\ue106"},
              {"code":":pk:","look":"x","kind":"pack"},
              {"code":":pk2:","look":"x","kind":"pack","glyph":"ab"},
              {"code":":sc:","kind":"shortcut"},
              {"code":":rank:","look":"x","rank":"MuchTooLongRank"},
              {"code":":srv:","look":"x","servers":["A","a"]},
              {"code":":OK:","look":"again"},
              {"code":":al:","aliases":[":ok:"],"look":"x"},
              {"code":":self:","aliases":[":self:"],"look":"x"},
              {"code":":last:","look":"x"}
            ]}"""));
        List<String> codes = read.emojis().stream().map(Emoji::code).toList();
        assertEquals(List.of(":ok:", ":last:"), codes);
    }

    @Test
    void anEmptyOrBrokenDocumentIsAnEmptyList() {
        EmojiList empty = EmojiList.read(json("{}"));
        assertTrue(empty.emojis().isEmpty());
        assertFalse(empty.appliesTo("projects.gg"), "no hosts: it applies nowhere");
        assertNotNull(EmojiList.load(new java.io.ByteArrayInputStream("not json".getBytes())));
        assertTrue(EmojiList.load(null).emojis().isEmpty());
    }

    @Test
    void codesFollowTheContentFormat() {
        assertTrue(EmojiList.validCode(":kalp:"));
        assertTrue(EmojiList.validCode("<3"));
        assertTrue(EmojiList.validCode(":başarım:"));
        assertFalse(EmojiList.validCode(""));
        assertFalse(EmojiList.validCode(":a b:"));
        assertFalse(EmojiList.validCode("§c"));
        assertFalse(EmojiList.validCode("x".repeat(33)));
        assertTrue(EmojiList.validHost("projects.gg"));
        assertTrue(EmojiList.validHost("10.0.0.5"));
        assertFalse(EmojiList.validHost("projects"));
        assertFalse(EmojiList.validHost("https://projects.gg"));
        assertFalse(EmojiList.validHost("projects.gg:25565"));
    }
}
