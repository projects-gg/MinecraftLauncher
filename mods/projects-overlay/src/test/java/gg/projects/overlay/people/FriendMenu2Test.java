package gg.projects.overlay.people;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.people.FriendMenu.Action;
import gg.projects.overlay.people.FriendMenu.AddFriend;
import gg.projects.overlay.people.FriendMenu.Input;
import gg.projects.overlay.people.FriendMenu.Row;
import gg.projects.overlay.people.FriendMenu.Team;
import gg.projects.overlay.people.WorldInvites.Offer;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * OVL-UX2: the rest of the launcher's person menu (menus.ts, personMenu): the friend list's group (favourite, online
 * notice, category, crew invite) between being together and the relationship, and a crewmate's menu from crew chat.
 */
class FriendMenu2Test {
    /** A friend on Projects with everything the launcher offers, one crew to invite them to. */
    private static Input friend(boolean fav, boolean alert, Team team, int teams) {
        return new Input(true, true, true, true, true, false, Offer.NONE, false, true, true, true, fav, alert, true,
            team, teams, AddFriend.HIDDEN);
    }

    /** A crewmate who is not a friend, from crew chat. */
    private static Input crewmate(AddFriend add, boolean online) {
        return new Input(false, false, true, online, false, false, Offer.NONE, false, false, false, true, false,
            false, false, Team.NONE, 0, add);
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
    void theFriendListsGroupSitsWhereTheLaunchersDoes() {
        List<Row> plan = FriendMenu.plan(friend(false, false, Team.INVITABLE, 1));
        List<Action> expected = new ArrayList<>(List.of(Action.JOIN, Action.INVITE, Action.MESSAGE, Action.DISCORD));
        expected.add(null);
        expected.addAll(List.of(Action.FAVORITE, Action.ALERT, Action.MOVE, Action.CREW_INVITE));
        expected.add(null);
        expected.addAll(List.of(Action.UNFRIEND, Action.BLOCK, Action.REPORT));
        assertEquals(expected, actions(plan));
        assertTrue(row(plan, Action.MOVE).sub(), "the categories are a list");
        assertFalse(row(plan, Action.CREW_INVITE).sub(), "one crew: the row invites");
        assertTrue(plan.stream().allMatch(r -> r == null || !r.disabled()));
    }

    @Test
    void theMarksSayWhatTheyAre() {
        List<Row> plan = FriendMenu.plan(friend(true, true, Team.NONE, 0));
        assertTrue(row(plan, Action.FAVORITE).on(), "Favorilerden çıkar");
        assertTrue(row(plan, Action.ALERT).on(), "its check");
        assertNull(row(plan, Action.CREW_INVITE), "no crew to invite to");
        // No categories: no "Kategoriye taşı".
        Input none = new Input(true, true, true, true, true, false, Offer.NONE, false, true, true, true, false, false,
            false, Team.NONE, 0, AddFriend.HIDDEN);
        assertNull(row(FriendMenu.plan(none), Action.MOVE));
    }

    @Test
    void aCrewInviteFollowsTheBestCrew() {
        // Invited to the one crew: the row says so and waits.
        Row invited = row(FriendMenu.plan(friend(false, false, Team.INVITED, 1)), Action.CREW_INVITE);
        assertTrue(invited.disabled() && invited.done());
        // Every crew full: soft, still "Ekibe davet et".
        Row full = row(FriendMenu.plan(friend(false, false, Team.FULL, 1)), Action.CREW_INVITE);
        assertTrue(full.disabled() && !full.done());
        // In every crew: no row (the launcher's member state).
        assertNull(row(FriendMenu.plan(friend(false, false, Team.MEMBER, 2)), Action.CREW_INVITE));
        // Several crews: a list, the row itself never soft while the launcher is there.
        Row list = row(FriendMenu.plan(friend(false, false, Team.INVITED, 2)), Action.CREW_INVITE);
        assertTrue(list.sub() && !list.disabled() && !list.done());
        // The best place among them, in the launcher's order.
        assertEquals(Team.INVITABLE, FriendMenu.best(List.of(Team.MEMBER, Team.FULL, Team.INVITABLE)));
        assertEquals(Team.INVITED, FriendMenu.best(List.of(Team.FULL, Team.INVITED)));
        assertEquals(Team.NONE, FriendMenu.best(List.of()));
    }

    @Test
    void anOlderLauncherKeepsOvlUxsMenu() {
        Input old = new Input(true, true, true, true, true, false, Offer.NONE, false, true, true, false, true, true, true,
            Team.INVITABLE, 1, AddFriend.HIDDEN);
        List<Row> plan = FriendMenu.plan(old);
        for (Action action : List.of(Action.FAVORITE, Action.ALERT, Action.MOVE, Action.CREW_INVITE)) {
            assertNull(row(plan, action), action.name());
        }
    }

    @Test
    void aCrewmateFromCrewChatGetsTheRowsTheLauncherGivesACrewmate() {
        List<Row> plan = FriendMenu.plan(crewmate(AddFriend.READY, true));
        List<Action> expected = new ArrayList<>(List.of(Action.ADD_FRIEND));
        expected.add(null);
        expected.addAll(List.of(Action.BLOCK, Action.REPORT));
        assertEquals(expected, actions(plan));
        // They asked first: "İsteği kabul et".
        assertTrue(row(FriendMenu.plan(crewmate(AddFriend.RECEIVED, true)), Action.ADD_FRIEND).accept());
        // Asked already: "İstek gönderildi", soft with its check.
        Row sent = row(FriendMenu.plan(crewmate(AddFriend.SENT, true)), Action.ADD_FRIEND);
        assertTrue(sent.disabled() && sent.done());
        // Requests between crewmates off: block and report only.
        assertEquals(List.of(Action.BLOCK, Action.REPORT), actions(FriendMenu.plan(crewmate(AddFriend.HIDDEN,
            true))));
        // The launcher away: everything soft.
        assertFalse(FriendMenu.anyEnabled(FriendMenu.plan(crewmate(AddFriend.READY, false))));
        // An older launcher takes nothing about someone who is not a friend: no menu.
        Input old = new Input(false, false, true, true, false, false, Offer.NONE, false, false, false, false, false,
            false, false, Team.NONE, 0, AddFriend.READY);
        assertTrue(FriendMenu.plan(old).isEmpty());
    }

    @Test
    void aCrewmateOnProjectsIsJoinedAndWrittenToOnDiscordAsTheLauncherOffers() {
        // The launcher's order: Katıl, Discord'dan mesaj at, Arkadaş ekle; then the relationship.
        Input mate = new Input(false, true, true, true, false, false, Offer.NONE, false, false, true, true, false,
            false, false, Team.NONE, 0, AddFriend.READY);
        List<Action> expected = new ArrayList<>(List.of(Action.JOIN, Action.DISCORD, Action.ADD_FRIEND));
        expected.add(null);
        expected.addAll(List.of(Action.BLOCK, Action.REPORT));
        assertEquals(expected, actions(FriendMenu.plan(mate)));
        // Never "Birlikte oyna", "Mesaj gönder" or the friend list's rows for someone who is not a friend.
        for (Action action : List.of(Action.INVITE, Action.WORLD_INVITE, Action.MESSAGE, Action.FAVORITE,
            Action.ALERT, Action.MOVE, Action.CREW_INVITE, Action.UNFRIEND)) {
            assertNull(row(FriendMenu.plan(mate), action), action.name());
        }
        // The launcher away: Discord waits for it (the launcher opens the profile).
        Input away = new Input(false, true, true, false, false, false, Offer.NONE, false, false, true, true, false,
            false, false, Team.NONE, 0, AddFriend.HIDDEN);
        assertTrue(row(FriendMenu.plan(away), Action.DISCORD).disabled());
    }
}
