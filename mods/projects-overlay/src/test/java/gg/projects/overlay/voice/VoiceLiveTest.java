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
 * "YAYINDA" and its card in the game (SHARE-LIVE, LIVE-R's frames): what each sharer shows and how many watch, who is
 * live in the crews' rooms, the request the card sends and the launcher's answer (a small PNG, locked or none).
 */
class VoiceLiveTest {
    private final VoiceState state = new VoiceState();

    private boolean apply(String json) {
        return state.apply(Protocol.parse(json));
    }

    @Test
    void theCardAsksForAShareOfARoom() {
        assertEquals("{\"t\":\"voice-thumb\",\"room\":\"f-1\",\"u\":\"u-kaan\",\"n\":2}",
            Protocol.voiceThumb("f-1", "u-kaan", 2).toString());
    }

    @Test
    void aShareSaysItsNumberWhatItShowsWhoWatchesAndItsPictureTime() {
        apply("""
            {"t":"voice-share","live":[{"person":"u-kaan","n":2,"kind":"game","watchers":2,"th":1730800060},
             {"person":"u-kaan","n":3,"kind":"screen","watchers":5},{"person":"u-ece","kind":"window"}]}""");
        assertEquals(new VoiceState.LiveShare("u-kaan", 2, "game", 2, 1730800060L, ""), state.liveShare("u-kaan"),
            "a person's first share");
        // Nobody watching and no picture yet are left out of the frame.
        assertEquals(new VoiceState.LiveShare("u-ece", 1, "window", 0, 0L, ""), state.liveShare("u-ece"));
        assertNull(state.liveShare("u-x"));
        state.clear();
        assertNull(state.liveShare("u-kaan"));
    }

    @Test
    void theCrewsRoomsSayWhoIsLiveInThem() {
        apply("""
            {"t":"voice-crews","crews":[{"crew":"c1","conv":"crew:c1","rooms":[
             {"id":"g1","general":true,"people":[{"id":"u-a","name":"A"}]},
             {"id":"r2","name":"Taş Ocağı","full":true,"people":[{"id":"u-b","name":"B","live":true}]}]}]}""");
        assertEquals("r2", state.crewLiveRoom("u-b"));
        assertNull(state.crewLiveRoom("u-a"));
        assertTrue(state.crewRoom("r2").full());
        assertNull(state.crewRoom("r9"));
    }

    @Test
    void theLaunchersAnswerIsAPictureALockOrNothing() {
        byte[] png = {(byte) 0x89, 0x50, 0x4E, 0x47, 0x01};
        String data = Base64.getEncoder().encodeToString(png);
        assertTrue(apply("{\"t\":\"voice-thumb\",\"room\":\"f-1\",\"u\":\"u-kaan\",\"n\":1,\"kind\":\"image\",\"png\":\""
            + data + "\"}"));
        VoiceState.Thumb first = state.thumb("u-kaan");
        assertTrue(first.image());
        assertArrayEquals(png, first.picture());
        // A newer picture is told apart.
        apply("{\"t\":\"voice-thumb\",\"u\":\"u-kaan\",\"kind\":\"image\",\"png\":\"" + data + "\"}");
        assertNotEquals(first.at(), state.thumb("u-kaan").at());
        apply("{\"t\":\"voice-thumb\",\"u\":\"u-ece\",\"kind\":\"locked\"}");
        assertTrue(state.thumb("u-ece").locked());
        // Broken or oversized data, an unknown word: no picture.
        apply("{\"t\":\"voice-thumb\",\"u\":\"u-a\",\"kind\":\"image\",\"png\":\"%%%\"}");
        assertFalse(state.thumb("u-a").image());
        apply("{\"t\":\"voice-thumb\",\"u\":\"u-b\",\"kind\":\"image\",\"png\":\""
            + "A".repeat(VoiceState.MAX_THUMB_CHARS + 4) + "\"}");
        assertFalse(state.thumb("u-b").image());
        apply("{\"t\":\"voice-thumb\",\"u\":\"u-c\",\"kind\":\"video\"}");
        assertEquals("none", state.thumb("u-c").kind());
        assertTrue(apply("{\"t\":\"voice-thumb\",\"kind\":\"image\"}"), "a frame without a person is taken");
        // A card that closed lets its picture go.
        state.dropThumb("u-kaan");
        assertNull(state.thumb("u-kaan"));
    }
}
