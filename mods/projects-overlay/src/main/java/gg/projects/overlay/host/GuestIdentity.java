package gg.projects.overlay.host;

/**
 * Who a local connection to the shared world is, as the host launcher vouched in its PLG1 header (PROTOCOL.md §7.2):
 * the publish it belongs to ({@code id}), its number in that publish ({@code conn}), the guest ({@code g1} …), the game
 * name the guest's launcher announced, whether that launcher signed in with Microsoft ({@code premium}) and the
 * friend's name-binding digest ({@code friend}; never a Discord id).
 */
public record GuestIdentity(String id, long conn, String guest, String name, boolean premium, String friend) {
}
