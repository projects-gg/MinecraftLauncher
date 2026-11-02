package gg.projects.overlay.voice;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.List;
import java.util.Set;
import org.junit.jupiter.api.Test;

/** Voice chat's frames (docs/tasks/ovl2-protocol.md, rows V) as the mod mirrors them. */
class VoiceStateTest {
    private final VoiceState state = new VoiceState();

    private boolean apply(String json) {
        return state.apply(Protocol.parse(json));
    }

    /** A friend room: Kaan_07 leads and speaks, someone the lists do not know is muted for the player. */
    private static final String ROOM = """
        {"t":"voice","on":true,"sound":true,
         "room":{"id":"f0123456789abcdef","kind":"friend","phase":"live","max":4,
           "me":{"role":"moderator","deaf":true},
           "people":[
             {"id":"u-kaan","name":"Kaan_07","role":"leader","speaking":true,"volume":250},
             {"id":"u-x","role":"member","mute":true,"quality":"weak","volume":60,"mutedForMe":true,
              "can":{"mute":true,"kick":true}},
             {"name":"no id"}],
           "ringing":[{"id":"f-can","name":"Can"}]},
         "calling":{"id":"f-can","name":"Can"},
         "rings":[{"room":"c0123456789abcdef","from":"f-ayse","name":"Ayşe","with":["Kaan_07",""],
                   "until":2000}]}""";

    private static final VoiceState.Words WORDS = new VoiceState.Words() {
        @Override
        public String general() {
            return "Genel";
        }

        @Override
        public String justYou() {
            return "Yalnız sen";
        }

        @Override
        public String someone() {
            return "Bir oyuncu";
        }

        @Override
        public String more(String names, int count) {
            return names + " ve " + count + " kişi daha";
        }
    };

    @Test
    void theRoomComesWholeAndHeldToItsLimits() {
        assertFalse(state.heard());
        assertTrue(apply(ROOM));
        assertTrue(state.heard() && state.on() && state.sound());
        VoiceState.Room room = state.room();
        assertEquals("f0123456789abcdef", room.id());
        assertFalse(room.crewRoom() || room.joining() || room.reconnecting());
        assertTrue(room.me().deaf() && room.me().micOff() && !room.me().weak());
        assertEquals(2, room.people().size(), "a person without an id is no one");
        VoiceState.Person kaan = room.person("u-kaan");
        assertTrue(kaan.leader() && kaan.speaking());
        assertEquals(200, kaan.volume(), "held to the slider's top");
        VoiceState.Person x = room.person("u-x");
        assertEquals("", x.name());
        assertTrue(x.mute() && x.weak() && x.mutedForMe() && x.canMute() && x.canKick() && !x.canMod());
        assertEquals(List.of(new VoiceState.Named("f-can", "Can")), room.ringing());
        assertEquals(new VoiceState.Named("f-can", "Can"), state.calling());
        assertEquals(Set.of("u-kaan"), state.speakers());
        assertEquals(1, state.rings(1_000).size());
        assertEquals(List.of("Kaan_07", ""), state.rings(1_000).get(0).with());
        assertTrue(state.rings(2_000).isEmpty(), "past its end it rings no more");
        assertEquals("Ayşe", state.ring("c0123456789abcdef").name());
        // Out of the room: everything goes with it.
        int before = state.revision();
        apply("{\"t\":\"voice\"}");
        assertTrue(state.revision() > before);
        assertNull(state.room());
        assertFalse(state.on() || state.sound());
        assertTrue(state.rings(0).isEmpty() && state.speakers().isEmpty());
        assertFalse(state.heard(), "voice chat off: nothing of it shows, not even its settings row");
        apply("{\"t\":\"voice\",\"on\":true}");
        assertTrue(state.heard(), "usable");
    }

    @Test
    void speakersChangeOnTheirOwnAndOnlyInTheRoom() {
        apply("{\"t\":\"voice-speaking\",\"ids\":[\"u-x\"],\"me\":true}");
        assertTrue(state.speakers().isEmpty(), "no room yet");
        apply(ROOM);
        apply("{\"t\":\"voice-speaking\",\"ids\":[\"u-x\",\"u-gone\",\"" + "u".repeat(80) + "\"],\"me\":true}");
        assertEquals(Set.of("u-x"), state.speakers());
        assertTrue(state.room().me().talking());
        assertFalse(state.room().person("u-kaan").speaking());
        apply("{\"t\":\"voice-speaking\",\"ids\":[],\"me\":false}");
        assertTrue(state.speakers().isEmpty());
        assertFalse(state.room().me().talking());
    }

    @Test
    void noticesAndResultsWaitToBeSaid() {
        assertTrue(apply("{\"t\":\"voice-notice\",\"kind\":\"left\",\"reason\":\"no-answer\",\"name\":\"Can\"}"));
        apply("{\"t\":\"voice-notice\"}");
        apply("{\"t\":\"voice-result\",\"act\":\"call\",\"ok\":false,\"reason\":\"unreachable\",\"who\":\"f-can\"}");
        assertEquals(new VoiceState.Notice("left", "no-answer", "Can"), state.takeNotice());
        assertNull(state.takeNotice(), "a notice without a kind says nothing");
        assertEquals(new VoiceState.Result("call", false, "unreachable", "f-can"), state.takeResult());
        assertNull(state.takeResult());
        for (int i = 0; i < 6; i++) {
            apply("{\"t\":\"voice-notice\",\"kind\":\"muted\",\"name\":\"" + i + "\"}");
        }
        assertEquals("2", state.takeNotice().name(), "a burst says its newest few");
        assertFalse(apply("{\"t\":\"friend\",\"id\":\"f-ayse\"}"), "not voice chat's");
    }

