package gg.projects.overlay.connect;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** A friend's world (HOST1, H12): the launcher's local way there, read as its label. */
class ServerTargetLabelTest {
    @Test
    void aLabelIsWhatThePlayerReadsNeverTheLocalAddress() {
        ServerTarget world = ServerTarget.of("127.0.0.1", 51234).withLabel("Arkadaş dünyası: Ayşe");
        assertEquals("Arkadaş dünyası: Ayşe", world.displayName());
        assertEquals("127.0.0.1:51234", world.address());
        assertTrue(world.sameServer(ServerTarget.parse("127.0.0.1:51234")));
        assertNull(ServerTarget.of("127.0.0.1", 51234).withLabel("  ").label());
        assertEquals("Projects", ServerTarget.parse("play.projects.gg").withLabel(null).displayName());
    }
}
