package gg.projects.overlay.host;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

/** Who gets into a shared world, and when (PROTOCOL.md §7.3). */
class HostSessionTest {
    private static final String NOTCH = "1UbirWHKbvkTXKMZNmDDug";
    private static final String JEB = "n7ppm8zILloa1X-qpWnqVA";
    private static final String CAN = "K9sApXlhlewD1zz01lv8HA";
    private static final String KEY = "sW9hzU3c";

    private HostSession session;

    @BeforeEach
    void setUp() {
        GuestBindings bindings = GuestBindings.parse(Vectors.text("binding/projects-guests.example.json"), KEY);
        session = new HostSession(new HostSession.Publish("p1", "survival", false, 2, KEY), "Ayse", bindings);
    }

    private static GuestIdentity id(long conn, String guest, String name, boolean premium, String friend) {
        return new GuestIdentity("p1", conn, guest, name, premium, friend);
    }

    private HostSession.Guest in(long conn, String guest, String name, String friend) {
        GuestIdentity identity = id(conn, guest, name, false, friend);
        HostSession.Guest g = session.admitHeader(identity, "channel-" + conn);
        assertNotNull(g);
        assertNull(session.hello(identity, name, false));
        UUID uuid = UUID.nameUUIDFromBytes(name.getBytes());
        assertNull(session.verify(identity, name, uuid, false));
        session.joined(uuid, 1_800_000_000L);
        return g;
    }

    @Test
    void aHeaderIsForThisPublishAndItsConnectionNumberOnlyOnce() {
        assertNotNull(session.admitHeader(id(1, "g1", "Notch", false, NOTCH), "a"));
        assertNull(session.admitHeader(id(1, "g2", "jeb_", false, JEB), "b"));
        assertNull(session.admitHeader(new GuestIdentity("p0", 2, "g1", "Notch", false, NOTCH), "c"));
        session.close();
        assertNull(session.admitHeader(id(3, "g1", "Notch", false, NOTCH), "d"));
    }

    @Test
    void theHostsNameNeverLogsInFromOutside() {
        GuestIdentity ayse = id(1, "g1", "AYSE", false, CAN);
        session.admitHeader(ayse, "a");
        assertEquals(KickText.Refusal.NAME, session.hello(ayse, "AYSE", false));
    }

    @Test
    void theNameMustBeTheOneTheLauncherVouchedForAndAValidOne() {
        GuestIdentity notch = id(1, "g1", "Notch", false, NOTCH);
        session.admitHeader(notch, "a");
        assertEquals(KickText.Refusal.NAME, session.hello(notch, "Herobrine", false));
        GuestIdentity bad = id(2, "g2", "Ayşe_1", false, JEB);
        session.admitHeader(bad, "b");
        assertEquals(KickText.Refusal.NAME, session.hello(bad, "Ayşe_1", false));
        GuestIdentity again = id(3, "g1", "notch", false, NOTCH);
        session.admitHeader(again, "c");
        assertNull(session.hello(again, "notch", false));
    }

    @Test
    void aNameBoundToAnotherFriendIsTaken() {
        GuestIdentity thief = id(1, "g3", "Notch", false, CAN);
        session.admitHeader(thief, "a");
        assertEquals(KickText.Refusal.NAME_TAKEN, session.hello(thief, "Notch", false));
    }

    @Test
    void aNameAlreadyInTheWorldOrInAnotherLoginIsRefused() {
        in(1, "g1", "Notch", NOTCH);
        GuestIdentity twice = id(2, "g1", "Notch", false, NOTCH);
        session.admitHeader(twice, "b");
        assertEquals(KickText.Refusal.NAME, session.hello(twice, "Notch", true));
        assertEquals(KickText.Refusal.NAME, session.hello(twice, "Notch", false));
    }

