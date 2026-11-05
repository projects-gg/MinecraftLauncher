package gg.projects.overlay.voice;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.lang.Dictionary;
import gg.projects.overlay.lang.Language;
import java.util.List;
import org.junit.jupiter.api.Test;

/** Screen sharing in the room as the launcher tells it (OVL-SHARE): {@code voice-share} and its notices. */
class VoiceShareTest {
    private final VoiceState state = new VoiceState();

    private boolean apply(String json) {
        return state.apply(Protocol.parse(json));
    }

    @Test
    void theHelloSaysShareAndThePressesAreTheLaunchers() {
        assertTrue(Protocol.hello("t", "1", "26.1.2", "tr_tr").toString().contains("\"share\""));
        assertFalse(Protocol.hello("t", "1", "26.1.2", "tr_tr", false).toString().contains("\"share\""),
            "the emoji picker alone has no panel");
        assertEquals("{\"t\":\"voice-share-game\"}", Protocol.voiceShareGame().toString());
        assertEquals("{\"t\":\"voice-share-stop\",\"n\":2}", Protocol.voiceShareStop(2).toString());
    }

    @Test
    void aShareFrameSaysTheButtonTheOwnSharesAndWhoIsLive() {
        assertTrue(apply("""
            {"t":"voice-share","game":"ready",
             "mine":[{"n":2,"kind":"window","live":true,"watchers":3},{"n":0,"kind":"game"}],
             "live":[{"person":"u-kaan","kind":"game"},{"kind":"screen"}]}"""));
        assertEquals("ready", state.shareGame());
        assertEquals(List.of(new VoiceState.MyShare(2, "window", true, 3)), state.myShares(), "no share without n");
        assertTrue(state.live());
        assertEquals(3, state.watchers());
        assertTrue(state.sharing("u-kaan"));
        assertFalse(state.sharing("u-x"));
        // Dim, and why; a word this build does not know shows nothing.
        apply("{\"t\":\"voice-share\",\"game\":\"off\",\"why\":\"not-allowed\"}");
        assertEquals("off", state.shareGame());
        assertEquals("not-allowed", state.shareWhy());
        assertTrue(state.myShares().isEmpty() && !state.live());
        apply("{\"t\":\"voice-share\",\"game\":\"maybe\",\"why\":\"x\"}");
        assertEquals("", state.shareGame());
        assertEquals("", state.shareWhy());
        // A share still starting is not live.
        apply("{\"t\":\"voice-share\",\"mine\":[{\"n\":1,\"kind\":\"game\"}]}");
        assertTrue(state.myShares().get(0).game());
        assertFalse(state.live());
        // The launcher going away takes it all.
        state.clear();
        assertTrue(state.myShares().isEmpty());
        assertEquals("", state.shareGame());
    }

    @Test
    void everySharingNoticeIsSaidCalmly() {
        Dictionary words = Dictionary.shared();
        for (String[] notice : new String[][] {
            {"share-refused", "off"}, {"share-refused", "unsupported"}, {"share-refused", "not-allowed"},
            {"share-refused", "limited"}, {"share-refused", "full"}, {"share-refused", "room-full"},
            {"share-refused", "budget"}, {"share-refused", "failed"}, {"share-refused", "new"},
            {"share-closed", "mod"}, {"share-closed", "budget"}, {"share-closed", "idle"}, {"share-closed", "limit"},
            {"share-closed", "game-gone"}, {"share-closed", "source-gone"}, {"share-closed", "network"},
            {"share-closed", "error"}, {"share-closed", "new"}, {"share-audio-off", ""}}) {
            VoiceState.Notice n = new VoiceState.Notice(notice[0], notice[1], "");
            String key = VoiceWords.noticeKey(n);
            assertTrue(words.has(Language.TURKISH, "projects_overlay." + key), key);
            assertTrue(words.has(Language.ENGLISH, "projects_overlay." + key), key);
            assertFalse(VoiceWords.noticeError(n), "one calm line: " + key);
        }
        assertTrue(apply("{\"t\":\"voice-notice\",\"kind\":\"share-refused\",\"reason\":\"not-allowed\"}"));
        assertEquals("share.note.not_allowed", VoiceWords.noticeKey(state.takeNotice()));
        for (String key : List.of("share.game", "share.stop", "share.live", "share.strip.watching",
            "share.mine.game", "share.mine.screen", "share.mine.window", "share.starting", "share.watchers",
            "share.resting")) {
            assertTrue(words.has(Language.TURKISH, "projects_overlay." + key), key);
            assertTrue(words.has(Language.ENGLISH, "projects_overlay." + key), key);
        }
    }
}
