package gg.projects.overlay.people;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.people.FriendMenu.Action;
import gg.projects.overlay.people.FriendMenu.Input;
import gg.projects.overlay.people.FriendMenu.Row;
import gg.projects.overlay.people.WorldInvites.Offer;
import gg.projects.overlay.state.OverlayState;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** A friend's menu in the overlay follows the launcher's person menu (menus.ts, personMenu): its rows, its order. */
class FriendMenuTest {
    /** A friend on Projects, the player on another server, chat on, Discord known, a new launcher connected. */
    private static Input everything() {
        return new Input(true, true, true, true, false, Offer.NONE, false, true, true);
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
    void theLaunchersOrderWithItsGroupsApart() {
        List<Row> plan = FriendMenu.plan(everything());
        List<Action> expected = new ArrayList<>(List.of(Action.JOIN, Action.INVITE, Action.MESSAGE, Action.DISCORD));
        expected.add(null);
        expected.addAll(List.of(Action.UNFRIEND, Action.BLOCK, Action.REPORT));
        assertEquals(expected, actions(plan));
        assertTrue(plan.stream().allMatch(r -> r == null || !r.disabled()));
        assertTrue(Action.BLOCK.danger() && !Action.UNFRIEND.danger());
        assertTrue(Action.UNFRIEND.asks() && Action.BLOCK.asks() && Action.REMOVE_FROM_WORLD.asks()
            && !Action.REPORT.asks());
    }

    @Test
    void aGuestOfThePlayersWorldHasItsOwnGroup() {
        Input in = new Input(false, true, true, false, false, Offer.INVITED, true, true, false);
        List<Action> expected = new ArrayList<>(List.of(Action.WORLD_INVITE, Action.MESSAGE));
        expected.add(null);
        expected.add(Action.REMOVE_FROM_WORLD);
        expected.add(null);
        expected.addAll(List.of(Action.UNFRIEND, Action.BLOCK, Action.REPORT));
        assertEquals(expected, actions(FriendMenu.plan(in)));
    }

    @Test
    void inASingleplayerWorldTheInviteGoesThere() {
        // The launcher's own invite cannot go (no server), the world one can.
        Input ready = new Input(false, true, true, false, false, Offer.READY, false, true, false);
        Row invite = row(FriendMenu.plan(ready), Action.WORLD_INVITE);
        assertFalse(invite.disabled() || invite.done());
        Row invited = row(FriendMenu.plan(new Input(false, true, true, false, false, Offer.INVITED, false, true,
            false)), Action.WORLD_INVITE);
        assertTrue(invited.disabled() && invited.done(), "Dünyana çağrıldı");
        Row full = row(FriendMenu.plan(new Input(false, true, true, false, false, Offer.FULL, false, true, false)),
            Action.WORLD_INVITE);
        assertTrue(full.disabled() && !full.done());
        Row away = row(FriendMenu.plan(new Input(false, true, true, false, false, Offer.AWAY, false, true, false)),
            Action.WORLD_INVITE);
        assertTrue(away.disabled() && !away.done());
        // With world sharing there even an older launcher without "people" offers it.
        assertTrue(row(FriendMenu.plan(new Input(false, false, true, false, false, Offer.READY, false, false, false)),
            Action.WORLD_INVITE) != null);
    }

    @Test
    void playTogetherWaitsItsFiveMinutesAndOnlyWhereItCanGo() {
        Row waiting = row(FriendMenu.plan(new Input(true, true, true, true, true, Offer.NONE, false, true, true)),
            Action.INVITE);
        assertTrue(waiting.disabled() && waiting.done());
        assertNull(row(FriendMenu.plan(new Input(true, true, true, false, false, Offer.NONE, false, true, true)),
            Action.INVITE), "not on a server: not offered");
    }

    @Test
    void whileTheLauncherIsAwayItsRowsAreSoftAndTheConversationStillOpens() {
        List<Row> plan = FriendMenu.plan(new Input(false, true, false, true, false, Offer.NONE, false, true, true));
        assertFalse(row(plan, Action.MESSAGE).disabled());
        for (Action a : List.of(Action.INVITE, Action.DISCORD, Action.UNFRIEND, Action.BLOCK, Action.REPORT)) {
            assertTrue(row(plan, a).disabled(), a.name());
        }
        assertTrue(FriendMenu.anyEnabled(plan));
    }

    @Test
    void anOlderLauncherOffersOnlyWhatTheGameDoesAlone() {
        // No "people" in its welcome: joining and the conversation, nothing it would not answer.
        List<Row> plan = FriendMenu.plan(new Input(true, false, true, true, false, Offer.NONE, false, true, true));
        assertEquals(List.of(Action.JOIN, Action.MESSAGE), actions(plan));
        assertTrue(FriendMenu.plan(new Input(false, false, true, false, false, Offer.NONE, false, false, false))
            .isEmpty());
        assertFalse(FriendMenu.anyEnabled(List.of()));
    }

    @Test
    void separatorsNeverLeadTrailOrDouble() {
        List<Row> withNulls = new ArrayList<>();
        withNulls.add(null);
        withNulls.add(new Row(Action.MESSAGE, false, false));
        withNulls.add(null);
        withNulls.add(null);
        withNulls.add(new Row(Action.REPORT, false, false));
        withNulls.add(null);
        List<Row> tidy = FriendMenu.tidy(withNulls);
        assertEquals(3, tidy.size());
        assertNull(tidy.get(1));
    }

    @Test
    void theWorldInviteFollowsTheSession() {
        OverlayState.HostGuest ayse = new OverlayState.HostGuest("g1", "f1", "Ayşe", "Ayse", "in");
        OverlayState.HostGuest mert = new OverlayState.HostGuest("", "f2", "Mert", "", "invited");
        OverlayState.HostView open = new OverlayState.HostView(true, 3, List.of(ayse, mert), null);
        // Not in a singleplayer world: the launcher's invite applies.
        assertEquals(Offer.NONE, WorldInvites.offer(false, open, false, List.of(), "f3", true));
        // In one with nothing open: the world opens for them.
        assertEquals(Offer.READY, WorldInvites.offer(true, null, false, List.of(), "f3", true));
        // Opening for them: sent; for someone else: still ready.
        assertEquals(Offer.INVITED, WorldInvites.offer(true, null, true, List.of("f3"), "f3", true));
        assertEquals(Offer.READY, WorldInvites.offer(true, null, true, List.of("f3"), "f4", true));
        // Open: in it or invited reads as sent; a free place takes one more; no place left is soft.
        assertEquals(Offer.INVITED, WorldInvites.offer(true, open, false, List.of(), "f1", true));
        assertEquals(Offer.INVITED, WorldInvites.offer(true, open, false, List.of(), "f2", true));
        assertEquals(Offer.READY, WorldInvites.offer(true, open, false, List.of(), "f3", true));
        OverlayState.HostView full = new OverlayState.HostView(true, 2, List.of(ayse, mert), null);
        assertEquals(Offer.FULL, WorldInvites.offer(true, full, false, List.of(), "f3", true));
        // Someone offline can't come now (world sharing invites friends who are online), unless already invited.
        assertEquals(Offer.AWAY, WorldInvites.offer(true, open, false, List.of(), "f3", false));
        assertEquals(Offer.INVITED, WorldInvites.offer(true, open, false, List.of(), "f2", false));
        // A closed session's leftovers count for nothing.
        OverlayState.HostView closed = new OverlayState.HostView(false, 2, List.of(ayse, mert), "unavailable");
        assertEquals(Offer.READY, WorldInvites.offer(true, closed, false, List.of(), "f1", true));
        // Only a guest whose launcher connected can be sent off.
        assertEquals("g1", WorldInvites.guest(open, "f1").guest());
        assertNull(WorldInvites.guest(open, "f2"));
        assertNull(WorldInvites.guest(closed, "f1"));
        assertNull(WorldInvites.guest(null, "f1"));
        assertNull(WorldInvites.guest(open, "f9"));
    }
}
