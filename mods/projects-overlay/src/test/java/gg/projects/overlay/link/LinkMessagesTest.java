package gg.projects.overlay.link;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

class LinkMessagesTest {
    static final String NONCE = "iAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB";

    @Test
    void readsTheServersAskAndQuestion() {
        assertInstanceOf(LinkMessages.Ask.class, LinkMessages.read("{\"t\":\"ask\",\"v\":1}"));
        // A later version of the ask is still an ask (unknown fields are skipped).
        assertInstanceOf(LinkMessages.Ask.class, LinkMessages.read("{\"t\":\"ask\",\"v\":2,\"more\":true}"));
        LinkMessages.FromServer read = LinkMessages.read(
            "{\"t\":\"question\",\"v\":1,\"token\":\"Ab3_x-Yz9Q\",\"displayName\":\"Ayşe\",\"username\":\"ayse\"}");
        LinkMessages.Question question = assertInstanceOf(LinkMessages.Question.class, read);
        assertEquals("Ab3_x-Yz9Q", question.token());
        assertEquals("Ayşe", question.displayName());
        assertEquals("ayse", question.username());
        assertFalse(question.toString().contains("Ab3"), "the token never goes to a log");
    }

    @Test
    void namesArePlainAndShortAndTheDisplayNameFallsBackToTheUserName() {
        LinkMessages.Question question = (LinkMessages.Question) LinkMessages.read(
            "{\"t\":\"question\",\"token\":\"Ab3_x-Yz9Q\",\"displayName\":\"K\\u202eötü\\u0000\","
                + "\"username\":\"" + "a".repeat(40) + "\"}");
        assertEquals("Kötü", question.displayName(), "no bidi overrides or controls");
        assertEquals(LinkMessages.MAX_NAME, question.username().length());
        LinkMessages.Question bare = (LinkMessages.Question) LinkMessages.read(
            "{\"t\":\"question\",\"token\":\"Ab3_x-Yz9Q\",\"username\":\"ayse\"}");
        assertEquals("ayse", bare.displayName());
    }

    @Test
    void anythingElseIsNotTaken() {
        for (String json : new String[] {
            null, "", "[]", "{", "\"ask\"", "{\"t\":\"intent\"}", "{\"t\":7}",
            // A question without a usable token or user name.
            "{\"t\":\"question\",\"token\":\"short\",\"username\":\"ayse\"}",
            "{\"t\":\"question\",\"token\":\"has space in it\",\"username\":\"ayse\"}",
            "{\"t\":\"question\",\"token\":\"Ab3_x-Yz9Q\",\"username\":\"\"}",
            // Longer than ProBotLinker's 1 KB.
            "{\"t\":\"ask\",\"pad\":\"" + "x".repeat(1100) + "\"}",
        }) {
            assertInstanceOf(LinkMessages.Other.class, LinkMessages.read(json), String.valueOf(json));
        }
    }

    @Test
    void writesTheGamesAnswersAsProBotLinkerReadsThem() {
        JsonObject intent = JsonParser.parseString(LinkMessages.intent(NONCE)).getAsJsonObject();
        assertEquals("intent", intent.get("t").getAsString());
        assertEquals(NONCE, intent.get("nonce").getAsString());
        assertEquals(1, intent.get("ui").getAsInt(), "this game draws the question itself");
        assertEquals("{\"t\":\"pending\"}", LinkMessages.pending());
        assertEquals("{\"t\":\"none\"}", LinkMessages.none());
        assertEquals("{\"t\":\"answer\",\"token\":\"Ab3_x-Yz9Q\",\"answer\":\"yes\"}",
            LinkMessages.answer("Ab3_x-Yz9Q", true));
        assertEquals("{\"t\":\"answer\",\"token\":\"Ab3_x-Yz9Q\",\"answer\":\"no\"}",
            LinkMessages.answer("Ab3_x-Yz9Q", false));
        assertTrue(LinkMessages.nonce(NONCE));
        assertFalse(LinkMessages.nonce("i" + "A".repeat(31)));
        assertFalse(LinkMessages.nonce("x" + "A".repeat(32)));
        assertFalse(LinkMessages.nonce(null));
    }
}
