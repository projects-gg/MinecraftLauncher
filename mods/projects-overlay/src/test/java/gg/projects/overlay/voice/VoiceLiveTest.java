package gg.projects.overlay.voice;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.Base64;
import org.junit.jupiter.api.Test;

/**
 * "YAYINDA" and its card in the game (SHARE-LIVE): what each sharer shows and how many watch, the request the card
 * sends and the launcher's answer (a small JPEG, locked or none).
 */
class VoiceLiveTest {
    private final VoiceState state = new VoiceState();

    private boolean apply(String json) {
        return state.apply(Protocol.parse(json));
    }

    @Test
    void theHelloSaysLiveAndTheCardAsksByPerson() {
        assertTrue(Protocol.hello("t", "1", "26.1.2", "tr_tr").toString().contains("\"live\""));
        assertFalse(Protocol.hello("t", "1", "26.1.2", "tr_tr", false).toString().contains("\"live\""),
            "the emoji picker alone has no panel");
        assertEquals("{\"t\":\"voice-share-thumb\",\"person\":\"u-kaan\"}",
            Protocol.voiceShareThumb("u-kaan").toString());
    }

    @Test
    void aShareSaysWhatItShowsWhoWatchesAndItsPictureTime() {
        apply("""
            {"t":"voice-share","live":[{"person":"u-kaan","kind":"game","w":2,"th":1730800060},
             {"person":"u-kaan","kind":"screen","w":5},{"person":"u-ece","kind":"window"}]}""");
        assertEquals(new VoiceState.LiveShare("u-kaan", "game", 2, 1730800060L), state.liveShare("u-kaan"),
            "a person's first share");
        // An older launcher says neither: the card leaves them out.
        assertEquals(new VoiceState.LiveShare("u-ece", "window", -1, 0L), state.liveShare("u-ece"));
        assertNull(state.liveShare("u-x"));
        state.clear();
        assertNull(state.liveShare("u-kaan"));
    }

    @Test
    void theLaunchersAnswerIsAPictureALockOrNothing() {
        byte[] jpg = {(byte) 0xFF, (byte) 0xD8, (byte) 0xFF, 0x01, 0x02};
        String data = Base64.getEncoder().encodeToString(jpg);
        assertTrue(apply("{\"t\":\"voice-share-thumb\",\"person\":\"u-kaan\",\"kind\":\"image\",\"jpg\":\"" + data
            + "\"}"));
        VoiceState.Thumb first = state.thumb("u-kaan");
        assertTrue(first.image());
        assertArrayEquals(jpg, first.jpg());
        // A newer picture is told apart.
        apply("{\"t\":\"voice-share-thumb\",\"person\":\"u-kaan\",\"kind\":\"image\",\"jpg\":\"" + data + "\"}");
        assertNotEquals(first.at(), state.thumb("u-kaan").at());
        apply("{\"t\":\"voice-share-thumb\",\"person\":\"u-ece\",\"kind\":\"locked\",\"why\":\"private\"}");
        assertTrue(state.thumb("u-ece").locked());
        assertEquals("private", state.thumb("u-ece").why());
        // Broken or oversized data, an unknown word: no picture.
        apply("{\"t\":\"voice-share-thumb\",\"person\":\"u-a\",\"kind\":\"image\",\"jpg\":\"%%%\"}");
        assertFalse(state.thumb("u-a").image());
        apply("{\"t\":\"voice-share-thumb\",\"person\":\"u-b\",\"kind\":\"image\",\"jpg\":\""
            + "A".repeat(VoiceState.MAX_THUMB_CHARS + 4) + "\"}");
        assertFalse(state.thumb("u-b").image());
        apply("{\"t\":\"voice-share-thumb\",\"person\":\"u-c\",\"kind\":\"video\"}");
        assertEquals("none", state.thumb("u-c").kind());
        assertTrue(apply("{\"t\":\"voice-share-thumb\",\"kind\":\"image\"}"), "a frame without a person is taken");
        // A card that closed lets its picture go.
        state.dropThumb("u-kaan");
        assertNull(state.thumb("u-kaan"));
    }
}