    @Test
    void theLauncherGoingAwayTakesEverything() {
        apply(ROOM);
        apply("{\"t\":\"voice-notice\",\"kind\":\"muted\"}");
        state.clear();
        assertFalse(state.heard() || state.on());
        assertNull(state.room());
        assertNull(state.takeNotice());
    }

    @Test
    void whatAFriendsMenuOffers() {
        assertEquals(VoiceState.Offer.NONE, state.offer("f-ayse", "ready"), "voice chat never spoke");
        apply("{\"t\":\"voice\",\"on\":true}");
        assertEquals(VoiceState.Offer.CALL, state.offer("f-ayse", "ready"));
        assertEquals(VoiceState.Offer.CALL, state.offer("f-ayse", "in"), "a call still rings in a room");
        assertEquals(VoiceState.Offer.NONE, state.offer("f-ayse", null), "they cannot be called");
        apply(ROOM);
        assertEquals(VoiceState.Offer.RINGING, state.offer("f-can", "ready"), "the player's call rings at them");
        assertEquals(VoiceState.Offer.NONE, state.offer("u-kaan", "ready"), "in the room already");
        // Four places: the player, two people and Can's ringing invite.
        assertEquals(VoiceState.Offer.FULL, state.offer("f-ayse", "ready"));
        apply(ROOM.replace("\"max\":4", "\"max\":10"));
        assertEquals(VoiceState.Offer.INVITE, state.offer("f-ayse", "ready"));
        apply(ROOM.replace("\"kind\":\"friend\"", "\"kind\":\"crew\""));
        assertEquals(VoiceState.Offer.NONE, state.offer("f-ayse", "ready"), "a crew room's invites are the launcher's");
        apply(ROOM.replace("\"on\":true", "\"on\":false"));
        assertEquals(VoiceState.Offer.NONE, state.offer("f-ayse", "ready"), "voice chat switched off");
    }

    @Test
    void volumeStepsKeepTheOwnValueAndThePagesTop() {
        assertEquals(List.of(25, 50, 75, 100, 125, 150, 200), VoiceState.volumeChoices(100, 200));
        assertEquals(List.of(25, 50, 63, 75, 100, 125, 150, 200), VoiceState.volumeChoices(63, 200));
        assertEquals(List.of(0, 25, 50, 75, 100, 125, 150, 200), VoiceState.volumeChoices(0, 200));
        assertEquals(List.of(25, 50, 75, 100, 125, 150, 180, 200), VoiceState.volumeChoices(180, 200));
        // Through <audio> elements the page goes to 100 at most.
        assertEquals(List.of(25, 50, 75, 90, 100), VoiceState.volumeChoices(90, 100));
        assertEquals(List.of(25, 50, 75, 100), VoiceState.volumeChoices(150, 100));
        assertEquals(200, state.volumeMax(), "before the launcher said");
        apply("{\"t\":\"voice\",\"on\":true,\"volumeMax\":100}");
        assertEquals(100, state.volumeMax());
        apply("{\"t\":\"voice\",\"on\":true,\"volumeMax\":5}");
        assertEquals(100, state.volumeMax(), "never below as they come");
        apply("{\"t\":\"voice\",\"on\":true}");
        assertEquals(200, state.volumeMax());
    }

    @Test
    void aRoomsTitleIsTheLaunchers() {
        apply(ROOM);
        VoiceState.Room room = state.room();
        assertEquals("Kaan_07, Bir oyuncu", VoiceState.title(room, state.calling(), WORDS));
        apply(ROOM.replace("\"phase\":\"live\"", "\"phase\":\"live\",\"name\":\"Gece\""));
        assertEquals("Gece", VoiceState.title(state.room(), null, WORDS));
        apply(ROOM.replace("\"phase\":\"live\"", "\"phase\":\"joining\",\"general\":true,\"name\":\"Genel\""));
        assertEquals("Genel", VoiceState.title(state.room(), null, WORDS));
        assertTrue(state.room().joining());
        apply("{\"t\":\"voice\",\"on\":true,\"room\":{\"id\":\"f1\",\"people\":[]},\"calling\":{\"id\":\"f-can\","
            + "\"name\":\"Can\"}}");
        assertEquals("Can", VoiceState.title(state.room(), state.calling(), WORDS), "alone, waiting for the friend");
        assertEquals("Yalnız sen", VoiceState.title(state.room(), null, WORDS));
        assertEquals("A, B ve 3 kişi daha", VoiceState.names(List.of("A", "B", "C", "D", "E"), WORDS));
        assertEquals("A, B, C", VoiceState.names(List.of("A", "B", "C"), WORDS));
    }
}
