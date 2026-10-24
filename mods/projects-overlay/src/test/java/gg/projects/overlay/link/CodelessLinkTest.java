package gg.projects.overlay.link;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

class CodelessLinkTest {
    private static final String NONCE = LinkMessagesTest.NONCE;
    private static final long WALL = 1_800_000_000_000L;
    /** A permit with 25 minutes left. */
    private static final long EXPIRES = WALL / 1000 + 25 * 60;
    private static final LinkMessages.Question QUESTION = new LinkMessages.Question("Ab3_x-Yz9Q", "Ayşe", "ayse");

    private final List<String> server = new ArrayList<>();
    private final List<JsonObject> launcher = new ArrayList<>();
    private CodelessLink link;

    @BeforeEach
    void setUp() {
        link = new CodelessLink(new CodelessLink.Out() {
            @Override
            public void toServer(String json) {
                server.add(json);
            }

            @Override
            public void toLauncher(JsonObject frame) {
                launcher.add(frame);
            }
        });
    }

    @Test
    void theLaunchersPermitGoesStraightOnToTheServerThatAsked() {
        link.onAsk(0, true, "Ahmet", true);
        assertEquals(List.of(LinkMessages.pending()), server);
        assertEquals(1, launcher.size());
        assertEquals("link-want", launcher.get(0).get("t").getAsString());
        assertEquals("Ahmet", launcher.get(0).get("name").getAsString());

        link.onOffer(NONCE, "ahmet", EXPIRES, WALL);
        assertEquals(List.of(LinkMessages.pending(), LinkMessages.intent(NONCE)), server);
        // Not kept: a second offer (nobody asked) goes nowhere.
        link.onOffer(NONCE, "Ahmet", EXPIRES, WALL);
        link.onClear();
        assertEquals(2, server.size());
    }

    @Test
    void noPermitMeansNoneAndNothingGoesToAnotherServer() {
        // The launcher cannot vouch (not signed in, Studio has it off, an old launcher): none at once.
        link.onAsk(0, true, "Ahmet", false);
        assertEquals(List.of(LinkMessages.none()), server);
        assertTrue(launcher.isEmpty());
        // Not Projects: not even "none".
        link.onAsk(0, false, "Ahmet", true);
        assertEquals(1, server.size());
        assertTrue(launcher.isEmpty());

        // The launcher says it has none.
        link.onAsk(0, true, "Ahmet", true);
        link.onClear();
        assertEquals(LinkMessages.none(), server.get(server.size() - 1));
    }

    @Test
    void aPermitForAnotherAccountOrAboutToRunOutIsNotHandedOn() {
        link.onAsk(0, true, "Ahmet", true);
        link.onOffer(NONCE, "Mehmet", EXPIRES, WALL);
        assertEquals(LinkMessages.none(), server.get(1));

        link.onAsk(0, true, "Ahmet", true);
        link.onOffer(NONCE, "Ahmet", WALL / 1000 + 2 * 60, WALL);
        assertEquals(LinkMessages.none(), server.get(3), "two minutes left: ProBot and the player need longer");

        link.onAsk(0, true, "Ahmet", true);
        link.onOffer("not-a-permit", "Ahmet", EXPIRES, WALL);
        assertEquals(LinkMessages.none(), server.get(5));
    }

    @Test
    void aLauncherThatDoesNotAnswerInTimeMeansNone() {
        link.onAsk(1_000, true, "Ahmet", true);
        // A second ask while the first waits is not answered twice.
        link.onAsk(2_000, true, "Ahmet", true);
        link.tick(1_000 + CodelessLink.LAUNCHER_WAIT_MS - 1);
        assertEquals(1, server.size());
        link.tick(1_000 + CodelessLink.LAUNCHER_WAIT_MS);
        assertEquals(List.of(LinkMessages.pending(), LinkMessages.none()), server);
        assertEquals(1, launcher.size());
        // A late offer goes nowhere.
        link.onOffer(NONCE, "Ahmet", EXPIRES, WALL);
        assertEquals(2, server.size());
    }

    @Test
    void theQuestionWaitsForAFreeScreenAndIsAnsweredOnce() {
        link.onQuestion(0, QUESTION, true);
        assertNull(link.takeQuestion(100, false), "the chat is open");
        assertNull(link.takeQuestion(200, true), "free only just now");
        assertNull(link.takeQuestion(200 + CodelessLink.SETTLE_MS - 1, true));
        LinkMessages.Question shown = link.takeQuestion(200 + CodelessLink.SETTLE_MS, true);
        assertSame(QUESTION, shown);
        assertSame(QUESTION, link.shown());
        assertNull(link.takeQuestion(5_000, true), "once");
        // Another question while one is on screen is not taken.
        link.onQuestion(5_000, new LinkMessages.Question("Zz9_x-Yz9Q", "Biri", "biri"), true);
        assertNull(link.takeQuestion(9_000, true));

        link.answer(shown, true);
        assertEquals(List.of(LinkMessages.answer("Ab3_x-Yz9Q", true)), server);
        assertNull(link.shown());
        link.answer(shown, false);
        assertEquals(1, server.size(), "answered once");
    }

    @Test
    void eachQuestionWaitsForItsOwnFreeScreen() {
        link.onQuestion(0, QUESTION, true);
        LinkMessages.Question first = take();
        link.dismissed(first);
        // The next question, much later: the screen must again be free for the whole settle, from now on.
        link.onQuestion(60_000, QUESTION, true);
        assertNull(link.takeQuestion(60_000, true));
        assertNull(link.takeQuestion(60_000 + CodelessLink.SETTLE_MS - 1, true));
        assertSame(QUESTION, link.takeQuestion(60_000 + CodelessLink.SETTLE_MS, true));
    }

    @Test
    void escapeLeavingAndWaitingTooLongSendNothing() {
        link.onQuestion(0, QUESTION, true);
        LinkMessages.Question shown = take();
        link.dismissed(shown);
        link.answer(shown, false);
        assertTrue(server.isEmpty(), "Escape is not now, and the closed question takes no answer");

        link.onQuestion(0, QUESTION, true);
        shown = take();
        link.onLeave();
        assertNull(link.shown());
        link.answer(shown, true);
        assertTrue(server.isEmpty(), "the question means nothing after leaving the server");

        // Another screen stays open for the whole wait: the question is let go.
        link.onQuestion(0, QUESTION, true);
        link.takeQuestion(10, false);
        link.tick(CodelessLink.SHOW_WAIT_MS);
        assertNull(link.takeQuestion(CodelessLink.SHOW_WAIT_MS + 1_000, true));
        assertNull(link.takeQuestion(CodelessLink.SHOW_WAIT_MS + 2_000, true));
        // Only from Projects.
        link.onQuestion(0, QUESTION, false);
        link.takeQuestion(0, true);
        assertNull(link.takeQuestion(CodelessLink.SETTLE_MS, true));
        assertTrue(server.isEmpty());
    }

    @Test
    void leavingEndsAnAskWithoutAnAnswer() {
        link.onAsk(0, true, "Ahmet", true);
        link.onLeave();
        link.onOffer(NONCE, "Ahmet", EXPIRES, WALL);
        assertEquals(List.of(LinkMessages.pending()), server, "the old connection's server is gone");
        // The next connection may ask again.
        link.onAsk(100, true, "Ahmet", true);
        assertEquals(2, launcher.size());
    }

    private LinkMessages.Question take() {
        link.takeQuestion(0, true);
        LinkMessages.Question shown = link.takeQuestion(CodelessLink.SETTLE_MS, true);
        assertNotNull(shown);
        return shown;
    }
}
