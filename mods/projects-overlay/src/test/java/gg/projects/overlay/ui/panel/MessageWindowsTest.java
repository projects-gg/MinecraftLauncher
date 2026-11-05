package gg.projects.overlay.ui.panel;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * CHAT3 (OVL-SOC2): when the panel offers "Düzenle" and "Sil" on a message, as the launcher does (the service decides
 * anyway): the player's own, confirmed message, edited for 15 minutes (not a sticker), deleted for a week.
 */
class MessageWindowsTest {
    private static final long NOW = 1_800_000_000_000L;
    private static final long MINUTE = 60_000L;
    private static final long DAY = 86_400_000L;

    @Test
    void theWindowsAreTheLaunchers() {
        assertEquals(15 * MINUTE, MessageList.EDIT_WINDOW_MS);
        assertEquals(7 * DAY, MessageList.DELETE_WINDOW_MS);
    }

    @Test
    void anOwnMessageIsEditableForFifteenMinutes() {
        assertTrue(MessageList.editable(true, 42, false, false, NOW, NOW));
        assertTrue(MessageList.editable(true, 42, false, false, NOW - 15 * MINUTE + 1, NOW));
        assertFalse(MessageList.editable(true, 42, false, false, NOW - 15 * MINUTE, NOW), "15 minutes is over");
        // This computer's clock behind the service's: still editable (the service decides).
        assertTrue(MessageList.editable(true, 42, false, false, NOW + MINUTE, NOW));
    }

    @Test
    void onlyAnOwnConfirmedLiveTextIsEditable() {
        assertFalse(MessageList.editable(false, 42, false, false, NOW, NOW), "someone else's");
        assertFalse(MessageList.editable(true, 0, false, false, NOW, NOW), "not confirmed yet");
        assertFalse(MessageList.editable(true, 42, true, false, NOW, NOW), "deleted");
        assertFalse(MessageList.editable(true, 42, false, true, NOW, NOW), "a sticker");
    }

    @Test
    void anOwnMessageIsDeletableForAWeek() {
        assertTrue(MessageList.deletable(true, 42, false, NOW - 6 * DAY, NOW));
        assertTrue(MessageList.deletable(true, 42, false, NOW - 7 * DAY + 1, NOW));
        assertFalse(MessageList.deletable(true, 42, false, NOW - 7 * DAY, NOW), "a week is over");
    }

    @Test
    void onlyAnOwnConfirmedLiveMessageIsDeletable() {
        assertFalse(MessageList.deletable(false, 42, false, NOW, NOW), "someone else's");
        assertFalse(MessageList.deletable(true, 0, false, NOW, NOW), "not confirmed yet");
        assertFalse(MessageList.deletable(true, 42, true, NOW, NOW), "already deleted");
    }

    @Test
    void noLineIsNeitherEditableNorDeletable() {
        assertFalse(MessageList.editable(null, NOW));
        assertFalse(MessageList.deletable(null, NOW));
    }
}
