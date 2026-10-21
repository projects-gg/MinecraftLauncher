package gg.projects.overlay.connect;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class ServerTargetTest {
    @Test
    void readsLauncherFields() {
        assertEquals(new ServerTarget("play.projects.gg", 25565), ServerTarget.of("play.projects.gg", 25565));
        assertEquals(new ServerTarget("mc.ornek.net", 25570), ServerTarget.of(" MC.Ornek.net ", 25570));
        // The port left out: the host may carry it.
        assertEquals(new ServerTarget("mc.ornek.net", 25570), ServerTarget.of("mc.ornek.net:25570", -1));
        assertEquals(new ServerTarget("mc.ornek.net", 25565), ServerTarget.of("mc.ornek.net", -1));
        assertEquals(new ServerTarget("::1", 25599), ServerTarget.of("[::1]", 25599));
    }

    @Test
    void refusesWhatIsNotAServer() {
        assertNull(ServerTarget.of(null, 25565));
        assertNull(ServerTarget.of("", 25565));
        assertNull(ServerTarget.of("mc.ornek.net", 0));
        assertNull(ServerTarget.of("mc.ornek.net", 70000));
        assertNull(ServerTarget.of("mc ornek net", 25565));
        assertNull(ServerTarget.of("mc.ornek.net/yol", 25565));
        assertNull(ServerTarget.of("http://mc.ornek.net", 25565));
        assertNull(ServerTarget.of("mc.ornek.net:25570", 25570));
        assertNull(ServerTarget.of("mc.ornek.net:-5", -1));
        assertNull(ServerTarget.of("mc.ornek.net:abc", -1));
        assertNull(ServerTarget.of(".ornek.net", 25565));
        assertNull(ServerTarget.of("a".repeat(254), 25565));
    }

    @Test
    void parsesTheGamesAddresses() {
        assertEquals(new ServerTarget("localhost", 25565), ServerTarget.parse("localhost"));
        assertEquals(new ServerTarget("127.0.0.1", 25599), ServerTarget.parse("127.0.0.1:25599"));
        assertEquals(new ServerTarget("::1", 25599), ServerTarget.parse("[::1]:25599"));
        assertEquals(new ServerTarget("2001:db8::1", 25565), ServerTarget.parse("2001:db8::1"));
        assertNull(ServerTarget.parse("[::1"));
        assertNull(ServerTarget.parse("[::1]x"));
        assertNull(ServerTarget.parse(null));
    }

    @Test
    void projectsIsOneServerUnderManyNames() {
        ServerTarget play = ServerTarget.of("play.projects.gg", 25565);
        assertTrue(play.projects());
        assertTrue(play.sameServer(ServerTarget.parse("mc.projects.gg")));
        assertTrue(play.sameServer(ServerTarget.parse("projects.gg:25566")));
        assertFalse(play.sameServer(ServerTarget.parse("projects.gg.ornek.net")));
        assertFalse(ServerTarget.parse("notprojects.gg").projects());
        assertFalse(play.sameServer(null));
    }

    @Test
    void otherServersMatchByHostAndPort() {
        ServerTarget a = ServerTarget.parse("mc.ornek.net");
        assertTrue(a.sameServer(ServerTarget.parse("MC.ORNEK.NET:25565")));
        assertFalse(a.sameServer(ServerTarget.parse("mc.ornek.net:25570")));
        assertFalse(a.sameServer(ServerTarget.parse("ornek.net")));
    }

    @Test
    void addressAndNameForThePlayer() {
        assertEquals("Projects", ServerTarget.parse("mc.projects.gg").displayName());
        assertEquals("mc.ornek.net", ServerTarget.parse("mc.ornek.net:25565").displayName());
        assertEquals("mc.ornek.net:25570", ServerTarget.parse("mc.ornek.net:25570").address());
        assertEquals("[::1]:25599", ServerTarget.parse("[::1]:25599").address());
    }
}
