package gg.projects.overlay.ui.panel;

import static org.junit.jupiter.api.Assertions.assertEquals;

import gg.projects.overlay.state.OverlayState.Delivery;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Frames;
import org.junit.jupiter.api.Test;

/**
 * The player's own marks in a conversation (BETA2-FIX): the sent tick and an own reaction chip are never gold, as in
 * the launcher (design.md §2).
 */
class DeliveryMarkTest {
    @Test
    void aSentMessagesTickIsAshAsItsTime() {
        assertEquals(Theme.ASH_400, MessageList.deliveryMarkColor(Delivery.SENT), "the tick was gold-300");
        assertEquals(Theme.ASH_400, MessageList.deliveryMarkColor(Delivery.PENDING));
        assertEquals(Theme.FLARE_400, MessageList.deliveryMarkColor(Delivery.FAILED));
    }

    @Test
    void theOwnReactionChipIsBoneOnTheQuietPlate() {
        assertEquals(Theme.BONE_50, MessageList.reactionCountColor(true), "the count was gold-200");
        assertEquals(Theme.ASH_300, MessageList.reactionCountColor(false));
        assertEquals(Theme.withAlpha(Theme.BONE_100, 0.42f), Frames.REACTION_MINE.spec.border(), "the edge was gold");
        assertEquals(Theme.withAlpha(0xFFFFFF, 0.16f), Frames.REACTION_MINE.spec.fillTop(), "the plate was gold-tinted");
    }
}
