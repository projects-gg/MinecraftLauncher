package gg.projects.overlay.people;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.people.FriendMenu.Action;
import gg.projects.overlay.people.FriendMenu.AddFriend;
import gg.projects.overlay.people.FriendMenu.Input;
import gg.projects.overlay.people.FriendMenu.Row;
import gg.projects.overlay.people.FriendMenu.Team;
import gg.projects.overlay.people.FriendMenu.Voice;
import gg.projects.overlay.people.WorldInvites.Offer;
import gg.projects.overlay.voice.VoiceState;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * VOICE-O: voice chat's rows (the launcher's menus.ts personMenu): someone in the player's room gets their volume and
 * "Benim için sustur" first, then the player's role's rows; "Ara" or "Odaya davet et" stands beside "Mesaj gönder".
 */
class FriendMenuVoiceTest {
    /** A friend on Projects with chat on and a Discord profile. */
    private static Input friend(Voice voice, boolean online) {
        return new Input(true, true, true, online, true, false, Offer.NONE, false, true, true, true, false, false,
            false, Team.NONE, 0, AddFriend.HIDDEN, null, voice);
    }

    /** Someone in the room the lists do not know: only voice chat's rows. */
    private static Input stranger(Voice voice) {
        return new Input(false, false, false, true, false, false, Offer.NONE, false, false, false, false, false, false,
            false, Team.NONE, 0, AddFriend.HIDDEN, null, voice);
    }

    private static Voice inRoom(boolean canMute, boolean canKick, boolean canMod) {
        return new Voice(VoiceState.Offer.NONE, true, false, false, false, canMute, canKick, canMod);
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
    void someoneInTheRoomLeadsWithTheirVolumeThenTheRolesRows() {
        List<Row> plan = FriendMenu.plan(friend(inRoom(true, true, true), true));
        List<Action> expected = new ArrayList<>(List.of(Action.VOLUME, Action.MUTE_FOR_ME));
        expected.add(null);
        expected.addAll(List.of(Action.VOICE_MUTE, Action.VOICE_KICK, Action.VOICE_MOD));
        expected.add(null);
        expected.addAll(List.of(Action.JOIN, Action.INVITE, Action.MESSAGE, Action.DISCORD));
        expected.add(null);
        expected.addAll(List.of(Action.FAVORITE, Action.ALERT));
        expected.add(null);
        expected.addAll(List.of(Action.UNFRIEND, Action.BLOCK, Action.REPORT));
        assertEquals(expected, actions(plan));
        assertTrue(row(plan, Action.VOLUME).sub(), "the steps open as a list");
        assertTrue(Action.VOICE_KICK.asks(), "one sentence first");
        assertFalse(Action.VOICE_MUTE.asks() || Action.CALL.asks());
    }

    @Test
    void theRowsSayWhatIsSetNow() {
        Voice set = new Voice(VoiceState.Offer.NONE, true, true, true, true, true, false, true);
        List<Row> plan = FriendMenu.plan(stranger(set));
        assertEquals(List.of(Action.VOLUME, Action.MUTE_FOR_ME, Action.VOICE_UNMUTE, Action.VOICE_UNMOD),
            actions(plan).stream().filter(a -> a != null).toList());
        assertTrue(row(plan, Action.MUTE_FOR_ME).on());
        // Nobody the lists know: no friend rows, no block or report the launcher would refuse.
        assertFalse(actions(plan).contains(Action.BLOCK));
        // A plain member's menu: only the player's own choices.
        assertEquals(List.of(Action.VOLUME, Action.MUTE_FOR_ME),
            actions(FriendMenu.plan(stranger(inRoom(false, false, false)))));
    }

    @Test
    void callingStandsBesideTheMessage() {
        List<Action> call = actions(FriendMenu.plan(friend(
            new Voice(VoiceState.Offer.CALL, false, false, false, false, false, false, false), true)));
        assertEquals(List.of(Action.JOIN, Action.INVITE, Action.MESSAGE, Action.CALL, Action.DISCORD),
            call.subList(0, 5));
        List<Row> ringing = FriendMenu.plan(friend(
            new Voice(VoiceState.Offer.RINGING, false, false, false, false, false, false, false), true));
        Row invite = row(ringing, Action.CALL_INVITE);
        assertTrue(invite.disabled() && invite.done(), "\"Aranıyor…\", soft");
        Row full = row(FriendMenu.plan(friend(
            new Voice(VoiceState.Offer.FULL, false, false, false, false, false, false, false), true)),
            Action.CALL_INVITE);
        assertTrue(full.disabled() && !full.done());
        Row offline = row(FriendMenu.plan(friend(
            new Voice(VoiceState.Offer.INVITE, false, false, false, false, false, false, false), false)),
            Action.CALL_INVITE);
        assertTrue(offline.disabled(), "the launcher is away");
        List<Action> none = actions(FriendMenu.plan(friend(null, true)));
        assertFalse(none.contains(Action.CALL) || none.contains(Action.CALL_INVITE) || none.contains(Action.VOLUME));
    }
}
