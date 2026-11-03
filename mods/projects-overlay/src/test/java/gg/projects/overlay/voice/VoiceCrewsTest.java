package gg.projects.overlay.voice;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.List;
import org.junit.jupiter.api.Test;

/** OVL-SOC: the crews' voice rooms (V21) as the mod mirrors them for crew chat's "Sesli". */
class VoiceCrewsTest {
    private final VoiceState state = new VoiceState();

    private void apply(String json) {
        assertTrue(state.apply(Protocol.parse(json)));
    }

    private static final String CREWS = """
        {"t":"voice-crews","crews":[
          {"crew":"t-gk","conv":"c:t-gk","add":true,"rooms":[
            {"id":"c0123456789abcdef","general":true,"people":[{"id":"u-kaan","name":"Kaan_07"},{"id":"u-x"}]},
            {"id":"c1111111111111111","name":"Oda 2","people":[{"id":"f-ayse","name":"Ayşe"},{"name":"no id"}]},
            {"name":"no id"}]},
          {"crew":"t-empty","conv":"c:t-empty","rooms":[{"id":"gt-empty","general":true}]},
          {"conv":"c:no-crew","rooms":[]}]}""";

    @Test
    void theCrewsRoomsComeInTheLaunchersOrderAndShowOnlyWhileVoiceChatIsOn() {
        apply(CREWS);
        assertNull(state.crewRooms("c:t-gk"), "voice chat not on yet: no strip");
        apply("{\"t\":\"voice\",\"on\":true}");
        VoiceState.CrewRooms gk = state.crewRooms("c:t-gk");
        assertEquals("t-gk", gk.crew());
        assertTrue(gk.add(), "\"Yeni oda\"");
        assertEquals(List.of("c0123456789abcdef", "c1111111111111111"),
            gk.rooms().stream().map(VoiceState.CrewRoom::id).toList(), "a room without an id is no room");
        VoiceState.CrewRoom general = gk.rooms().get(0);
        assertTrue(general.general());
        assertNull(general.name(), "the mod says \"Genel\" itself");
        assertEquals(List.of(new VoiceState.Named("u-kaan", "Kaan_07"), new VoiceState.Named("u-x", "")),
            general.people());
        assertEquals("Oda 2", gk.rooms().get(1).name());
        assertEquals(1, gk.rooms().get(1).people().size(), "someone without an id is left out");
        VoiceState.CrewRooms empty = state.crewRooms("c:t-empty");
        assertFalse(empty.add());
        assertEquals("gt-empty", empty.rooms().get(0).id());
        assertNull(state.crewRooms("c:no-crew"), "a crew needs its id");
        assertNull(state.crewRooms("d:f-ayse:me"));
        // The player's own room lights its pill.
        assertFalse(state.inRoom("c1111111111111111"));
        apply("""
            {"t":"voice","on":true,"room":{"id":"c1111111111111111","kind":"crew","name":"Oda 2","phase":"live",
              "max":15,"me":{"role":"member"},"people":[{"id":"f-ayse","name":"Ayşe","role":"member","volume":100}]}}""");
        assertTrue(state.inRoom("c1111111111111111"));
        assertEquals(2, state.crewRooms("c:t-gk").rooms().size(), "a whole voice frame leaves the crews as they were");
        // An empty list: the rooms are gone; the launcher going away takes them too.
        apply("{\"t\":\"voice-crews\",\"crews\":[]}");
        assertNull(state.crewRooms("c:t-gk"));
        apply(CREWS);
        state.clear();
        assertNull(state.crewRooms("c:t-gk"));
    }

    @Test
    void crewsAreHeldToTheirLimits() {
        StringBuilder many = new StringBuilder("{\"t\":\"voice-crews\",\"crews\":[");
        for (int i = 0; i < 30; i++) {
            many.append(i == 0 ? "" : ",").append("{\"crew\":\"t").append(i).append("\",\"conv\":\"c:t").append(i)
                .append("\",\"rooms\":[");
            for (int r = 0; r < 20; r++) {
                many.append(r == 0 ? "" : ",").append("{\"id\":\"r").append(r).append("\"}");
            }
            many.append("]}");
        }
        many.append("]}");
        apply("{\"t\":\"voice\",\"on\":true}");
        apply(many.toString());
        assertEquals(VoiceState.MAX_CREW_ROOMS, state.crewRooms("c:t0").rooms().size());
        assertNull(state.crewRooms("c:t" + VoiceState.MAX_CREWS));
    }

    @Test
    void roomsFullHasItsOwnSentence() {
        assertEquals("voice.refused.rooms_full", VoiceWords.refusalKey("rooms-full"));
    }
}
