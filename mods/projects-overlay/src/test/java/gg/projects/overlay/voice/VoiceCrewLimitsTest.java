package gg.projects.overlay.voice;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import org.junit.jupiter.api.Test;

/** OVL-SOC 2 (VOICE-ROOMS): a crew room's own people limit and a full room, as crew chat's "Sesli" shows them. */
class VoiceCrewLimitsTest {
    private final VoiceState state = new VoiceState();

    private void apply(String json) {
        assertTrue(state.apply(Protocol.parse(json)));
    }

    @Test
    void aRoomsOwnLimitAndAFullRoomAreKept() {
        apply("{\"t\":\"voice\",\"on\":true}");
        apply("""
            {"t":"voice-crews","crews":[{"crew":"t-gk","conv":"c:t-gk","rooms":[
              {"id":"c0123456789abcdef","general":true,"people":[{"id":"u-kaan","name":"Kaan_07"}]},
              {"id":"c1111111111111111","name":"Oda 2","people":[{"id":"u-4","name":"KumSaati"}],"max":1,"full":true},
              {"id":"c2222222222222222","name":"Taktik","max":500}]}]}""");
        VoiceState.CrewRooms gk = state.crewRooms("c:t-gk");
        VoiceState.CrewRoom general = gk.rooms().get(0);
        assertEquals(0, general.max(), "no limit of its own: no count");
        assertFalse(general.full());
        VoiceState.CrewRoom two = gk.rooms().get(1);
        assertEquals(1, two.max());
        assertTrue(two.full());
        assertEquals(99, gk.rooms().get(2).max(), "held to two digits");
    }
}
