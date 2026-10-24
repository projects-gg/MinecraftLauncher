package gg.projects.overlay.people;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * What a friend's menu offers in the overlay (OVL-UX): the launcher's person menu for a friend in the list
 * ({@code apps/launcher/src/features/social/menus.ts}, {@code personMenu}), in its order, with the rows the game can do
 * (FIX2-D: the same menu, in the same order, wherever a player shows). The groups: being together (join, play together,
 * message, Discord), the player's open world (send off), the relationship (unfriend, block, report). A row shows only
 * where the rules allow it. The launcher's own list rows (favourite, online notice, category, crew invite) stay in the
 * launcher: they change only its list and its notifications. Pure, so it is tested on its own.
 */
public final class FriendMenu {
    /** A row's action. */
    public enum Action {
        /** "Katıl": their server, or their world when they invited the player. */
        JOIN,
        /** "Birlikte oyna": the launcher's play-together invite to where the player plays. Soft "Davet gönderildi". */
        INVITE,
        /**
         * "Dünyana çağır": in a singleplayer world, an invite to it that opens it to friends first when it is not open
         * yet ({@link WorldInvites}; the launcher's world invite). Soft "Dünyana çağrıldı" once it went.
         */
        WORLD_INVITE,
        /** "Mesaj gönder": their conversation in the panel. */
        MESSAGE,
        /** "Discord'dan mesaj at": their Discord profile, opened by the launcher. */
        DISCORD,
        /** "Dünyadan çıkar": a guest of the player's open world, asked first (world sharing's own words). */
        REMOVE_FROM_WORLD,
        /** "Arkadaşlıktan çıkar", asked first. */
        UNFRIEND,
        /** "Engelle", asked first. */
        BLOCK,
        /** "Rapor et": the report dialog. */
        REPORT;

        /** One sentence is asked before it goes. */
        public boolean asks() {
            return this == REMOVE_FROM_WORLD || this == UNFRIEND || this == BLOCK;
        }

        /** Drawn in the error colour (the launcher's danger row). */
        public boolean danger() {
            return this == BLOCK;
        }
    }

    /** A row: soft rows cannot be picked; {@code done} words a soft row as what already happened. */
    public record Row(Action action, boolean disabled, boolean done) {
    }

    /**
     * Everything a friend's menu depends on.
     *
     * @param joinable      "Katıl" makes sense now (on a server the game is not on, or their world invite)
     * @param people        the launcher takes the menu's frames (its welcome lists {@code people})
     * @param online        the launcher's bridge is connected now
     * @param invite        "Birlikte oyna" can go to where the player plays (the welcome lists {@code invite})
     * @param inviteWaiting an invite to them went in the last five minutes
     * @param world         what an invite to the player's singleplayer world may do ({@code NONE}: not in one)
     * @param guest         they are a guest of the player's open world
     * @param message       their conversation can be opened (chat is on)
     * @param discord       their Discord profile is known
     */
    public record Input(boolean joinable, boolean people, boolean online, boolean invite, boolean inviteWaiting,
        WorldInvites.Offer world, boolean guest, boolean message, boolean discord) {
    }

    private FriendMenu() {
    }

    /** The rows, a {@code null} between groups (never first, last or twice). */
    public static List<Row> plan(Input in) {
        List<Row> out = new ArrayList<>();
        boolean launcher = in.people();
        boolean live = launcher && in.online();
        WorldInvites.Offer world = in.world() == null ? WorldInvites.Offer.NONE : in.world();
        if (in.joinable()) {
            out.add(new Row(Action.JOIN, false, false));
        }
        if (world != WorldInvites.Offer.NONE) {
            // In a singleplayer world the invite goes there (the world opens to friends first).
            boolean invited = world == WorldInvites.Offer.INVITED;
            out.add(new Row(Action.WORLD_INVITE, !in.online() || world != WorldInvites.Offer.READY, invited));
        } else if (launcher && in.invite()) {
            out.add(new Row(Action.INVITE, !in.online() || in.inviteWaiting(), in.inviteWaiting()));
        }
        if (in.message()) {
            // Opens even while the launcher is away: the conversation says why it cannot be written to.
            out.add(new Row(Action.MESSAGE, false, false));
        }
        if (launcher && in.discord()) {
            out.add(new Row(Action.DISCORD, !in.online(), false));
        }
        out.add(null);
        if (in.guest()) {
            out.add(new Row(Action.REMOVE_FROM_WORLD, !in.online(), false));
        }
        out.add(null);
        if (launcher) {
            out.add(new Row(Action.UNFRIEND, !live, false));
            out.add(new Row(Action.BLOCK, !live, false));
            out.add(new Row(Action.REPORT, !live, false));
        }
        return tidy(out);
    }

    /** Whether the plan has a row that can be picked. */
    public static boolean anyEnabled(List<Row> plan) {
        for (Row row : plan) {
            if (row != null && !row.disabled()) {
                return true;
            }
        }
        return false;
    }

    /** Separators only between groups that have rows. */
    static List<Row> tidy(List<Row> plan) {
        List<Row> out = new ArrayList<>(plan.size());
        for (Row row : plan) {
            if (row == null && (out.isEmpty() || out.get(out.size() - 1) == null)) {
                continue;
            }
            out.add(row);
        }
        if (!out.isEmpty() && out.get(out.size() - 1) == null) {
            out.remove(out.size() - 1);
        }
        return Collections.unmodifiableList(out);
    }
}
