package gg.projects.overlay.ui.panel;

import static org.junit.jupiter.api.Assertions.assertEquals;

import gg.projects.overlay.state.OverlayState.Delivery;
import gg.projects.overlay.ui.Theme;
import org.junit.jupiter.api.Test;

/** The mark before the player's own message's time (BETA2-FIX): never gold, as in the launcher (design.md §2). */
class DeliveryMarkTest {
    @Test
    void aSentMessagesTickIsAshAsItsTime() {
        assertEquals(Theme.ASH_400, MessageList.deliveryMarkColor(Delivery.SENT), "the tick was gold-300");
        assertEquals(Theme.ASH_400, MessageList.deliveryMarkColor(Delivery.PENDING));
        assertEquals(Theme.FLARE_400, MessageList.deliveryMarkColor(Delivery.FAILED));
    }
}
