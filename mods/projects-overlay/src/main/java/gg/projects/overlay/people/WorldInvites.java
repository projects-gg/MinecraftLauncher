package gg.projects.overlay.people;

import gg.projects.overlay.state.OverlayState;
import java.util.List;

/**
 * What the friend menu's "Birlikte oyna" may do in the player's singleplayer world (HOST1; the owner, 24 Oct: an invite
 * from the overlay there opens the world to friends and invites in one step, as the launcher's own invite does), read
 * from world sharing's state: {@code HostController} (in a world, opening) and the launcher's mirror of the session. On
 * a server the row is the launcher's play-together invite. Pure, so it is tested on its own.
 */
public final class WorldInvites {
    /** What a world invite may do for a friend now. */
    public enum Offer {
        /** Not in a singleplayer world: invites go where the player plays. */
        NONE,
        /** May be sent (the world opens first when needed; a world that can't open says why when it is pressed). */
        READY,
        /** In the open session already (invited, joining or in), or the opening world is for them ("Davet gönderildi"). */
        INVITED,
        /** Every place of the open world is taken (soft). */
        FULL,
        /** They are not online: world sharing invites friends who can come now, as its page does (soft). */
        AWAY
    }

    private WorldInvites() {
    }

    /**
     * @param inWorld    the game is in a singleplayer world ({@code HostController.invitesToWorld()})
     * @param session    the launcher's mirror of the player's world session, or null
     * @param opening    the world is opening ({@code HostController.requesting()})
     * @param openingFor the friends the opening is for ({@code HostController.lastInvite()})
     * @param online     the friend is online now
     */
    public static Offer offer(boolean inWorld, OverlayState.HostView session, boolean opening,
        List<String> openingFor, String friendId, boolean online) {
        if (!inWorld) {
            return Offer.NONE;
        }
        if (opening && openingFor != null && openingFor.contains(friendId)) {
            return Offer.INVITED;
        }
        boolean open = session != null && session.open();
        if (open && entry(session, friendId) != null) {
            return Offer.INVITED;
        }
        if (!online) {
            return Offer.AWAY;
        }
        return open && session.guests().size() >= session.max() ? Offer.FULL : Offer.READY;
    }

    /** The friend's place in the open session once their launcher connected (it can be sent off), or null. */
    public static OverlayState.HostGuest guest(OverlayState.HostView session, String friendId) {
        OverlayState.HostGuest g = session == null || !session.open() ? null : entry(session, friendId);
        return g == null || g.guest().isEmpty() ? null : g;
    }

    private static OverlayState.HostGuest entry(OverlayState.HostView session, String friendId) {
        for (OverlayState.HostGuest g : session.guests()) {
            if (g.friend().equals(friendId)) {
                return g;
            }
        }
        return null;
    }
}