    @Test
    void theWorldHoldsAsManyGuestsAsTheLauncherSaid() {
        in(1, "g1", "Notch", NOTCH);
        in(2, "g2", "jeb_", JEB);
        GuestIdentity third = id(3, "g3", "Dinnerbone", false, CAN);
        session.admitHeader(third, "c");
        assertEquals(KickText.Refusal.FULL, session.hello(third, "Dinnerbone", false));
        // One leaves: there is room again.
        session.closedConnection(id(2, "g2", "jeb_", false, JEB));
        assertNull(session.hello(third, "Dinnerbone", false));
    }

    @Test
    void theRoomIsCountedAgainAsTheLoginEnds() {
        in(1, "g1", "Notch", NOTCH);
        GuestIdentity a = id(2, "g2", "jeb_", false, JEB);
        GuestIdentity b = id(3, "g3", "Dinnerbone", false, CAN);
        session.admitHeader(a, "a");
        session.admitHeader(b, "b");
        assertNull(session.hello(a, "jeb_", false));
        assertNull(session.hello(b, "Dinnerbone", false));
        assertNull(session.verify(a, "jeb_", UUID.randomUUID(), false));
        assertEquals(KickText.Refusal.FULL, session.verify(b, "Dinnerbone", UUID.randomUUID(), false));
    }

    @Test
    void aMicrosoftAccountKeepsTheNameMojangVouchedFor() {
        GuestIdentity premium = id(1, "g1", "Notch", true, NOTCH);
        session.admitHeader(premium, "a");
        assertNull(session.hello(premium, "notch", false));
        assertEquals(KickText.Refusal.NAME, session.verify(premium, "Dinnerbone", UUID.randomUUID(), false));
        assertNull(session.verify(premium, "Notch", UUID.randomUUID(), false));
    }

    @Test
    void aRemovedGuestNeverGetsBackInThisSession() {
        HostSession.Guest g = in(1, "g1", "Notch", NOTCH);
        List<HostSession.Guest> off = session.remove("g1");
        assertEquals(List.of(g), off);
        assertTrue(g.kicked());
        assertTrue(session.removed("g1"));
        GuestIdentity back = id(2, "g1", "Notch", false, NOTCH);
        session.admitHeader(back, "b");
        assertEquals(KickText.Refusal.REMOVED, session.hello(back, "Notch", false));
        HostSession.Left left = session.closedConnection(g.identity);
        assertTrue(left.wasIn());
        assertTrue(left.kicked());
    }

    @Test
    void joiningBindsTheNameOnceAndLeavingSaysWhetherItWasIn() {
        GuestIdentity can = id(1, "g3", "Dinnerbone", false, CAN);
        session.admitHeader(can, "a");
        assertNull(session.hello(can, "Dinnerbone", false));
        UUID uuid = UUID.randomUUID();
        assertNull(session.verify(can, "Dinnerbone", uuid, false));
        assertEquals(HostSession.Stage.ADMITTED, session.guest(can).stage());
        assertSame(session.guest(can), session.byUuid(uuid));
        assertTrue(session.joined(uuid, 1_800_000_000L));
        assertFalse(session.joined(uuid, 1_800_000_100L));
        assertTrue(session.bindingsJson().contains("Dinnerbone"));
        assertEquals(1, session.inWorld().size());
        HostSession.Left left = session.closedConnection(can);
        assertTrue(left.wasIn());
        assertFalse(left.kicked());
        assertNull(session.closedConnection(can));
        // A connection that never got in leaves without a word.
        GuestIdentity quick = id(2, "g4", "Quick", false, CAN);
        session.admitHeader(quick, "b");
        assertFalse(session.closedConnection(quick).wasIn());
    }

    @Test
    void closingSendsEveryoneOffOnce() {
        in(1, "g1", "Notch", NOTCH);
        session.admitHeader(id(2, "g2", "jeb_", false, JEB), "b");
        List<HostSession.Guest> off = session.close();
        assertEquals(2, off.size());
        assertTrue(off.stream().allMatch(HostSession.Guest::kicked));
        assertTrue(session.close().isEmpty());
        assertTrue(session.closed());
        assertEquals(KickText.Refusal.CLOSED, session.hello(id(2, "g2", "jeb_", false, JEB), "jeb_", false));
    }
}
