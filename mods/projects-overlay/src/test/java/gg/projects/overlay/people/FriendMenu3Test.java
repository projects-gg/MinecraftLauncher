package gg.projects.overlay.people;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.people.FriendMenu.Action;
import gg.projects.overlay.people.FriendMenu.AddFriend;
import gg.projects.overlay.people.FriendMenu.Crew;
import gg.projects.overlay.people.FriendMenu.Input;
import gg.projects.overlay.people.FriendMenu.Row;
import gg.projects.overlay.people.FriendMenu.Team;
import gg.projects.overlay.people.WorldInvites.Offer;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * OVL-UX3: a sender's crew rows in crew chat (menus.ts, personMenu's crew group: "Moderatör yap" or "Moderatörlükten
 * al", "Liderliği devret", "Ekipten çıkar") between the friend list's group and the relationship, and the name
 * "Yeni kategori…" keeps.
 */
class FriendMenu3Test {
    /** A crewmate who is not a friend, from crew chat, with the crew rows {@code crew} allows. */
    private static Input crewmate(Crew crew, boolean online) {
        return new Input(false, false, true, online, false, false, Offer.NONE, false, false, false, true, false,
            false, false, Team.NONE, 0, AddFriend.HIDDEN, crew);
    }

    /** A friend who is a crewmate too, from crew chat. */
    private static Input friend(Crew crew) {
        return new Input(true, true, true, true, true, false, Offer.NONE, false, true, true, true, false, false, true,
            Team.NONE, 0, AddFriend.HIDDEN, crew);
    }

    private static List<Action> actions(List<Row> plan) {
        List<Action> out = new ArrayList<>();
        for (Row row : plan) {
            out.add(row == null ? null : row.action());
        }
        return out;
    }

    private static Row row(List<Row> plan, Action action) {
        for (Row row : plan) {
            if (row != null && row.action() == action) {
                return row;
            }
        }
        return null;
    }

    @Test
    void theLeadersRowsSitWhereTheLaunchersDo() {
        List<Row> plan = FriendMenu.plan(friend(new Crew(true, true, false, false, true)));
        List<Action> expected = new ArrayList<>(List.of(Action.JOIN, Action.INVITE, Action.MESSAGE, Action.DISCORD));
        expected.add(null);
        expected.addAll(List.of(Action.FAVORITE, Action.ALERT, Action.MOVE));
        expected.add(null);
        expected.addAll(List.of(Action.MAKE_MODERATOR, Action.MAKE_FOUNDER, Action.REMOVE_MEMBER));
        expected.add(null);
        expected.addAll(List.of(Action.UNFRIEND, Action.BLOCK, Action.REPORT));
        assertEquals(expected, actions(plan));
    }

    @Test
    void aModeratorIsTakenBackAndTheFourthMakesTheRowSoft() {
        List<Row> mod = FriendMenu.plan(crewmate(new Crew(true, true, true, false, true), true));
        assertNull(row(mod, Action.MAKE_MODERATOR));
        assertFalse(row(mod, Action.UNMAKE_MODERATOR).disabled());
        List<Row> full = FriendMenu.plan(crewmate(new Crew(true, true, false, true, true), true));
        assertTrue(row(full, Action.MAKE_MODERATOR).disabled());
        assertFalse(row(full, Action.MAKE_FOUNDER).disabled());
    }

    @Test
    void withoutRolesOrTheLeadOnlyTheirRightsShow() {
        // A leader whose service has no moderators: the hand-over and the removal.
        List<Row> plain = FriendMenu.plan(crewmate(new Crew(false, true, false, false, true), true));
        assertNull(row(plain, Action.MAKE_MODERATOR));
        assertNull(row(plain, Action.UNMAKE_MODERATOR));
        assertTrue(row(plain, Action.MAKE_FOUNDER) != null && row(plain, Action.REMOVE_MEMBER) != null);
        // A moderator who may remove plain members: only that.
        List<Row> moderator = FriendMenu.plan(crewmate(new Crew(false, false, false, false, true), true));
        assertNull(row(moderator, Action.MAKE_FOUNDER));
        assertTrue(row(moderator, Action.REMOVE_MEMBER) != null);
        // Nothing allowed about them, or no crew: no crew group at all.
        assertFalse(actions(FriendMenu.plan(crewmate(new Crew(false, false, false, false, false), true)))
            .contains(Action.REMOVE_MEMBER));
        assertFalse(actions(FriendMenu.plan(crewmate(null, true))).contains(Action.MAKE_FOUNDER));
    }

    @Test
    void theRowsAreSoftWhileTheLauncherIsAway() {
        List<Row> away = FriendMenu.plan(crewmate(new Crew(true, true, false, false, true), false));
        for (Action action : List.of(Action.MAKE_MODERATOR, Action.MAKE_FOUNDER, Action.REMOVE_MEMBER)) {
            assertTrue(row(away, action).disabled(), action.name());
        }
    }

    @Test
    void theHandOverAndTheRemovalAskFirst() {
        assertTrue(Action.MAKE_FOUNDER.asks() && Action.REMOVE_MEMBER.asks());
        assertFalse(Action.MAKE_MODERATOR.asks() || Action.UNMAKE_MODERATOR.asks());
        assertFalse(Action.REMOVE_MEMBER.danger(), "the row itself is plain, as the launcher's");
    }

    @Test
    void aNewCategorysNameIsTheLaunchers() {
        assertEquals("Okul arkadaşları", FriendMenu.cleanCategoryName("  Okul \t  arkadaşları "));
        assertEquals("Gece Ekibi", FriendMenu.cleanCategoryName("Gece  Ekibi"));
        assertEquals(24, FriendMenu.cleanCategoryName("ç".repeat(40)).codePointCount(0, 24));
        assertEquals("", FriendMenu.cleanCategoryName(" \n "));
        assertEquals("", FriendMenu.cleanCategoryName(null));
    }
}
