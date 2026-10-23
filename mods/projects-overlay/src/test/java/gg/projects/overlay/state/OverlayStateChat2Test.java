package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** CHAT2's frames and fields (docs/tasks/ovl2-protocol.md, CHAT2-O) as the mod mirrors them. */
class OverlayStateChat2Test {
    private final OverlayState state = new OverlayState();

    private OverlayState.ToastRequest apply(String json, long now) {
        return state.apply(Protocol.parse(json), now);
    }

    private OverlayState.ToastRequest apply(String json) {
        return apply(json, 1_000);
    }

    private OverlayState.ChatLine line(String conv, String id) {
        return state.line(conv, id);
    }

    @Test
    void extrasAreFeaturesEachOnItsOwn() {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Ece\"},\"features\":[\"friends\",\"chat\","
            + "\"replies\",\"stickers\"]}");
        assertTrue(state.has("replies") && state.has("stickers"));
        assertFalse(state.has("reactions") || state.has("typing"));
    }

    @Test
    void aMessageCarriesItsQuoteReactionsLinksAndCard() {
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:8\",\"sid\":8,\"from\":\"Cem\","
            + "\"text\":\"bak https://youtu.be/x ve https://bit.ly/y\",\"at\":5,"
            + "\"reply\":{\"id\":\"n:abc\",\"sid\":7,\"from\":\"Ece\",\"mine\":true,\"text\":\"geliyor musun\"},"
            + "\"reactions\":[{\"e\":\"❤️\",\"n\":3,\"mine\":true,\"who\":[\"Ayşe\",\"Cem\"]},{\"e\":\"😂\",\"n\":1}],"
            + "\"links\":[{\"url\":\"https://youtu.be/x\",\"trusted\":true},{\"url\":\"http://kotu.example\"}],"
            + "\"card\":{\"site\":\"YouTube\",\"title\":\"Rekor kırıldı\"}}");
        OverlayState.ChatLine l = line("c:t1", "i:8");
        assertEquals(8, l.sid);
        assertEquals(new OverlayState.Reply("n:abc", 7, "Ece", true, "geliyor musun", null, false, false), l.reply);
        assertEquals(2, l.reactions.size());
        OverlayState.Reaction heart = l.reactions.get(0);
        assertEquals("❤️", heart.emoji(), "sent back as the launcher gave it");
        assertEquals("❤", heart.shown(), "drawn without the emoji style mark");
        assertEquals(3, heart.count());
        assertTrue(heart.mine());
        assertEquals(List.of("Ayşe", "Cem"), heart.who());
        assertSame(heart, l.reaction("❤"));
        assertEquals(1, l.ownReactions());
        // Only a plain https address is ever kept to open.
        assertEquals(List.of(new OverlayState.Link("https://youtu.be/x", true)), l.links);
        assertEquals(new OverlayState.Card("YouTube", "Rekor kırıldı"), l.card);
    }

    @Test
    void theSameMessageAgainReplacesItsExtras() {
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:8\",\"sid\":8,\"from\":\"Cem\",\"text\":\"gg\","
            + "\"reactions\":[{\"e\":\"👍\",\"n\":1}]}");
        int before = state.lineRevision("c:t1");
        // A launcher that knows reactions leaves the field out when none are left.
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:8\",\"sid\":8,\"from\":\"Cem\",\"text\":\"gg\"}");
        assertTrue(line("c:t1", "i:8").reactions.isEmpty());
        assertTrue(state.lineRevision("c:t1") > before);
        // Deleted for everyone: nothing of it is left.
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:8\",\"sid\":8,\"from\":\"Cem\",\"deleted\":true,"
            + "\"reactions\":[{\"e\":\"👍\",\"n\":1}]}");
        OverlayState.ChatLine gone = line("c:t1", "i:8");
        assertTrue(gone.deleted && gone.reactions.isEmpty() && gone.reply == null && gone.card == null);
    }

    @Test
    void anOlderLauncherBringsNoneOfThem() {
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m1\",\"from\":\"Ayşe\",\"text\":\"selam\"}");
        OverlayState.ChatLine l = line("d:1", "m1");
        assertEquals(0, l.sid);
        assertNull(l.sticker);
        assertNull(l.reply);
        assertTrue(l.reactions.isEmpty() && l.links.isEmpty());
    }

    @Test
    void aStickerDrawsFromTheSetAndItsWordsAreForOlderMods() {
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"i:3\",\"sid\":3,\"from\":\"Ayşe\",\"sticker\":\"pl1/gg\","
            + "\"text\":\"[Çıkartma: GG]\"}");
        OverlayState.ChatLine l = line("d:1", "i:3");
        assertEquals("pl1/gg", l.sticker);
        assertEquals("", l.text);
        // Something that is not a sticker reference is not one.
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"i:4\",\"sid\":4,\"from\":\"Ayşe\",\"sticker\":\"../x\","
            + "\"text\":\"selam\"}");
        assertNull(line("d:1", "i:4").sticker);
        assertEquals("selam", line("d:1", "i:4").text);
        // A sticker toast names its sticker; a bad one is dropped.
        OverlayState.ToastRequest toast = apply("{\"t\":\"toast\",\"kind\":\"message\",\"key\":\"d:1\","
            + "\"title\":\"Ayşe\",\"body\":\"Çıkartma · GG\",\"conv\":\"d:1\",\"sticker\":\"pl1/gg\"}");
        assertEquals("pl1/gg", toast.sticker());
        assertNull(apply("{\"t\":\"toast\",\"kind\":\"message\",\"key\":\"d:1\",\"title\":\"A\",\"body\":\"b\","
            + "\"sticker\":\"PL1 GG\"}").sticker());
    }

    @Test
    void thePanelsOwnStickerAndReplyWaitForTheirId() {
        OverlayState.Reply quote = new OverlayState.Reply("i:3", 3, "Ayşe", false, "akşam?", null, false, false);
        String cid = state.addPending("d:1", "", "pl1/hi", quote, 1_000);
        OverlayState.ChatLine pending = state.lines("d:1").getFirst();
        assertEquals("pl1/hi", pending.sticker);
        assertSame(quote, pending.reply);
        assertEquals(0, pending.sid);
        apply("{\"t\":\"sent\",\"cid\":\"" + cid + "\",\"id\":\"n:k1\",\"sid\":12}");
        assertEquals(12, pending.sid);
        assertEquals(OverlayState.Delivery.SENT, pending.delivery);
        // The whole message follows; a reply whose message was gone went as a plain one.
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"n:k1\",\"sid\":12,\"from\":\"Ece\",\"mine\":true,"
            + "\"sticker\":\"pl1/hi\",\"text\":\"[Çıkartma: Selam]\"}");
        assertNull(pending.reply);
        assertEquals(1, state.lines("d:1").size());
        // A refusal keeps the link rule for its words.
        String other = state.addPending("d:1", "https://bit.ly/x", 1_000);
        apply("{\"t\":\"send-failed\",\"cid\":\"" + other + "\",\"reason\":\"link\",\"rule\":\"shortener\"}");
        assertEquals("shortener", state.lines("d:1").getLast().failureRule);
    }

    @Test
    void aQuoteIsMadeFromALine() {
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"i:5\",\"sid\":5,\"from\":\"Ayşe\",\"text\":\"akşam bedwars?\"}");
        OverlayState.Reply quote = OverlayState.quoteOf(line("d:1", "i:5"));
        assertEquals(new OverlayState.Reply("i:5", 5, "Ayşe", false, "akşam bedwars?", null, false, false), quote);
    }

    @Test
    void seenSitsUnderTheLastSeenMessageUntilTheFriendWrites() {
        apply("{\"t\":\"conv\",\"id\":\"d:1\",\"kind\":\"dm\",\"title\":\"Ayşe\",\"seen\":11}");
        assertEquals(11, state.conversation("d:1").seen());
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"i:9\",\"sid\":9,\"from\":\"Ayşe\",\"text\":\"a\"}");
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"i:10\",\"sid\":10,\"from\":\"Ece\",\"mine\":true,\"text\":\"b\"}");
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"i:11\",\"sid\":11,\"from\":\"Ece\",\"mine\":true,\"text\":\"c\"}");
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"i:12\",\"sid\":12,\"from\":\"Ece\",\"mine\":true,\"text\":\"d\"}");
        assertSame(line("d:1", "i:11"), OverlayState.seenLine(state.lines("d:1"), 11));
        assertSame(line("d:1", "i:12"), OverlayState.seenLine(state.lines("d:1"), 12));
        assertNull(OverlayState.seenLine(state.lines("d:1"), 0), "not shared");
        assertNull(OverlayState.seenLine(state.lines("d:1"), 9), "nothing of the player's seen yet");
        // The friend answers: the answer says it was seen.
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"i:13\",\"sid\":13,\"from\":\"Ayşe\",\"text\":\"tamam\"}");
        assertNull(OverlayState.seenLine(state.lines("d:1"), 13));
        // A change of "Görüldü" redraws the conversation.
        int before = state.lineRevision("d:1");
        apply("{\"t\":\"conv\",\"id\":\"d:1\",\"kind\":\"dm\",\"title\":\"Ayşe\",\"seen\":13}");
        assertTrue(state.lineRevision("d:1") > before);
    }

    @Test
    void writersShowForSixSecondsAndStopWhenTheyWriteOrLeave() {
        apply("{\"t\":\"typing\",\"conv\":\"c:t1\",\"by\":\"u-cem\",\"name\":\"Cem\"}", 1_000);
        apply("{\"t\":\"typing\",\"conv\":\"c:t1\",\"by\":\"u-ayse\",\"name\":\"Ayşe\"}", 2_000);
        assertEquals(List.of("Cem", "Ayşe"), state.writers("c:t1", 2_500));
        assertEquals(1_000 + OverlayState.TYPING_SHOW_MS, state.typingEndsAt("c:t1"));
        assertEquals(List.of("Ayşe"), state.writers("c:t1", 7_500), "Cem's time is up");
        // Cem's message ends his at once; Ayşe going offline ends hers.
        apply("{\"t\":\"typing\",\"conv\":\"c:t1\",\"by\":\"u-cem\",\"name\":\"Cem\"}", 3_000);
        int revision = state.typingRevision();
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:20\",\"sid\":20,\"from\":\"Cem\",\"text\":\"geldim\"}",
            3_100);
        assertEquals(List.of("Ayşe"), state.writers("c:t1", 3_200));
        assertTrue(state.typingRevision() > revision);
        apply("{\"t\":\"friend\",\"id\":\"u-ayse\",\"name\":\"Ayşe\",\"where\":\"offline\"}", 3_300);
        assertTrue(state.writers("c:t1", 3_400).isEmpty());
        // The panel closing forgets them all.
        apply("{\"t\":\"typing\",\"conv\":\"c:t1\",\"by\":\"u-cem\",\"name\":\"Cem\"}", 4_000);
        state.clearTyping();
        assertTrue(state.writers("c:t1", 4_100).isEmpty());
    }

    @Test
    void aRefusedReactionLeavesANoteForAMoment() {
        assertNull(apply("{\"t\":\"react-failed\",\"conv\":\"c:t1\",\"sid\":3,\"e\":\"👍\",\"reason\":\"full\"}", 1_000));
        OverlayState.Note note = state.note(1_500);
        assertNotNull(note);
        assertEquals("react.full", note.key());
        assertEquals("c:t1", note.conversation());
        assertNull(state.note(1_000 + OverlayState.NOTE_MS + 1));
        apply("{\"t\":\"react-failed\",\"conv\":\"c:t1\",\"sid\":3,\"e\":\"👍\",\"reason\":\"something-new\"}", 2_000);
        assertEquals("react.failed", state.note(2_000).key());
        apply("{\"t\":\"react-failed\",\"conv\":\"c:t1\",\"sid\":3,\"e\":\"👍\",\"reason\":\"not-allowed\"}", 3_000);
        assertEquals("react.not_allowed", state.note(3_000).key());
    }

    @Test
    void anotherAccountForgetsWritersAndNotes() {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Ece\"},\"features\":[\"friends\",\"chat\"]}");
        apply("{\"t\":\"typing\",\"conv\":\"c:t1\",\"by\":\"u-cem\",\"name\":\"Cem\"}", 1_000);
        state.note("c:t1", "react.full", 1_000);
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Mert\"},\"features\":[\"friends\",\"chat\"]}");
        assertTrue(state.writers("c:t1", 1_100).isEmpty());
        assertNull(state.note(1_100));
    }

    @Test
    void newMessagesGoesOverTheFirstUnreadFromOthers() {
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:1\",\"sid\":1,\"from\":\"Mert\",\"text\":\"a\"}");
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:2\",\"sid\":2,\"from\":\"Deniz\",\"text\":\"b\"}");
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:3\",\"sid\":3,\"from\":\"Ece\",\"mine\":true,\"text\":\"c\"}");
        List<OverlayState.ChatLine> lines = new ArrayList<>(state.lines("c:t1"));
        // The player's own message after them is never unread: one unread is Deniz's, not the player's.
        assertSame(line("c:t1", "i:2"), OverlayState.firstUnread(lines, 1));
        assertSame(line("c:t1", "i:1"), OverlayState.firstUnread(lines, 2));
        assertSame(line("c:t1", "i:1"), OverlayState.firstUnread(lines, 9), "more unread than kept: the top");
        assertNull(OverlayState.firstUnread(List.of(), 1));
    }
}
