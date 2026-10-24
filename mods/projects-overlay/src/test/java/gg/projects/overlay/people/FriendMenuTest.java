package gg.projects.overlay.people;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.people.FriendMenu.Action;
import gg.projects.overlay.people.FriendMenu.Input;
import gg.projects.overlay.people.FriendMenu.Row;
import gg.projects.overlay.people.WorldInvites.Offer;
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
        assertTrue(Action.UNFRIEND.asks() && Action.BLOCK.asks() && !Action.REPORT.asks());
    }

    @Test
    void aGuestOfThePlayersWorldHasItsOwnGroup() {
        Input in = new Input(false, true, true, false, false, Offer.INVITED, true, true, false);
        List<Action> expected = new ArrayList<>(List.of(Action.INVITE, Action.MESSAGE));
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
        Row invite = row(FriendMenu.plan(ready), Action.INVITE);
        assertFalse(invite.disabled() || invite.done());
        Row invited = row(FriendMenu.plan(new Input(false, true, true, false, false, Offer.INVITED, false, true,
            false)), Action.INVITE);
        assertTrue(invited.disabled() && invited.done(), "Davet gönderildi");
        Row full = row(FriendMenu.plan(new Input(false, true, true, false, false, Offer.FULL, false, true, false)),
            Action.INVITE);
        assertTrue(full.disabled() && !full.done());
        // With world sharing there even an older launcher without "people" offers it.
        assertTrue(row(FriendMenu.plan(new Input(false, false, true, false, false, Offer.READY, false, false, false)),
            Action.INVITE) != null);
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
    void withoutWorldSharingNoWorldInviteIsOffered() {
        WorldInvites.install(null);
        assertEquals(Offer.NONE, WorldInvites.offer("f1"));
        assertFalse(WorldInvites.guest("f1"));
        assertNull(WorldInvites.invite("f1"));
        assertFalse(WorldInvites.remove("f1"));
        List<String> asked = new ArrayList<>();
        WorldInvites.install(new WorldInvites.Hook() {
            @Override
            public Offer offer(String friendId) {
                return friendId.equals("f1") ? Offer.READY : null;
            }

            @Override
            public String invite(String friendId) {
                asked.add("invite " + friendId);
                return friendId.equals("f2") ? "Dünyan şu an açılamıyor." : null;
            }

            @Override
            public boolean guest(String friendId) {
                return friendId.equals("f3");
            }

            @Override
            public void remove(String friendId) {
                asked.add("remove " + friendId);
            }
        });
        try {
            assertEquals(Offer.READY, WorldInvites.offer("f1"));
            assertEquals(Offer.NONE, WorldInvites.offer("f9"), "a hook's null reads as none");
            assertNull(WorldInvites.invite("f1"));
            assertEquals("Dünyan şu an açılamıyor.", WorldInvites.invite("f2"));
            assertTrue(WorldInvites.guest("f3"));
            assertTrue(WorldInvites.remove("f3"));
            assertEquals(List.of("invite f1", "invite f2", "remove f3"), asked);
        } finally {
            WorldInvites.install(null);
        }
    }
}
