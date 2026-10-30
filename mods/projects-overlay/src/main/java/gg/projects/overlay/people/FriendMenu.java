package gg.projects.overlay.people;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * What a person's menu offers in the overlay (OVL-UX, OVL-UX2): the launcher's person menu
 * ({@code apps/launcher/src/features/social/menus.ts}, {@code personMenu}), in its order, with the rows the game can do
 * (FIX2-D: the same menu, in the same order, wherever a player shows: a friend's row, their conversation's chip, a
 * sender's name in crew chat). The groups: being together (join, play together, message, Discord, add as a friend),
 * the friend list (favourite, online notice, category, crew invite), the crew the menu opened from (moderator,
 * leadership, remove from the crew; OVL-UX3, a sender in crew chat), the player's open world (send off), the
 * relationship (unfriend, block, report). A row shows only where the rules allow it. Pure, so it is tested on its own.
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
        /**
         * "Arkadaş ekle" on a crewmate who is not a friend (OVL-UX2; a soft "İstek gönderildi" once asked), or "İsteği
         * kabul et" when they asked first.
         */
        ADD_FRIEND,
        /** "Favorilere ekle" / "Favorilerden çıkar" (OVL-UX2). */
        FAVORITE,
        /** "Çevrimiçi olunca bildir", a check (OVL-UX2). */
        ALERT,
        /** "Kategoriye taşı": a list of the player's categories (OVL-UX2). */
        MOVE,
        /** "Ekibe davet et": the crew, or a list of them with several (OVL-UX2). */
        CREW_INVITE,
        /** "Moderatör yap" in a crew the player leads (OVL-UX3); soft while it has its 4 moderators. */
        MAKE_MODERATOR,
        /** "Moderatörlükten al" (OVL-UX3). */
        UNMAKE_MODERATOR,
        /** "Liderliği devret", asked first (OVL-UX3). */
        MAKE_FOUNDER,
        /** "Ekipten çıkar", asked first (OVL-UX3). */
        REMOVE_MEMBER,
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
            return this == MAKE_FOUNDER || this == REMOVE_MEMBER || this == REMOVE_FROM_WORLD || this == UNFRIEND
                || this == BLOCK;
        }

        /** Drawn in the error colour (the launcher's danger row). */
        public boolean danger() {
            return this == BLOCK;
        }
    }

    /**
     * A row. {@code disabled}: soft, cannot be picked; {@code done}: worded as what already happened ("Davet
     * gönderildi", "Dünyana çağrıldı", "İstek gönderildi"); {@code on}: a mark that is set (the favourite's filled star,
     * the online notice's check); {@code sub}: it opens a list (categories, several crews); {@code accept}: "Arkadaş
     * ekle" answers their request ("İsteği kabul et").
     */
    public record Row(Action action, boolean disabled, boolean done, boolean on, boolean sub, boolean accept) {
        public Row(Action action, boolean disabled, boolean done) {
            this(action, disabled, done, false, false, false);
        }
    }

    /** Where the friend stands with the player's crews, the best of them (the launcher's {@code INVITE_ORDER}). */
    public enum Team {
        /** No crew to invite them to (none the player may invite to, or crews off). */
        NONE,
        /** At least one crew may take them. */
        INVITABLE,
        /** Invited to one already (none other may take them). */
        INVITED,
        /** Every crew that could take them is full. */
        FULL,
        /** In every crew the player may invite to: no row. */
        MEMBER
    }

    /** A crewmate who is not a friend, and friend requests between them (OVL-UX2). */
    public enum AddFriend {
        /** Not offered (a friend, crewmate requests off, or the launcher does not take the frame). */
        HIDDEN,
        READY,
        /** The player asked: soft "İstek gönderildi". */
        SENT,
        /** They asked: "İsteği kabul et". */
        RECEIVED
    }

    /**
     * The crew whose chat the menu opened from, and what the player may do about them there (OVL-UX3; the launcher's
     * {@code PersonCrew}, from {@code lists.roles}). Only while the launcher takes the crew rows.
     *
     * @param roles          the player names moderators there (leads it, and the service offers roles)
     * @param leader         the player leads the crew: "Liderliği devret"
     * @param moderator      they are a moderator: "Moderatörlükten al" instead of "Moderatör yap"
     * @param moderatorsFull the crew has its 4 moderators: "Moderatör yap" soft
     * @param removable      the player may remove them: "Ekipten çıkar"
     */
    public record Crew(boolean roles, boolean leader, boolean moderator, boolean moderatorsFull, boolean removable) {
    }

    /**
     * Everything a person's menu depends on.
     *
     * @param friend        they are on the player's friend list (else a crewmate from crew chat)
     * @param joinable      "Katıl" makes sense now (on a server the game is not on, or their world invite)
     * @param people        the launcher takes the menu's frames (its welcome lists {@code people})
     * @param online        the launcher's bridge is connected now
     * @param invite        "Birlikte oyna" can go to where the player plays (the welcome lists {@code invite})
     * @param inviteWaiting an invite to them went in the last five minutes
     * @param world         what an invite to the player's singleplayer world may do ({@code NONE}: not in one)
     * @param guest         they are a guest of the player's open world
     * @param message       their conversation can be opened from here (chat is on, and it is not the one on screen)
     * @param discord       their Discord profile is known
     * @param people2       the launcher takes the rest of the person menu (its welcome lists {@code people2})
     * @param fav           one of the player's favourites
     * @param alert         "Çevrimiçi olunca bildir" is on for them
     * @param categories    the player has friend categories ("Kategoriye taşı")
     * @param team          their place in the crews the player may invite to
     * @param teams         how many crews could be shown ("Ekibe davet et" opens a list with more than one)
     * @param addFriend     a crewmate who is not a friend: "Arkadaş ekle" and its state
     * @param crew          the crew rows of the crew chat the menu opened from (OVL-UX3), or null
     */
    public record Input(boolean friend, boolean joinable, boolean people, boolean online, boolean invite,
        boolean inviteWaiting, WorldInvites.Offer world, boolean guest, boolean message, boolean discord,
        boolean people2, boolean fav, boolean alert, boolean categories, Team team, int teams, AddFriend addFriend,
        Crew crew) {
        /** OVL-UX's friend row (the rows before OVL-UX2). */
        public Input(boolean joinable, boolean people, boolean online, boolean invite, boolean inviteWaiting,
            WorldInvites.Offer world, boolean guest, boolean message, boolean discord) {
            this(true, joinable, people, online, invite, inviteWaiting, world, guest, message, discord, false, false,
                false, false, Team.NONE, 0, AddFriend.HIDDEN, null);
        }

        /** OVL-UX2's menu (the rows before OVL-UX3's crew rows). */
        public Input(boolean friend, boolean joinable, boolean people, boolean online, boolean invite,
            boolean inviteWaiting, WorldInvites.Offer world, boolean guest, boolean message, boolean discord,
            boolean people2, boolean fav, boolean alert, boolean categories, Team team, int teams,
            AddFriend addFriend) {
            this(friend, joinable, people, online, invite, inviteWaiting, world, guest, message, discord, people2, fav,
                alert, categories, team, teams, addFriend, null);
        }
    }

    /** The most a friend category's name keeps (the launcher's {@code GROUP_NAME_MAX}; OVL-UX3). */
    public static final int CATEGORY_NAME_MAX = 24;

    private FriendMenu() {
    }

    /**
     * A new category's name as the launcher keeps it ({@code cleanGroupName}; "Yeni kategori…", OVL-UX3): spaces
     * folded and trimmed, at most {@link #CATEGORY_NAME_MAX} characters. Empty: no name.
     */
    public static String cleanCategoryName(String typed) {
        String folded = typed == null ? "" : typed.replaceAll("(?U)\\s+", " ").strip();
        int[] points = folded.codePoints().limit(CATEGORY_NAME_MAX).toArray();
        return new String(points, 0, points.length);
    }

    /** The rows, a {@code null} between groups (never first, last or twice). */
    public static List<Row> plan(Input in) {
        List<Row> out = new ArrayList<>();
        boolean launcher = in.people();
        boolean live = launcher && in.online();
        boolean more = in.people2() && launcher;
        WorldInvites.Offer world = in.world() == null ? WorldInvites.Offer.NONE : in.world();
        // Being together.
        if (in.joinable()) {
            out.add(new Row(Action.JOIN, false, false));
        }
        if (in.friend()) {
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
        }
        if (launcher && in.discord()) {
            out.add(new Row(Action.DISCORD, !in.online(), false));
        }
        AddFriend add = in.addFriend() == null ? AddFriend.HIDDEN : in.addFriend();
        if (!in.friend() && more && add != AddFriend.HIDDEN) {
            boolean sent = add == AddFriend.SENT;
            out.add(new Row(Action.ADD_FRIEND, !live || sent, sent, false, false, add == AddFriend.RECEIVED));
        }
        out.add(null);
        // The friend list.
        if (in.friend() && more) {
            out.add(new Row(Action.FAVORITE, !in.online(), false, in.fav(), false, false));
            out.add(new Row(Action.ALERT, !in.online(), false, in.alert(), false, false));
            if (in.categories()) {
                out.add(new Row(Action.MOVE, !live, false, false, true, false));
            }
            Team team = in.team() == null ? Team.NONE : in.team();
            if (team != Team.NONE && team != Team.MEMBER) {
                boolean list = in.teams() > 1;
                boolean soft = !live || (!list && team != Team.INVITABLE);
                out.add(new Row(Action.CREW_INVITE, soft, !list && team == Team.INVITED, false, list, false));
            }
        }
        out.add(null);
        // The crew the menu opened from (OVL-UX3): the launcher's crew rows, in its order.
        Crew crew = in.crew();
        if (crew != null && launcher) {
            if (crew.leader() && crew.roles()) {
                out.add(crew.moderator() ? new Row(Action.UNMAKE_MODERATOR, !live, false)
                    : new Row(Action.MAKE_MODERATOR, !live || crew.moderatorsFull(), false));
            }
            if (crew.leader()) {
                out.add(new Row(Action.MAKE_FOUNDER, !live, false));
            }
            if (crew.removable()) {
                out.add(new Row(Action.REMOVE_MEMBER, !live, false));
            }
        }
        out.add(null);
        // The player's open world (HOST1): its guest leaves it.
        if (in.guest()) {
            out.add(new Row(Action.REMOVE_FROM_WORLD, !in.online(), false));
        }
        out.add(null);
        // The relationship.
        if (in.friend() && launcher) {
            out.add(new Row(Action.UNFRIEND, !live, false));
        }
        if (launcher && (in.friend() || more)) {
            // A crewmate from crew chat is blocked and reported as the launcher does (OVL-UX2).
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

    /**
     * The best place among the crews (the launcher's {@code INVITE_ORDER}: invitable, invited, full, member), or
     * {@link Team#NONE} without any.
     */
    public static Team best(List<Team> places) {
        Team best = Team.NONE;
        for (Team place : places) {
            if (best == Team.NONE || rank(place) < rank(best)) {
                best = place;
            }
        }
        return best;
    }

    private static int rank(Team team) {
        return switch (team) {
            case INVITABLE -> 0;
            case INVITED -> 1;
            case FULL -> 2;
            case MEMBER -> 3;
            case NONE -> 4;
        };
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
