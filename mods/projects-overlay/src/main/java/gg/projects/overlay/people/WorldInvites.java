package gg.projects.overlay.people;

/**
 * The seam between the friend menu and world sharing (HOST1-M): in a singleplayer world the menu's "Birlikte oyna"
 * opens the world to friends when it is not open yet and invites the friend, in one step (the owner, 24 Oct: "like
 * the launcher"); on a server it stays the launcher's play-together invite. World sharing installs its {@link Hook}
 * (HOST1-M's {@code HostController}: {@code invitesToWorld()}, {@code inviteFriend(friendId)}); until then no world
 * invite is offered.
 *
 * <p>Called on the render thread only.
 */
public final class WorldInvites {
    /** What a world invite may do for a friend now. */
    public enum Offer {
        /** Not in a singleplayer world (or world sharing is not there): invites go where the player plays. */
        NONE,
        /** May be sent (the world opens first when needed). */
        READY,
        /** Invited to the open world already, or a guest in it ("Davet gönderildi", soft). */
        INVITED,
        /** Every place of the open world is taken (soft). */
        FULL
    }

    /** World sharing's side. */
    public interface Hook {
        /** What a world invite may do for {@code friendId} now ({@link Offer#NONE} outside a singleplayer world). */
        Offer offer(String friendId);

        /**
         * Invites {@code friendId} to the player's world, opening it to friends when needed. Null when the invite went
         * (or goes once the world is open), else the one short line to show (the world can't open, the launcher is
         * away).
         */
        String invite(String friendId);

        /** Whether {@code friendId} is a guest of the player's open world ("Dünyadan çıkar"). */
        boolean guest(String friendId);

        /** "Dünyadan çıkar": asks its own one-sentence question, then sends the guest off. */
        void remove(String friendId);
    }

    private static Hook hook;

    private WorldInvites() {
    }

    /** World sharing hands in its side (null takes it away). */
    public static void install(Hook value) {
        hook = value;
    }

    public static Offer offer(String friendId) {
        Hook h = hook;
        Offer offer = h == null ? null : h.offer(friendId);
        return offer == null ? Offer.NONE : offer;
    }

    /** A world invite; null when it went, else the line to show (never called without an offer). */
    public static String invite(String friendId) {
        Hook h = hook;
        return h == null ? null : h.invite(friendId);
    }

    public static boolean guest(String friendId) {
        Hook h = hook;
        return h != null && h.guest(friendId);
    }

    /** "Dünyadan çıkar"; false when world sharing is not there. */
    public static boolean remove(String friendId) {
        Hook h = hook;
        if (h == null) {
            return false;
        }
        h.remove(friendId);
        return true;
    }
}
