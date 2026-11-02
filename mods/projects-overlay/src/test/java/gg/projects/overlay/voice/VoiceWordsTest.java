package gg.projects.overlay.voice;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.lang.Dictionary;
import gg.projects.overlay.lang.Language;
import java.util.List;
import org.junit.jupiter.api.Test;

/** Voice chat's sentences: every notice and refusal the launcher sends has words in both languages. */
class VoiceWordsTest {
    private static void hasWords(String key) {
        Dictionary words = Dictionary.shared();
        assertTrue(words.has(Language.TURKISH, "projects_overlay." + key), key);
        assertTrue(words.has(Language.ENGLISH, "projects_overlay." + key), key);
    }

    @Test
    void everyNoticeIsSaid() {
        for (String reason : List.of("closed", "alone", "no-answer", "elsewhere", "kicked", "crew", "idle",
            "unavailable", "failed", "something-new")) {
            hasWords(VoiceWords.noticeKey(new VoiceState.Notice("left", reason, "")));
        }
        assertEquals("voice.left.no_answer_from",
            VoiceWords.noticeKey(new VoiceState.Notice("left", "no-answer", "KorAtesi")));
        for (String kind : List.of("moved", "muted", "mic-changed")) {
            hasWords(VoiceWords.noticeKey(new VoiceState.Notice(kind, "", "")));
        }
        hasWords(VoiceWords.noticeKey(new VoiceState.Notice("mic", "denied", "")));
        hasWords(VoiceWords.noticeKey(new VoiceState.Notice("mic", "missing", "")));
        assertNull(VoiceWords.noticeKey(new VoiceState.Notice("mic", "ok", "")));
        assertNull(VoiceWords.noticeKey(new VoiceState.Notice("later", "", "")), "a newer notice says nothing");
        assertTrue(VoiceWords.noticeError(new VoiceState.Notice("left", "kicked", "")));
        assertFalse(VoiceWords.noticeError(new VoiceState.Notice("left", "alone", "")));
    }

    @Test
    void everyRefusalIsSaid() {
        for (String reason : List.of("off", "unreachable", "rate-limited", "account-limited", "staff-muted",
            "room-full", "rooms-full", "room-gone", "kicked", "not-member", "not-allowed", "not-found",
            "invalid-name", "name-rejected", "failed", "something-new")) {
            hasWords(VoiceWords.refusalKey(reason));
        }
        assertNull(VoiceWords.refusalKey("nothing"), "nothing to do says nothing");
        assertEquals("voice.refused.failed", VoiceWords.refusalKey(null));
        assertEquals("voice.refused.not_member", VoiceWords.refusalKey("not-member"), "the launcher's own sentence");
    }

    @Test
    void theRoomsAndRingsWordsAreThere() {
        for (String key : List.of("voice.title", "voice.someone", "voice.just_you", "voice.names.more",
            "voice.general", "voice.ring.verb", "voice.ring.calling", "voice.ring.others", "voice.ring.join",
            "voice.ring.decline", "voice.call", "voice.invite", "voice.ringing", "voice.ringing_name",
            "voice.cancel_call", "voice.cancel", "voice.room.mic", "voice.room.deafen", "voice.room.leave", "voice.room.mute_mic",
            "voice.room.unmute_mic", "voice.room.deafen_off", "voice.room.leader", "voice.room.moderator",
            "voice.room.you", "voice.room.crew_of", "voice.room.joining", "voice.room.reconnecting",
            "voice.room.open", "voice.room.people", "voice.person.volume", "voice.person.mute_for_me",
            "voice.person.mute_for_all", "voice.person.unmute_for_all", "voice.person.kick",
            "voice.person.kick.ask", "voice.person.kicked", "voice.person.narration", "voice.muted_by_mod",
            "settings.voice.speakers")) {
            hasWords(key);
        }
    }
}
