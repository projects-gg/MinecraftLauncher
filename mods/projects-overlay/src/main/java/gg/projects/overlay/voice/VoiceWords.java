package gg.projects.overlay.voice;

import gg.projects.overlay.ui.Texts;
import java.util.ArrayList;
import java.util.List;

/**
 * Voice chat's words in the overlay's language (VOICE-O; the launcher's tr-voice.ts and en-voice.ts): a room's title,
 * names in a line, and the sentence for a notice or a press that did not go.
 */
public final class VoiceWords {
    /** The words {@link VoiceState}'s titles need. */
    public static final VoiceState.Words WORDS = new VoiceState.Words() {
        @Override
        public String general() {
            return Texts.str("voice.general");
        }

        @Override
        public String justYou() {
            return Texts.str("voice.just_you");
        }

        @Override
        public String someone() {
            return Texts.str("voice.someone");
        }

        @Override
        public String more(String names, int count) {
            return Texts.str("voice.names.more", names, count);
        }
    };

    private VoiceWords() {
    }

    /** "Kaan_07, Bir oyuncu", "A, B ve 3 kişi daha". */
    public static String names(List<String> names) {
        List<String> shown = new ArrayList<>(names.size());
        for (String name : names) {
            shown.add(VoiceState.shown(name, WORDS));
        }
        return VoiceState.names(shown, WORDS);
    }

    /** A notice's sentence, or null for one this build does not say. */
    public static String notice(VoiceState.Notice notice) {
        String key = noticeKey(notice);
        if (key == null) {
            return null;
        }
        return key.equals("voice.left.no_answer_from") ? Texts.str(key, notice.name()) : Texts.str(key);
    }

    /** {@code lang} key of a notice's sentence (pure, for the tests), or null. */
    public static String noticeKey(VoiceState.Notice notice) {
        return switch (notice.kind()) {
            case "left" -> switch (notice.reason()) {
                case "closed" -> "voice.left.closed";
                case "alone" -> "voice.left.alone";
                case "no-answer" -> notice.name().isEmpty() ? "voice.left.no_answer" : "voice.left.no_answer_from";
                case "elsewhere" -> "voice.left.elsewhere";
                case "kicked" -> "voice.left.kicked";
                case "crew" -> "voice.left.crew";
                case "idle" -> "voice.left.idle";
                case "failed" -> "voice.left.failed";
                default -> "voice.left.unavailable";
            };
            case "moved" -> "voice.moved";
            case "muted" -> "voice.muted_by_mod";
            case "mic" -> switch (notice.reason()) {
                case "denied" -> "voice.mic.denied";
                case "missing" -> "voice.mic.missing";
                default -> null;
            };
            case "mic-changed" -> "voice.mic.changed";
            // OVL-SHARE: the player's own screen share (the launcher's share notices, its words).
            case "share-refused" -> switch (notice.reason()) {
                case "unsupported" -> "share.note.unsupported";
                case "not-allowed" -> "share.note.not_allowed";
                case "limited" -> "share.note.limited";
                case "full" -> "share.note.full";
                case "room-full" -> "share.note.room_full";
                case "failed" -> "share.note.failed";
                default -> "share.note.off";
            };
            case "share-closed" -> switch (notice.reason()) {
                case "mod" -> "share.note.mod";
                case "budget", "limit" -> "share.note.off";
                case "idle" -> "share.note.idle";
                case "game-gone" -> "share.note.game_gone";
                case "source-gone" -> "share.note.source_gone";
                case "network" -> "share.note.network";
                default -> "share.note.stopped";
            };
            case "share-audio-off" -> "share.note.audio_off";
            default -> null;
        };
    }

    /** Whether a notice tells of something that went wrong (said in the error colour). */
    public static boolean noticeError(VoiceState.Notice notice) {
        return switch (notice.kind()) {
            case "left" -> switch (notice.reason()) {
                case "kicked", "failed", "unavailable" -> true;
                default -> false;
            };
            case "mic", "muted" -> true;
            default -> false;
        };
    }

    /**
     * {@code lang} key of a refusal's sentence ({@code voice-result}'s reason), the general one for a reason this build
     * does not know; null for "nothing to do", which says nothing.
     */
    public static String refusalKey(String reason) {
        return switch (reason == null ? "" : reason) {
            case "off" -> "voice.refused.off";
            case "unreachable" -> "voice.refused.unreachable";
            case "rate-limited" -> "voice.refused.rate_limited";
            case "account-limited" -> "voice.refused.account_limited";
            case "staff-muted" -> "voice.refused.staff_muted";
            case "room-full" -> "voice.refused.room_full";
            // OVL-SOC: "Yeni oda" in a crew that has all its rooms (the launcher's words).
            case "rooms-full" -> "voice.refused.rooms_full";
            case "room-gone" -> "voice.refused.room_gone";
            case "kicked" -> "voice.refused.kicked";
            case "not-allowed" -> "voice.refused.not_allowed";
            case "not-member" -> "voice.refused.not_member";
            case "not-found" -> "voice.refused.not_found";
            case "nothing" -> null;
            default -> "voice.refused.failed";
        };
    }

    /** A refusal's sentence, naming {@code name} where it does ("Can şu an aranamıyor."), or null. */
    public static String refusal(String reason, String name) {
        String key = refusalKey(reason);
        if (key == null) {
            return null;
        }
        return key.equals("voice.refused.unreachable")
            ? Texts.str(key, VoiceState.shown(name, WORDS)) : Texts.str(key);
    }
}
